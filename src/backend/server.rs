#![allow(unused_imports)]

use std::path::PathBuf;
use std::sync::Arc;

use arc_swap::ArcSwap;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress as ProgressNotification;
use tower_lsp::lsp_types::request::WorkDoneProgressCreate;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, async_trait};

use php_ast::{
    ClassMember, ClassMemberKind, EnumMember, EnumMemberKind, ExprKind, NamespaceBody, Stmt,
    StmtKind,
};

use super::panic_guard::{guard_async, guard_async_result};
use crate::completion::{CompletionCtx, filtered_completions_at};
use crate::document::ast::{ParsedDoc, str_offset};
use crate::document::document_store::DocumentStore;
use crate::document::open_files::{OpenFiles, compute_open_file_diagnostics};
use crate::editing::file_rename::{use_edits_for_delete, use_edits_for_rename};
use crate::editing::use_import::{build_use_import_edit, find_fqn_for_class};
use crate::hover::{
    class_hover_from_index, docs_for_symbol_from_index, extract_static_class_before_cursor,
    hover_info_with_maps, method_hover_from_index, signature_for_symbol_from_index,
};
use crate::index::workspace_scan::{scan_workspace, send_refresh_requests};
use crate::lang::autoload::Psr4Map;
use crate::lang::config::LspConfig;
use crate::lang::phpstorm_meta::PhpStormMeta;
use crate::navigation::symbols::{
    document_symbols, resolve_workspace_symbol, workspace_symbols_from_workspace,
};
use crate::text::{fqn_short_name, word_at_position};

use crate::actions::extract_action::extract_variable_actions;
use crate::actions::extract_constant_action::extract_constant_actions;
use crate::actions::extract_method_action::extract_method_actions;
use crate::actions::generate_action::{
    generate_constructor_actions, generate_getters_setters_actions,
};
use crate::actions::implement_action::implement_missing_actions;
use crate::actions::inline_action::inline_variable_actions;
use crate::actions::phpdoc_action::phpdoc_actions;
use crate::actions::promote_action::promote_constructor_actions;
use crate::actions::type_action::add_return_type_actions;

use crate::navigation::call_hierarchy::{
    incoming_calls, outgoing_calls_indexed, prepare_call_hierarchy_indexed,
};
use crate::navigation::declaration::{goto_declaration, goto_declaration_from_index};
use crate::navigation::definition::{
    find_declaration_range, find_method_in_class_hierarchy, find_method_range_in_class,
    goto_definition,
};
use crate::navigation::implementation::{
    find_implementations, find_implementations_from_workspace,
    find_method_implementations_from_workspace,
};
use crate::navigation::moniker::moniker_at;
use crate::navigation::references::{
    SymbolKind, find_constructor_references, find_references, find_references_with_target,
};
use crate::navigation::type_definition::{goto_type_definition, goto_type_definition_from_index};
use crate::navigation::type_hierarchy::{
    prepare_type_hierarchy_from_workspace, subtypes_of_from_workspace, supertypes_of_from_workspace,
};

use crate::analysis::code_lens::code_lenses;
use crate::analysis::diagnostics::{
    diagnostics_from_doc, merge_file_diagnostics, parse_document, parse_document_no_diags,
};
use crate::analysis::document_highlight::document_highlights;
use crate::analysis::inlay_hints::inlay_hints;
use crate::analysis::inline_value::inline_values_in_range;
use crate::analysis::semantic_tokens::{
    compute_token_delta, legend, semantic_tokens, semantic_tokens_range, token_hash,
};

use crate::editing::document_link::document_links;
use crate::editing::folding::folding_ranges;
use crate::editing::formatting::{format_document, format_range};
use crate::editing::on_type_format::on_type_format;
use crate::editing::organize_imports::organize_imports_action;
use crate::editing::rename::{prepare_rename, rename, rename_property, rename_variable};
use crate::editing::selection_range::selection_ranges;
use crate::editing::signature_help::signature_help;

use super::helpers::{
    DEFERRED_ACTION_TAGS, class_name_at_construct_decl, cursor_is_on_constant_decl,
    cursor_is_on_method_decl, cursor_is_on_property_decl, defer_actions, is_after_arrow,
    php_file_op, promoted_property_at_cursor, range_within, run_phpunit, symbol_kind_at,
};
use super::{
    Backend, IndexReadyNotification, compute_dependent_publishes_owned,
    compute_diagnostic_result_id, publish_with_dependents, resolve_reference_symbol,
};

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.handle_initialize(params).await
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.handle_initialized(_params).await
    }

    async fn did_change_configuration(&self, _params: DidChangeConfigurationParams) {
        // Pull the current configuration from the client rather than parsing the
        // (often-null) params.settings, which not all clients populate.
        let items = vec![ConfigurationItem {
            scope_uri: None,
            section: Some("php-lsp".to_string()),
        }];
        if let Ok(values) = self.client.configuration(items).await
            && let Some(value) = values.into_iter().next()
        {
            let roots = self.root_paths.load_full();

            // Re-read .php-lsp.json so a user who edits the file and then
            // triggers a configuration reload picks up the latest values.
            let file_cfg = crate::lang::autoload::load_project_config_json(&roots);

            if let Some(ver) = value.get("phpVersion").and_then(|v| v.as_str())
                && !crate::lang::autoload::is_valid_php_version(ver)
            {
                self.client
                    .log_message(
                        tower_lsp::lsp_types::MessageType::WARNING,
                        format!(
                            "php-lsp: unsupported phpVersion {ver:?} — valid values: {}",
                            crate::lang::autoload::SUPPORTED_PHP_VERSIONS.join(", ")
                        ),
                    )
                    .await;
            }

            let file_obj = file_cfg.as_ref().filter(|v| v.is_object());
            let merged = LspConfig::merge_project_configs(file_obj, Some(&value));
            let mut cfg = LspConfig::from_value(&merged);

            // Resolve the PHP version and log what was chosen and why.
            let (ver, source) = self.resolve_php_version(cfg.php_version.as_deref());
            self.client
                .log_message(
                    tower_lsp::lsp_types::MessageType::INFO,
                    format!("php-lsp: using PHP {ver} ({source})"),
                )
                .await;
            // Clamp unsupported versions to the nearest supported one and warn.
            let ver = if source != "set by editor"
                && !crate::lang::autoload::is_valid_php_version(&ver)
            {
                let clamped = crate::lang::autoload::clamp_php_version(&ver);
                self.client
                    .show_message(
                        tower_lsp::lsp_types::MessageType::WARNING,
                        format!(
                            "php-lsp: detected PHP {ver} is outside the supported range ({}); \
                             using PHP {clamped} for analysis",
                            crate::lang::autoload::SUPPORTED_PHP_VERSIONS.join(", ")
                        ),
                    )
                    .await;
                clamped.to_string()
            } else {
                ver
            };
            cfg.php_version = Some(ver.clone());
            if let Ok(pv) = ver.parse::<mir_analyzer::PhpVersion>() {
                self.docs.set_php_version(pv);
            }
            self.config.store(Arc::new(cfg));
            send_refresh_requests(&self.client).await;
        }
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        // Remove folders from our tracked roots.
        {
            let mut roots = (**self.root_paths.load()).clone();
            for removed in &params.event.removed {
                if let Ok(path) = removed.uri.to_file_path() {
                    roots.retain(|r| r != &path);
                }
            }
            self.root_paths.store(Arc::new(roots));
        }

        // Add new folders and kick off background scans for each.
        let (exclude_paths, include_paths, max_indexed_files) = {
            let cfg = self.config.load();
            (
                cfg.exclude_paths.clone(),
                cfg.include_paths.clone(),
                cfg.max_indexed_files,
            )
        };
        for added in &params.event.added {
            if let Ok(path) = added.uri.to_file_path() {
                let is_new = {
                    let mut roots = (**self.root_paths.load()).clone();
                    if !roots.contains(&path) {
                        roots.push(path.clone());
                        self.root_paths.store(Arc::new(roots));
                        true
                    } else {
                        false
                    }
                };
                if is_new {
                    let docs = Arc::clone(&self.docs);
                    let open_files = self.open_files.clone();
                    let ex = exclude_paths.clone();
                    let ip = include_paths.clone();
                    let path_clone = path.clone();
                    let client = self.client.clone();
                    tokio::spawn(async move {
                        let cache = crate::index::cache::WorkspaceCache::new(&path_clone);
                        scan_workspace(
                            path_clone,
                            docs,
                            open_files,
                            cache,
                            &ex,
                            &ip,
                            max_indexed_files,
                        )
                        .await;
                        send_refresh_requests(&client).await;
                    });
                }
            }
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        guard_async("did_open", async move {
            let uri = params.text_document.uri;
            let text = params.text_document.text;

            // Store text immediately so other features work while parsing.
            // This also mirrors the new text into salsa, so the codebase query
            // sees it when semantic_diagnostics runs below.
            self.set_open_text(uri.clone(), text);

            // Seed parse diagnostics from the salsa-cached doc. On the fast
            // path this is a lock-free DashMap lookup — no re-parse.
            let parse_diags = self
                .docs
                .get_doc_salsa(&uri)
                .map(|doc| diagnostics_from_doc(&doc))
                .unwrap_or_default();
            self.set_parse_diagnostics(&uri, parse_diags);

            publish_with_dependents(
                self.client.clone(),
                Arc::clone(&self.docs),
                self.open_files.clone(),
                uri,
                self.config.load().diagnostics.clone(),
            )
            .await;
        })
        .await
    }

    #[tracing::instrument(skip_all)]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        guard_async("did_change", async move {
            let uri = params.text_document.uri;
            // Incremental sync: apply changes in order to the live buffer.
            // Each ranged change refers to the document state produced by the
            // previous one; a change without a range is a full-document
            // replacement (clients may still send those under INCREMENTAL).
            let mut updated: Option<String> = None;
            for change in params.content_changes {
                match change.range {
                    None => updated = Some(change.text),
                    Some(range) => {
                        let mut cur = match updated.take() {
                            Some(t) => t,
                            None => self.get_open_text(&uri).unwrap_or_default(),
                        };
                        crate::text::apply_content_change(&mut cur, range, &change.text);
                        updated = Some(cur);
                    }
                }
            }
            let Some(text) = updated else { return };

            // Store text immediately and capture the version token.
            // Features (completion, hover, …) see the new text instantly while
            // the parse runs in the background.
            let version = self.set_open_text(uri.clone(), text.clone());

            let docs = Arc::clone(&self.docs);
            let open_files = self.open_files.clone();
            let client = self.client.clone();
            let diag_cfg = self.config.load().diagnostics.clone();
            tokio::spawn(async move {
                // 100 ms debounce: if another edit arrives before we parse,
                // the version gate below will discard this result.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                // Skip the expensive parse+analyze if a newer edit already
                // superseded this one. Collapses N rapid keystrokes into 1
                // spawn_blocking call instead of N.
                if open_files.current_version(&uri) != Some(version) {
                    return;
                }

                let (_doc, parse_diags) =
                    tokio::task::spawn_blocking(move || parse_document(&text))
                        .await
                        .unwrap_or_else(|_| (ParsedDoc::default(), vec![]));

                // Only apply if no newer edit arrived while we were parsing.
                if open_files.current_version(&uri) == Some(version) {
                    open_files.set_parse_diagnostics(&uri, parse_diags);
                    publish_with_dependents(client, docs, open_files, uri, diag_cfg).await;
                }
            });
        })
        .await
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.close_open_file(&uri);
        // Clear editor diagnostics; the file stays indexed for cross-file features
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn will_save(&self, _params: WillSaveTextDocumentParams) {}

    async fn will_save_wait_until(
        &self,
        params: WillSaveTextDocumentParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let source = self
            .get_open_text(&params.text_document.uri)
            .unwrap_or_default();
        Ok(format_document(&source))
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        // Re-publish diagnostics on save so editors that defer diagnostics
        // until save (rather than on every keystroke) see up-to-date results.
        // Must include semantic diagnostics — publishDiagnostics replaces the
        // prior set entirely, so omitting them would clear errors the editor
        // showed after the last did_change.
        let diag_cfg = self.config.load().diagnostics.clone();
        let all = compute_open_file_diagnostics(&self.docs, &self.open_files, &uri, &diag_cfg);
        self.client
            .publish_diagnostics(uri.clone(), all, None)
            .await;

        // Persist the FileIndex to the disk cache so that a server restart
        // can skip re-parsing this file even for edits that happened between
        // workspace scans. Use the same stat-based key as the workspace scan
        // so the entry is found on the next cold start.
        //
        // Both the stat and the content are read from disk inside the blocking
        // task so they come from the same on-disk snapshot. Using the editor
        // buffer (open_files text) instead would risk writing an index derived
        // from unsaved edits typed after the save but before this task runs,
        // producing a cache entry where the key (disk stat) and the value
        // (index of newer buffer content) describe different file versions.
        if let (Some(root), Ok(path)) =
            (self.root_paths.load().first().cloned(), uri.to_file_path())
        {
            tokio::task::spawn_blocking(move || {
                let Some(cache) = crate::index::cache::WorkspaceCache::new(&root) else {
                    return;
                };
                // Stat before read to match workspace_scan ordering and
                // minimise the TOCTOU window.
                let Ok(meta) = std::fs::metadata(&path) else {
                    return;
                };
                let Ok(text) = std::fs::read_to_string(&path) else {
                    return;
                };
                let mtime_secs = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let key = crate::index::cache::WorkspaceCache::key_for_stat(
                    uri.as_str(),
                    mtime_secs,
                    meta.len(),
                );
                let doc = parse_document_no_diags(&text);
                let index = crate::index::file_index::FileIndex::extract(&doc);
                let _ = cache.write(&key, &index);
            });
        }
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for change in params.changes {
            match change.typ {
                FileChangeType::CREATED | FileChangeType::CHANGED => {
                    if let Ok(path) = change.uri.to_file_path()
                        && let Ok(text) = tokio::fs::read_to_string(&path).await
                    {
                        // Salsa path: ingest_from_doc mirrors the new text into
                        // the SourceFile input. On the next codebase() call,
                        // salsa re-runs file_definitions for this file and the
                        // aggregator re-folds — no manual remove/collect/finalize.
                        let doc = parse_document_no_diags(&text);
                        self.ingest_from_doc_if_not_open(change.uri.clone(), &doc);
                    }
                }
                FileChangeType::DELETED => {
                    self.docs.remove(&change.uri);
                }
                _ => {}
            }
        }
        // File changes may affect cross-file features — refresh all live editors.
        send_refresh_requests(&self.client).await;
    }

    #[tracing::instrument(skip_all)]
    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        guard_async_result("completion", async move {
            let uri = &params.text_document_position.text_document.uri;
            let position = params.text_document_position.position;
            let source = self.get_open_text(uri).unwrap_or_default();
            let doc = match self.get_doc(uri) {
                Some(d) => d,
                None => return Ok(Some(CompletionResponse::Array(vec![]))),
            };
            let other_docs: Vec<Arc<ParsedDoc>> = self
                .docs
                .other_docs(uri, &self.open_urls())
                .into_iter()
                .map(|(_, d)| d)
                .collect();
            let trigger = params
                .context
                .as_ref()
                .and_then(|c| c.trigger_character.as_deref());
            let meta_loaded = self.meta.load();
            let meta_opt = if meta_loaded.is_empty() {
                None
            } else {
                Some(&**meta_loaded)
            };
            let imports = self.file_imports(uri);
            let wi = self.workspace_index_async().await;
            let docs_for_lookup = Arc::clone(&self.docs);
            let find_class_doc_fn = move |name: &str| -> Option<Arc<ParsedDoc>> {
                let cr = *wi.classes_by_name.get(name)?.first()?;
                let (uri, _) = wi.at(cr)?;
                docs_for_lookup.get_doc_salsa(uri)
            };
            let analysis = self.cached_analysis_async(uri).await;
            // Cross-request TypeMap cache: rebuilt only when the document text
            // (or PHPStorm meta) changes, instead of one full AST walk per
            // completion request.
            let docs_for_tm = Arc::clone(&self.docs);
            let doc_for_tm = Arc::clone(&doc);
            let uri_for_tm = uri.clone();
            let get_type_map =
                move || docs_for_tm.cached_type_map(&uri_for_tm, &doc_for_tm, meta_opt);
            let session = self
                .docs
                .analysis_session(self.docs.workspace_php_version());
            let ctx = CompletionCtx {
                source: Some(&source),
                position: Some(position),
                meta: meta_opt,
                doc_uri: Some(uri),
                file_imports: Some(&imports),
                find_class_doc: Some(&find_class_doc_fn),
                analysis: analysis.as_deref(),
                type_map: Some(&get_type_map),
                session: Some(session),
            };
            Ok(Some(CompletionResponse::Array(filtered_completions_at(
                &doc,
                &other_docs,
                trigger,
                &ctx,
            ))))
        })
        .await
    }

    async fn completion_resolve(&self, mut item: CompletionItem) -> Result<CompletionItem> {
        if item.documentation.is_some() && item.detail.is_some() {
            return Ok(item);
        }
        // Strip trailing ':' from named-argument labels (e.g. "param:") before lookup.
        let name = item.label.trim_end_matches(':');
        let all_indexes = self.docs.all_indexes();
        if item.detail.is_none()
            && let Some(sig) = signature_for_symbol_from_index(name, &all_indexes)
        {
            item.detail = Some(sig);
        }
        if item.documentation.is_none()
            && let Some(md) = docs_for_symbol_from_index(name, &all_indexes)
        {
            item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }));
        }
        Ok(item)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.handle_goto_definition(params).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        self.handle_references(params).await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let source = self.get_open_text(uri).unwrap_or_default();
        Ok(prepare_rename(&source, params.position).map(PrepareRenameResponse::Range))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let word = match word_at_position(&source, position) {
            Some(w) => w,
            None => return Ok(None),
        };
        if word.starts_with('$') {
            let doc = match self.get_doc(uri) {
                Some(d) => d,
                None => return Ok(None),
            };
            // Cursor on a property declaration (`public int $x`) or a promoted
            // constructor parameter (`private string $x`) — both act as property
            // declarations and must use the cross-file property rename path.
            let prop_name = cursor_is_on_property_decl(&source, &doc.program().stmts, position)
                .or_else(|| promoted_property_at_cursor(&source, &doc.program().stmts, position));
            if let Some(prop_name) = prop_name {
                let all_docs = self.docs.all_docs_for_scan();
                return Ok(Some(rename_property(
                    &prop_name,
                    &params.new_name,
                    &all_docs,
                )));
            }
            Ok(Some(rename_variable(
                &word,
                &params.new_name,
                uri,
                &doc,
                position,
            )))
        } else if is_after_arrow(&source, position) {
            let all_docs = self.docs.all_docs_for_scan();
            Ok(Some(rename_property(&word, &params.new_name, &all_docs)))
        } else {
            let all_docs = self.docs.all_docs_for_scan();
            let doc_opt = self.get_doc(uri);
            let target_fqn: Option<String> = doc_opt.as_ref().map(|doc| {
                let imports = self.file_imports(uri);
                crate::navigation::moniker::resolve_fqn(doc, &word, &imports)
            });
            Ok(Some(rename(
                &word,
                &params.new_name,
                &all_docs,
                target_fqn.as_deref(),
            )))
        }
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let all_indexes = self.docs.all_indexes();
        Ok(signature_help(&source, &doc, position, &all_indexes))
    }

    #[tracing::instrument(skip_all)]
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        guard_async_result("hover", async move {
            let uri = &params.text_document_position_params.text_document.uri;
            let position = params.text_document_position_params.position;
            let source = self.get_open_text(uri).unwrap_or_default();
            let doc = match self.get_doc(uri) {
                Some(d) => d,
                None => return Ok(None),
            };
            let other_docs = self.docs.other_docs(uri, &self.open_urls());
            let other_maps = self.docs.other_symbol_maps(uri, &self.open_urls());
            let analysis = self.cached_analysis_async(uri).await;
            let hover_session = self
                .docs
                .analysis_session(self.docs.workspace_php_version());
            let result = hover_info_with_maps(
                &source,
                &doc,
                analysis.as_deref(),
                position,
                &other_docs,
                &other_maps,
                Some(&hover_session),
            );
            if result.is_some() {
                return Ok(result);
            }
            // Fallback: look up the word in the workspace index so class names in
            // extends clauses and parameter types resolve even when their defining
            // file is never opened.  Also try the alias-resolved name so that
            // `use Foo as Bar` works even when Foo is only in the index.
            if let Some(word) = crate::text::word_at_position(&source, position) {
                let wi = self.workspace_index_async().await;
                // Try the literal word first.
                if let Some(h) = class_hover_from_index(&word, &wi.files) {
                    return Ok(Some(h));
                }
                // Try alias resolution.
                if let Some(resolved) = crate::hover::resolve_use_alias(&doc.program().stmts, &word)
                    && let Some(h) = class_hover_from_index(&resolved, &wi.files)
                {
                    return Ok(Some(h));
                }
                // Try static method hover: `ClassName::method(…)`.
                if let Some(line_text) = source.lines().nth(position.line as usize)
                    && let Some(class_token) =
                        extract_static_class_before_cursor(line_text, position.character as usize)
                {
                    if let Some(h) = method_hover_from_index(&class_token, &word, &wi.files) {
                        return Ok(Some(h));
                    }
                    if let Some(resolved_class) =
                        crate::hover::resolve_use_alias(&doc.program().stmts, &class_token)
                        && let Some(h) = method_hover_from_index(&resolved_class, &word, &wi.files)
                    {
                        return Ok(Some(h));
                    }
                }
            }
            Ok(None)
        })
        .await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        Ok(Some(DocumentSymbolResponse::Nested(document_symbols(
            doc.source(),
            &doc,
        ))))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let ranges = folding_ranges(doc.source(), &doc);
        Ok(if ranges.is_empty() {
            None
        } else {
            Some(ranges)
        })
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let analysis = self.cached_analysis_async(uri).await;
        let wi = self.workspace_index_async().await;
        Ok(Some(inlay_hints(
            doc.source(),
            &doc,
            analysis.as_deref(),
            params.range,
            &wi.files,
        )))
    }

    async fn inlay_hint_resolve(&self, mut item: InlayHint) -> Result<InlayHint> {
        if item.tooltip.is_some() {
            return Ok(item);
        }
        let func_name = item
            .data
            .as_ref()
            .and_then(|d| d.get("php_lsp_fn"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if let Some(name) = func_name {
            let all_indexes = self.docs.all_indexes();
            if let Some(md) = docs_for_symbol_from_index(&name, &all_indexes) {
                item.tooltip = Some(InlayHintTooltip::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }));
            }
        }
        Ok(item)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        // Phase J: read through the salsa-memoized aggregate so repeated
        // workspace-symbol queries (every keystroke in the picker) share the
        // same `Arc` until a file changes.
        let wi = self.workspace_index_async().await;
        let results = workspace_symbols_from_workspace(&params.query, &wi);
        Ok(Some(results))
    }

    async fn symbol_resolve(&self, params: WorkspaceSymbol) -> Result<WorkspaceSymbol> {
        // For resolve, we need the full range from the ParsedDoc of open files.
        let docs = self.docs.docs_for(&self.open_urls());
        Ok(resolve_workspace_symbol(params, &docs))
    }

    #[tracing::instrument(skip_all)]
    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        guard_async_result("semantic_tokens_full", async move {
            let uri = &params.text_document.uri;
            let doc = match self.get_doc(uri) {
                Some(d) => d,
                None => {
                    return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                        result_id: None,
                        data: vec![],
                    })));
                }
            };
            let tokens = semantic_tokens(doc.source(), &doc);
            let result_id = token_hash(&tokens);
            let tokens_arc = Arc::new(tokens);
            self.docs
                .store_token_cache(uri, result_id.clone(), Arc::clone(&tokens_arc));
            let data = Arc::try_unwrap(tokens_arc).unwrap_or_else(|arc| (*arc).clone());
            Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: Some(result_id),
                data,
            })))
        })
        .await
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => {
                return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            }
        };
        let tokens = semantic_tokens_range(doc.source(), &doc, params.range);
        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };

        let new_tokens = Arc::new(semantic_tokens(doc.source(), &doc));
        let new_result_id = token_hash(&new_tokens);
        let prev_id = &params.previous_result_id;

        let result = match self.docs.get_token_cache(uri, prev_id) {
            Some(old_tokens) => {
                let edits = compute_token_delta(&old_tokens, &new_tokens);
                SemanticTokensFullDeltaResult::TokensDelta(SemanticTokensDelta {
                    result_id: Some(new_result_id.clone()),
                    edits,
                })
            }
            // Unknown previous result — fall back to full tokens
            None => SemanticTokensFullDeltaResult::Tokens(SemanticTokens {
                result_id: Some(new_result_id.clone()),
                data: (*new_tokens).clone(),
            }),
        };

        self.docs.store_token_cache(uri, new_result_id, new_tokens);
        Ok(Some(result))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let ranges = selection_ranges(&doc, &params.positions);
        Ok(if ranges.is_empty() {
            None
        } else {
            Some(ranges)
        })
    }

    async fn prepare_call_hierarchy(
        &self,
        params: CallHierarchyPrepareParams,
    ) -> Result<Option<Vec<CallHierarchyItem>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let word = match word_at_position(&source, position) {
            Some(w) => w,
            None => return Ok(None),
        };
        // O(matches) lookup via the aggregate's `decls_by_name` map instead
        // of scanning every workspace doc.
        let wi = self.workspace_index_async().await;
        let docs = Arc::clone(&self.docs);
        let get_doc = move |u: &Url| docs.get_doc_salsa(u);
        Ok(prepare_call_hierarchy_indexed(&word, &wi, &get_doc).map(|item| vec![item]))
    }

    async fn incoming_calls(
        &self,
        params: CallHierarchyIncomingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
        // Genuinely needs every doc (call sites are body-level, not indexed);
        // run the workspace scan on the blocking pool.
        let docs = Arc::clone(&self.docs);
        let item = params.item;
        let calls = tokio::task::spawn_blocking(move || {
            let all_docs = docs.all_docs_for_scan();
            incoming_calls(&item, &all_docs)
        })
        .await
        .unwrap_or_default();
        Ok(if calls.is_empty() { None } else { Some(calls) })
    }

    async fn outgoing_calls(
        &self,
        params: CallHierarchyOutgoingCallsParams,
    ) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
        // Per-callee declaration lookups go through `decls_by_name` — the old
        // path re-scanned the whole workspace once per distinct callee.
        let wi = self.workspace_index_async().await;
        let docs = Arc::clone(&self.docs);
        let item = params.item;
        let calls = tokio::task::spawn_blocking(move || {
            let get_doc = |u: &Url| docs.get_doc_salsa(u);
            outgoing_calls_indexed(&item, &wi, &get_doc)
        })
        .await
        .unwrap_or_default();
        Ok(if calls.is_empty() { None } else { Some(calls) })
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let highlights = document_highlights(&source, &doc, position);
        Ok(if highlights.is_empty() {
            None
        } else {
            Some(highlights)
        })
    }

    async fn linked_editing_range(
        &self,
        params: LinkedEditingRangeParams,
    ) -> Result<Option<LinkedEditingRanges>> {
        self.handle_linked_editing_range(params).await
    }

    async fn goto_implementation(
        &self,
        params: tower_lsp::lsp_types::request::GotoImplementationParams,
    ) -> Result<Option<tower_lsp::lsp_types::request::GotoImplementationResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let imports = self.file_imports(uri);
        let raw_word = crate::text::word_at_position(&source, position).unwrap_or_default();
        // `word_at_position` includes `\` as a word character, so the cursor on
        // a use-statement import (`use A\B\Foo`) returns the full qualified name.
        // Split to recover the short name and treat the rest as the FQN so the
        // workspace index lookup (keyed by short name) still finds subtypes.
        let (word, fqn_owned): (String, Option<String>) = if raw_word.contains('\\') {
            let short = raw_word
                .rsplit('\\')
                .next()
                .unwrap_or(&raw_word)
                .to_string();
            let full = raw_word.trim_start_matches('\\').to_string();
            (short, Some(full))
        } else {
            let fqn = imports.get(&raw_word).cloned();
            (raw_word, fqn)
        };
        let fqn = fqn_owned.as_deref();
        // First pass: open-file ParsedDocs give accurate character positions.
        let open_docs = self.docs.docs_for(&self.open_urls());
        let mut locs = find_implementations(&word, fqn, &open_docs);
        // Second pass: workspace aggregate's `subtypes_of` reverse map.
        let wi = self.workspace_index_async().await;
        if locs.is_empty() {
            locs = find_implementations_from_workspace(&word, fqn, &wi);
        }
        // Third pass: treat word as a method name inside its declaring
        // class/interface — returns concrete overrides in every subtype.
        // Handles cursor on an interface method like `Factory::guard()`.
        if locs.is_empty()
            && let Some(doc) = self.get_doc(uri)
            && let Some(enclosing) =
                crate::types::type_map::enclosing_class_at(&source, &doc, position)
        {
            locs = find_method_implementations_from_workspace(&word, &enclosing, &wi);
        }
        if locs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(GotoDefinitionResponse::Array(locs)))
        }
    }

    async fn goto_declaration(
        &self,
        params: tower_lsp::lsp_types::request::GotoDeclarationParams,
    ) -> Result<Option<tower_lsp::lsp_types::request::GotoDeclarationResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        // First pass: open-file ParsedDocs give accurate character positions.
        let open_docs = self.docs.docs_for(&self.open_urls());
        if let Some(loc) = goto_declaration(&source, &open_docs, position) {
            return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
        }
        // Second pass: background files via FileIndex (line-only positions).
        let all_indexes = self.docs.all_indexes();
        Ok(goto_declaration_from_index(&source, &all_indexes, position)
            .map(GotoDefinitionResponse::Scalar))
    }

    async fn goto_type_definition(
        &self,
        params: tower_lsp::lsp_types::request::GotoTypeDefinitionParams,
    ) -> Result<Option<tower_lsp::lsp_types::request::GotoTypeDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let analysis = self.cached_analysis_async(uri).await;
        // First pass: open-file ParsedDocs give accurate character positions.
        let open_docs = self.docs.docs_for(&self.open_urls());
        let mut results =
            goto_type_definition(&source, &doc, analysis.as_deref(), &open_docs, position);

        // If no results from first pass, try background files via FileIndex (line-only positions).
        if results.is_empty() {
            let all_indexes = self.docs.all_indexes();
            results = goto_type_definition_from_index(
                &source,
                &doc,
                analysis.as_deref(),
                &all_indexes,
                position,
            );
        }

        // Format response: scalar for single result, array for multiple, none for empty
        let response = match results.len() {
            0 => None,
            1 => Some(GotoDefinitionResponse::Scalar(
                results.into_iter().next().unwrap(),
            )),
            _ => Some(GotoDefinitionResponse::Array(results)),
        };
        Ok(response)
    }

    async fn prepare_type_hierarchy(
        &self,
        params: TypeHierarchyPrepareParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        // Phase J: use the salsa-memoized aggregate's `classes_by_name` map.
        let wi = self.workspace_index_async().await;
        Ok(prepare_type_hierarchy_from_workspace(&source, &wi, position).map(|item| vec![item]))
    }

    async fn supertypes(
        &self,
        params: TypeHierarchySupertypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        // Phase J: resolve parents via the aggregate's `classes_by_name` map.
        // Pre-load any direct vendor supertypes via PSR-4 so they appear in the
        // workspace index before the lookup runs.
        let wi = self.workspace_index_async().await;
        let loaded_new = self
            .ensure_direct_supertypes_loaded(&params.item.name, &wi)
            .await;
        let wi = if loaded_new {
            self.workspace_index_async().await
        } else {
            wi
        };
        let result = supertypes_of_from_workspace(&params.item, &wi);
        Ok(if result.is_empty() {
            None
        } else {
            Some(result)
        })
    }

    async fn subtypes(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        // Phase J: O(matches) lookup via the aggregate's `subtypes_of` map.
        let wi = self.workspace_index_async().await;
        let result = subtypes_of_from_workspace(&params.item, &wi);
        Ok(if result.is_empty() {
            None
        } else {
            Some(result)
        })
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        // Reference-count lenses scan every doc per declaration; run the
        // whole computation on the blocking pool.
        let docs = Arc::clone(&self.docs);
        let uri_owned = uri.clone();
        let lenses = tokio::task::spawn_blocking(move || {
            let all_docs = docs.all_docs_for_scan();
            code_lenses(&uri_owned, &doc, &all_docs)
        })
        .await
        .unwrap_or_default();
        Ok(if lenses.is_empty() {
            None
        } else {
            Some(lenses)
        })
    }

    async fn code_lens_resolve(&self, params: CodeLens) -> Result<CodeLens> {
        // Lenses are fully populated by code_lens; nothing to add.
        Ok(params)
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = &params.text_document.uri;
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let links = document_links(uri, &doc, doc.source());
        Ok(if links.is_empty() { None } else { Some(links) })
    }

    async fn document_link_resolve(&self, params: DocumentLink) -> Result<DocumentLink> {
        // Links already carry their target URI; nothing to add.
        Ok(params)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let source = self.get_open_text(uri).unwrap_or_default();
        Ok(format_document(&source))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let source = self.get_open_text(uri).unwrap_or_default();
        Ok(format_range(&source, params.range))
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document_position.text_document.uri;
        let source = self.get_open_text(uri).unwrap_or_default();
        let edits = on_type_format(
            &source,
            params.text_document_position.position,
            &params.ch,
            &params.options,
        );
        Ok(if edits.is_empty() { None } else { Some(edits) })
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "php-lsp.runTest" => {
                // Arguments: [uri_string, "ClassName::methodName"]
                let file_uri = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .and_then(|s| Url::parse(s).ok());
                let filter = params
                    .arguments
                    .get(1)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let root = self.root_paths.load().first().cloned();
                let client = self.client.clone();

                tokio::spawn(async move {
                    run_phpunit(&client, &filter, root.as_deref(), file_uri.as_ref()).await;
                });

                Ok(None)
            }
            _ => Ok(None),
        }
    }

    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        self.handle_will_rename_files(params).await
    }

    async fn did_rename_files(&self, params: RenameFilesParams) {
        self.handle_did_rename_files(params).await
    }

    async fn will_create_files(&self, params: CreateFilesParams) -> Result<Option<WorkspaceEdit>> {
        self.handle_will_create_files(params).await
    }

    async fn did_create_files(&self, params: CreateFilesParams) {
        self.handle_did_create_files(params).await
    }

    /// Before a file is deleted, return workspace edits that remove every
    /// `use` import referencing its PSR-4 class name.
    async fn will_delete_files(&self, params: DeleteFilesParams) -> Result<Option<WorkspaceEdit>> {
        self.handle_will_delete_files(params).await
    }

    async fn did_delete_files(&self, params: DeleteFilesParams) {
        self.handle_did_delete_files(params).await
    }

    async fn moniker(&self, params: MonikerParams) -> Result<Option<Vec<Moniker>>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let imports = self.file_imports(uri);
        Ok(moniker_at(&source, &doc, position, &imports).map(|m| vec![m]))
    }

    async fn inline_value(&self, params: InlineValueParams) -> Result<Option<Vec<InlineValue>>> {
        let uri = &params.text_document.uri;
        let source = self.get_open_text(uri).unwrap_or_default();
        let values = inline_values_in_range(&source, params.range);
        Ok(if values.is_empty() {
            None
        } else {
            Some(values)
        })
    }

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        self.handle_diagnostic(params).await
    }

    async fn workspace_diagnostic(
        &self,
        params: WorkspaceDiagnosticParams,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        self.handle_workspace_diagnostic(params).await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        self.handle_code_action(params).await
    }

    async fn code_action_resolve(&self, item: CodeAction) -> Result<CodeAction> {
        self.handle_code_action_resolve(item).await
    }
}
