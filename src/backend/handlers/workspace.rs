use std::path::PathBuf;
use std::sync::Arc;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress as ProgressNotification;
use tower_lsp::lsp_types::request::WorkDoneProgressCreate;
use tower_lsp::lsp_types::*;

use crate::analysis::semantic_tokens::legend;
use crate::editing::file_rename::{use_edits_for_delete, use_edits_for_rename};
use crate::index::workspace_scan::{scan_workspace, send_refresh_requests};
use crate::lang::autoload::Psr4Map;
use crate::lang::config::LspConfig;
use crate::lang::phpstorm_meta::PhpStormMeta;

use super::super::helpers::php_file_op;
use super::super::{Backend, IndexReadyNotification};

impl Backend {
    pub(crate) async fn handle_initialize(
        &self,
        params: InitializeParams,
    ) -> Result<InitializeResult> {
        {
            let mut roots: Vec<PathBuf> = params
                .workspace_folders
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .filter_map(|f| f.uri.to_file_path().ok())
                .collect();
            if roots.is_empty()
                && let Some(path) = params.root_uri.as_ref().and_then(|u| u.to_file_path().ok())
            {
                roots.push(path);
            }
            self.root_paths.store(Arc::new(roots));
        }

        {
            let opts = params.initialization_options.as_ref();
            let roots = self.root_paths.load_full();
            let file_cfg = crate::lang::autoload::load_project_config_json(&roots);

            if matches!(file_cfg, Some(serde_json::Value::Null)) {
                self.client
                    .log_message(
                        tower_lsp::lsp_types::MessageType::WARNING,
                        "php-lsp: .php-lsp.json contains invalid JSON — ignoring",
                    )
                    .await;
            }

            if let Some(serde_json::Value::Object(ref obj)) = file_cfg
                && let Some(ver) = obj.get("phpVersion").and_then(|v| v.as_str())
                && !crate::lang::autoload::is_valid_php_version(ver)
            {
                self.client
                    .log_message(
                        tower_lsp::lsp_types::MessageType::WARNING,
                        format!(
                            "php-lsp: .php-lsp.json unsupported phpVersion {ver:?} — valid values: {}",
                            crate::lang::autoload::SUPPORTED_PHP_VERSIONS.join(", ")
                        ),
                    )
                    .await;
            }

            if let Some(ver) = opts
                .and_then(|o| o.get("phpVersion"))
                .and_then(|v| v.as_str())
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
            let merged = LspConfig::merge_project_configs(file_obj, opts);
            let mut cfg = LspConfig::from_value(&merged);

            let roots_for_psr4 = (*roots).clone();
            let roots_for_ver = (*roots).clone();
            let explicit_version = cfg.php_version.clone();
            let (psr4_result, ver_result) = tokio::join!(
                tokio::task::spawn_blocking(move || {
                    let mut merged = Psr4Map::empty();
                    for root in &roots_for_psr4 {
                        merged.extend(Psr4Map::load(root));
                    }
                    merged
                }),
                tokio::task::spawn_blocking(move || {
                    crate::lang::autoload::resolve_php_version_from_roots(
                        &roots_for_ver,
                        explicit_version.as_deref(),
                    )
                }),
            );
            if let Ok(psr4) = psr4_result {
                self.psr4.store(Arc::new(psr4));
            }
            let (ver, source) = ver_result
                .unwrap_or_else(|_| (crate::lang::autoload::PHP_8_5.to_string(), "default"));
            self.client
                .log_message(
                    tower_lsp::lsp_types::MessageType::INFO,
                    format!("php-lsp: using PHP {ver} ({source})"),
                )
                .await;
            let ver = if source != "set by editor"
                && !crate::lang::autoload::is_valid_php_version(&ver)
            {
                let clamped = crate::lang::autoload::clamp_php_version(&ver);
                self.client
                    .show_message(
                        tower_lsp::lsp_types::MessageType::WARNING,
                        format!(
                            "php-lsp: detected PHP {ver} is outside the supported range \
                                 ({}); using PHP {clamped} for analysis",
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
        }

        let feat = self.config.load().features.clone();
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: Some(true),
                        will_save_wait_until: Some(true),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                    },
                )),
                completion_provider: feat.completion.then(|| CompletionOptions {
                    trigger_characters: Some(vec![
                        "$".to_string(),
                        ">".to_string(),
                        ":".to_string(),
                        "(".to_string(),
                        "[".to_string(),
                    ]),
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                hover_provider: feat.hover.then_some(HoverProviderCapability::Simple(true)),
                definition_provider: feat.definition.then_some(OneOf::Left(true)),
                references_provider: feat.references.then_some(OneOf::Left(true)),
                document_symbol_provider: feat.document_symbols.then_some(OneOf::Left(true)),
                workspace_symbol_provider: feat.workspace_symbols.then(|| {
                    OneOf::Right(WorkspaceSymbolOptions {
                        resolve_provider: Some(true),
                        work_done_progress_options: Default::default(),
                    })
                }),
                rename_provider: feat.rename.then(|| {
                    OneOf::Right(RenameOptions {
                        prepare_provider: Some(true),
                        work_done_progress_options: Default::default(),
                    })
                }),
                signature_help_provider: feat.signature_help.then(|| SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                inlay_hint_provider: feat.inlay_hints.then(|| {
                    OneOf::Right(InlayHintServerCapabilities::Options(InlayHintOptions {
                        resolve_provider: Some(true),
                        work_done_progress_options: Default::default(),
                    }))
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                semantic_tokens_provider: feat.semantic_tokens.then(|| {
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: legend(),
                        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                        range: Some(true),
                        ..Default::default()
                    })
                }),
                selection_range_provider: feat
                    .selection_range
                    .then_some(SelectionRangeProviderCapability::Simple(true)),
                call_hierarchy_provider: feat
                    .call_hierarchy
                    .then_some(CallHierarchyServerCapability::Simple(true)),
                document_highlight_provider: feat.document_highlight.then_some(OneOf::Left(true)),
                implementation_provider: feat
                    .implementation
                    .then_some(ImplementationProviderCapability::Simple(true)),
                code_action_provider: feat.code_action.then(|| {
                    CodeActionProviderCapability::Options(CodeActionOptions {
                        resolve_provider: Some(true),
                        ..Default::default()
                    })
                }),
                declaration_provider: feat
                    .declaration
                    .then_some(DeclarationCapability::Simple(true)),
                type_definition_provider: feat
                    .type_definition
                    .then_some(TypeDefinitionProviderCapability::Simple(true)),
                code_lens_provider: feat.code_lens.then_some(CodeLensOptions {
                    resolve_provider: Some(true),
                }),
                document_formatting_provider: feat.formatting.then_some(OneOf::Left(true)),
                document_range_formatting_provider: feat
                    .range_formatting
                    .then_some(OneOf::Left(true)),
                document_on_type_formatting_provider: feat.on_type_formatting.then(|| {
                    DocumentOnTypeFormattingOptions {
                        first_trigger_character: "}".to_string(),
                        more_trigger_character: Some(vec!["\n".to_string()]),
                    }
                }),
                document_link_provider: feat.document_link.then(|| DocumentLinkOptions {
                    resolve_provider: Some(true),
                    work_done_progress_options: Default::default(),
                }),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["php-lsp.runTest".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: None,
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        work_done_progress_options: Default::default(),
                    },
                )),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                        will_rename: Some(php_file_op()),
                        did_rename: Some(php_file_op()),
                        will_create: Some(php_file_op()),
                        did_create: Some(php_file_op()),
                        will_delete: Some(php_file_op()),
                        did_delete: Some(php_file_op()),
                    }),
                }),
                linked_editing_range_provider: feat
                    .linked_editing_range
                    .then_some(LinkedEditingRangeServerCapabilities::Simple(true)),
                moniker_provider: Some(OneOf::Left(true)),
                inline_value_provider: feat.inline_values.then(|| {
                    OneOf::Right(InlineValueServerCapabilities::Options(InlineValueOptions {
                        work_done_progress_options: Default::default(),
                    }))
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    pub(crate) async fn handle_initialized(&self, _params: InitializedParams) {
        let php_selector = serde_json::json!([{"language": "php"}]);
        let registrations = vec![
            Registration {
                id: "php-lsp-file-watcher".to_string(),
                method: "workspace/didChangeWatchedFiles".to_string(),
                register_options: Some(serde_json::json!({
                    "watchers": [{"globPattern": "**/*.php"}]
                })),
            },
            Registration {
                id: "php-lsp-type-hierarchy".to_string(),
                method: "textDocument/prepareTypeHierarchy".to_string(),
                register_options: Some(serde_json::json!({"documentSelector": php_selector})),
            },
            Registration {
                id: "php-lsp-config-change".to_string(),
                method: "workspace/didChangeConfiguration".to_string(),
                register_options: Some(serde_json::json!({"section": "php-lsp"})),
            },
        ];
        self.client.register_capability(registrations).await.ok();

        let roots: Vec<PathBuf> = (**self.root_paths.load()).clone();
        if !roots.is_empty() {
            {
                let mut merged = Psr4Map::empty();
                for root in &roots {
                    merged.extend(Psr4Map::load(root));
                }
                self.psr4.store(Arc::new(merged));
            }
            self.meta.store(Arc::new(PhpStormMeta::load(&roots[0])));

            let token = NumberOrString::String("php-lsp/indexing".to_string());
            self.client
                .send_request::<WorkDoneProgressCreate>(WorkDoneProgressCreateParams {
                    token: token.clone(),
                })
                .await
                .ok();

            let warm_docs = Arc::clone(&self.docs);
            tokio::task::spawn_blocking(move || {
                let php_version = warm_docs.workspace_php_version();
                warm_docs.analysis_session(php_version);
            });

            let docs = Arc::clone(&self.docs);
            let open_files = self.open_files.clone();
            let client = self.client.clone();
            let (exclude_paths, include_paths, max_indexed_files) = {
                let cfg = self.config.load();
                let mut exclude = cfg.exclude_paths.clone();
                if !cfg.index_vendor && !exclude.iter().any(|p| p == "vendor" || p == "vendor/") {
                    exclude.push("vendor/".to_string());
                }
                (exclude, cfg.include_paths.clone(), cfg.max_indexed_files)
            };
            tokio::spawn(async move {
                client
                    .send_notification::<ProgressNotification>(ProgressParams {
                        token: token.clone(),
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                            WorkDoneProgressBegin {
                                title: "php-lsp: indexing workspace".to_string(),
                                cancellable: Some(false),
                                message: None,
                                percentage: None,
                            },
                        )),
                    })
                    .await;

                let mut total = 0usize;
                let mut session_cache_set = false;
                for root in roots {
                    let cache = crate::index::cache::WorkspaceCache::new(&root);
                    if !session_cache_set && let Some(ref c) = cache {
                        let session_dir = c.cache_dir().join("session");
                        docs.set_session_cache_dir(session_dir);
                        session_cache_set = true;
                    }
                    total += scan_workspace(
                        root,
                        Arc::clone(&docs),
                        open_files.clone(),
                        cache,
                        &exclude_paths,
                        &include_paths,
                        max_indexed_files,
                    )
                    .await;
                }

                client
                    .send_notification::<ProgressNotification>(ProgressParams {
                        token,
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                            WorkDoneProgressEnd {
                                message: Some(format!("Indexed {total} files")),
                            },
                        )),
                    })
                    .await;

                client
                    .log_message(
                        MessageType::INFO,
                        format!("php-lsp: indexed {total} workspace files"),
                    )
                    .await;

                send_refresh_requests(&client).await;

                let salsa_docs = Arc::clone(&docs);
                drop(docs);
                client.send_notification::<IndexReadyNotification>(()).await;
                drop(tokio::task::spawn_blocking(move || {
                    salsa_docs.get_workspace_index_salsa();
                }));
            });
        }

        self.client
            .log_message(MessageType::INFO, "php-lsp ready")
            .await;
    }

    pub(crate) async fn handle_will_rename_files(
        &self,
        params: RenameFilesParams,
    ) -> Result<Option<WorkspaceEdit>> {
        let psr4 = self.psr4.load();
        let all_docs = self.docs.all_docs_for_scan();
        let mut merged_changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();

        for file_rename in &params.files {
            let old_path = Url::parse(&file_rename.old_uri)
                .ok()
                .and_then(|u| u.to_file_path().ok());
            let new_path = Url::parse(&file_rename.new_uri)
                .ok()
                .and_then(|u| u.to_file_path().ok());

            let (Some(old_path), Some(new_path)) = (old_path, new_path) else {
                continue;
            };

            let old_fqn = psr4.file_to_fqn(&old_path);
            let new_fqn = psr4.file_to_fqn(&new_path);

            let (Some(old_fqn), Some(new_fqn)) = (old_fqn, new_fqn) else {
                continue;
            };

            let edit = use_edits_for_rename(&old_fqn, &new_fqn, &all_docs);
            if let Some(changes) = edit.changes {
                for (uri, edits) in changes {
                    merged_changes.entry(uri).or_default().extend(edits);
                }
            }
        }

        Ok(if merged_changes.is_empty() {
            None
        } else {
            Some(WorkspaceEdit {
                changes: Some(merged_changes),
                ..Default::default()
            })
        })
    }

    pub(crate) async fn handle_did_rename_files(&self, params: RenameFilesParams) {
        for file_rename in &params.files {
            if let Ok(old_uri) = Url::parse(&file_rename.old_uri) {
                self.docs.remove(&old_uri);
            }
            if let Ok(new_uri) = Url::parse(&file_rename.new_uri)
                && let Ok(path) = new_uri.to_file_path()
                && let Ok(text) = tokio::fs::read_to_string(&path).await
            {
                self.ingest_if_not_open(new_uri, &text);
            }
        }
    }

    pub(crate) async fn handle_will_create_files(
        &self,
        params: CreateFilesParams,
    ) -> Result<Option<WorkspaceEdit>> {
        let psr4 = self.psr4.load();
        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();

        for file in &params.files {
            let Ok(uri) = Url::parse(&file.uri) else {
                continue;
            };
            if !uri.path().ends_with(".php") {
                continue;
            }

            let stub = if let Ok(path) = uri.to_file_path()
                && let Some(fqn) = psr4.file_to_fqn(&path)
            {
                let (ns, class_name) = match fqn.rfind('\\') {
                    Some(pos) => (&fqn[..pos], &fqn[pos + 1..]),
                    None => ("", fqn.as_str()),
                };
                if ns.is_empty() {
                    format!("<?php\n\ndeclare(strict_types=1);\n\nclass {class_name}\n{{\n}}\n")
                } else {
                    format!(
                        "<?php\n\ndeclare(strict_types=1);\n\nnamespace {ns};\n\nclass {class_name}\n{{\n}}\n"
                    )
                }
            } else {
                "<?php\n\n".to_string()
            };

            changes.insert(
                uri,
                vec![TextEdit {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    new_text: stub,
                }],
            );
        }

        Ok(if changes.is_empty() {
            None
        } else {
            Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            })
        })
    }

    pub(crate) async fn handle_did_create_files(&self, params: CreateFilesParams) {
        for file in &params.files {
            if let Ok(uri) = Url::parse(&file.uri)
                && let Ok(path) = uri.to_file_path()
                && let Ok(text) = tokio::fs::read_to_string(&path).await
            {
                self.ingest_if_not_open(uri, &text);
            }
        }
        send_refresh_requests(&self.client).await;
    }

    pub(crate) async fn handle_will_delete_files(
        &self,
        params: DeleteFilesParams,
    ) -> Result<Option<WorkspaceEdit>> {
        let psr4 = self.psr4.load();
        let all_docs = self.docs.all_docs_for_scan();
        let mut merged_changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();

        for file in &params.files {
            let path = Url::parse(&file.uri)
                .ok()
                .and_then(|u| u.to_file_path().ok());
            let Some(path) = path else { continue };
            let Some(fqn) = psr4.file_to_fqn(&path) else {
                continue;
            };

            let edit = use_edits_for_delete(&fqn, &all_docs);
            if let Some(changes) = edit.changes {
                for (uri, edits) in changes {
                    merged_changes.entry(uri).or_default().extend(edits);
                }
            }
        }

        Ok(if merged_changes.is_empty() {
            None
        } else {
            Some(WorkspaceEdit {
                changes: Some(merged_changes),
                ..Default::default()
            })
        })
    }

    pub(crate) async fn handle_did_delete_files(&self, params: DeleteFilesParams) {
        for file in &params.files {
            if let Ok(uri) = Url::parse(&file.uri) {
                self.docs.remove(&uri);
                self.client.publish_diagnostics(uri, vec![], None).await;
            }
        }
        send_refresh_requests(&self.client).await;
    }
}
