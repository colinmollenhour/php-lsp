use std::sync::Arc;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::analysis::document_highlight::document_highlights;
use crate::navigation::definition::{
    find_declaration_range, find_method_in_class_hierarchy, find_method_range_in_class,
};
use crate::navigation::references::{
    build_mir_symbol, dedup_ref_locations, session_tuple_to_location,
};
use crate::navigation::walk::collect_var_refs_in_scope;
use crate::text::{fqn_short_name, utf16_code_units, word_at_position};
use crate::types::type_map::{TypeMap, enclosing_class_at, enclosing_class_fqn_at};

use super::super::helpers::{
    class_name_at_construct_decl, promoted_property_at_cursor, range_within,
};
use super::super::panic_guard::guard_async_result;
use super::super::{Backend, class_before_double_colon, resolve_reference_symbol};

impl Backend {
    pub(crate) async fn handle_goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        guard_async_result("goto_definition", async move {
            let uri = &params.text_document_position_params.text_document.uri;
            let position = params.text_document_position_params.position;
            let source = self.get_open_text(uri).unwrap_or_default();
            let doc = match self.get_doc(uri) {
                Some(d) => d,
                None => return Ok(None),
            };
            if let Some(word) = crate::text::word_at_position(&source, position)
                && !word.starts_with('$')
            {
                let analysis = self.cached_analysis_async(uri).await;

                // mir 0.41: ClassReference is recorded on the class token in
                // static calls (Foo::bar), new expressions, instanceof, and
                // type hints. When the cursor sits on a class name, jump
                // directly to the class via PSR-4 using the resolved FQN —
                // more accurate than the workspace index for aliased names.
                if let Some(fqn) = analysis.as_deref().and_then(|a| {
                    let off = crate::text::word_range_at(&source, position)
                        .map(|r| doc.view().byte_of_position(r.start))?;
                    let sym = a.symbol_at(off)?;
                    match &sym.kind {
                        mir_analyzer::ReferenceKind::ClassReference(fqn) => Some(fqn.to_string()),
                        _ => None,
                    }
                }) && let Some(loc) = self.psr4_goto(&fqn).await
                {
                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                }

                // Keep both the short name (workspace-index lookup) and the full
                // FQN Arc (PSR-4 vendor fallback). Arc<str> clone is an atomic
                // increment — no heap allocation on the hot path.
                let resolved_method_target = analysis.as_deref().and_then(|a| {
                    let off = crate::text::word_range_at(&source, position)
                        .map(|r| doc.view().byte_of_position(r.start))?;
                    let sym = a.symbol_at(off)?;
                    match &sym.kind {
                        mir_analyzer::ReferenceKind::MethodCall { class, .. }
                        | mir_analyzer::ReferenceKind::StaticCall { class, .. } => {
                            Some((fqn_short_name(class).to_string(), Arc::clone(class)))
                        }
                        _ => None,
                    }
                });
                if let Some((cls, class_fqn_arc)) = resolved_method_target {
                    let wi = self.workspace_index_async().await;
                    if let Some(loc) = find_method_in_class_hierarchy(&cls, &word, &wi.files) {
                        let refined = self
                            .docs
                            .get_doc_salsa(&loc.uri)
                            .and_then(|d| {
                                let range = find_method_range_in_class(&d, &cls, &word)
                                    .or_else(|| find_declaration_range(d.source(), &d, &word));
                                range.map(|range| Location {
                                    uri: loc.uri.clone(),
                                    range,
                                })
                            })
                            .unwrap_or(loc);
                        return Ok(Some(GotoDefinitionResponse::Scalar(refined)));
                    }
                    // Fallback: walk the PSR-4 vendor hierarchy for the resolved class.
                    // trim_start_matches is a pointer offset (no allocation).
                    let class_fqn = class_fqn_arc.trim_start_matches('\\');
                    if let Some(loc) = self.psr4_method_goto(class_fqn, &word).await {
                        return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                    }
                }
            }

            if let Some(loc) =
                crate::navigation::definition::goto_definition(uri, &source, &doc, &[], position)
            {
                return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
            }
            if let Some(line_text) = source.lines().nth(position.line as usize)
                && let Some(word) = crate::text::word_at_position(&source, position)
                && let Some(receiver) = crate::hover::extract_receiver_var_before_cursor(
                    line_text,
                    position.character as usize,
                )
            {
                let class_name = if receiver == "$this" {
                    enclosing_class_at(&source, &doc, position)
                } else {
                    let tm = TypeMap::from_doc_at_position(&doc, None, position);
                    tm.get(&receiver).map(|s| s.to_string())
                };
                if let Some(cls) = class_name {
                    let first_cls = cls.split('|').next().unwrap_or(&cls).to_owned();
                    let wi2 = self.workspace_index_async().await;
                    if let Some(loc) = find_method_in_class_hierarchy(&first_cls, &word, &wi2.files)
                    {
                        let refined = self
                            .docs
                            .get_doc_salsa(&loc.uri)
                            .and_then(|doc| {
                                find_declaration_range(doc.source(), &doc, &word).map(|range| {
                                    Location {
                                        uri: loc.uri.clone(),
                                        range,
                                    }
                                })
                            })
                            .unwrap_or(loc);
                        return Ok(Some(GotoDefinitionResponse::Scalar(refined)));
                    }
                    // Fallback: resolve the class FQN via the workspace index and
                    // walk the PSR-4 vendor hierarchy starting from there.
                    let class_fqn = wi2
                        .files
                        .iter()
                        .find_map(|(_, idx)| {
                            idx.classes
                                .iter()
                                .find(|c| c.name.as_ref() == first_cls.as_str())
                                .map(|c| c.fqn.trim_start_matches('\\').to_owned())
                        })
                        .unwrap_or_else(|| first_cls.clone());
                    if let Some(loc) = self.psr4_method_goto(&class_fqn, &word).await {
                        return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                    }
                }
            }

            let wi = self.workspace_index_async().await;
            if let Some(word) = crate::text::word_at_position(&source, position)
                && let Some(loc) = wi.find_declaration(&word, Some(uri))
            {
                let refined = self
                    .docs
                    .get_doc_salsa(&loc.uri)
                    .and_then(|doc| {
                        find_declaration_range(doc.source(), &doc, &word).map(|range| Location {
                            uri: loc.uri.clone(),
                            range,
                        })
                    })
                    .unwrap_or(loc);
                return Ok(Some(GotoDefinitionResponse::Scalar(refined)));
            }

            if let Some(word) = word_at_position(&source, position)
                && word.contains('\\')
            {
                let imports = crate::navigation::references::collect_class_imports(&doc);
                let expanded = expand_alias_prefix(&word, &imports);
                if let Some(loc) = self.psr4_goto(&expanded).await {
                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                }
            }

            // Resolve `use Foo\Bar as Alias` → navigate to Foo\Bar.
            // Handles cursor on the alias name in `implements Alias` or `extends Alias`
            // where the alias was introduced by a `use … as Alias` statement in this file.
            if let Some(word) = word_at_position(&source, position)
                && !word.contains('\\')
            {
                let imports = crate::navigation::references::collect_class_imports(&doc);
                if let Some(fqn) = imports.get(&word as &str)
                    && let Some(loc) = self.psr4_goto(fqn).await
                {
                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                }
                // PSR-0 fallback: bare class names with underscores (e.g. `Acme_Client`)
                // are not in the workspace index when vendor is excluded. Try PSR-0 resolution.
                if let Some(word) = word_at_position(&source, position)
                    && let Some(loc) = self.psr4_goto(&word).await
                {
                    return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
                }
            }

            Ok(None)
        })
        .await
    }

    pub(crate) async fn handle_references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
        guard_async_result("references", async move {
            let uri = &params.text_document_position.text_document.uri;
            let position = params.text_document_position.position;
            let source = self.get_open_text(uri).unwrap_or_default();
            let word = match word_at_position(&source, position) {
                Some(w) => w,
                None => return Ok(None),
            };
            let include_declaration = params.context.include_declaration;

            if word == "__construct"
                && let Some(doc) = self.get_doc(uri)
            {
                // Try declaration site first. `parent::` is compile-time resolved
                // in PHP — it always names the literal `extends` class, never
                // subject to late static binding — so a `parent::__construct()`
                // call site must resolve to that parent, not the enclosing
                // (child) class. Only fall back to the enclosing-class heuristic
                // when the parent can't be resolved (e.g. an external/vendor
                // base class not present in the workspace index).
                let decl_class =
                    class_name_at_construct_decl(doc.source(), &doc.program().stmts, position);
                let on_call_site = decl_class.is_none();
                let is_parent_call_site = on_call_site
                    && class_before_double_colon(&source, position).as_deref() == Some("parent");
                let class_name = if let Some(decl_class) = decl_class {
                    Some(decl_class)
                } else if is_parent_call_site {
                    let wi = self.workspace_index_async().await;
                    resolve_parent_construct_class(&doc, position, &wi.files)
                        .or_else(|| enclosing_class_fqn_at(doc.source(), &doc, position))
                } else {
                    enclosing_class_fqn_at(doc.source(), &doc, position)
                };
                if let Some(class_name) = class_name {
                    // When cursor is on a call site (not the `function __construct`
                    // declaration), exclude the cursor span from results — it points
                    // to the `parent::__construct()` text, not to the declaration.
                    let incl_decl = include_declaration && !on_call_site;
                    // Instantiation sites (`new Short(...)`) always name the
                    // class's short name; mir records them under
                    // `meth:{fqcn}::__construct`.
                    let fqn = if class_name.contains('\\') {
                        class_name.trim_start_matches('\\').to_string()
                    } else {
                        let imports = self.file_imports(uri);
                        crate::navigation::moniker::resolve_fqn(&doc, &class_name, &imports)
                            .trim_start_matches('\\')
                            .to_string()
                    };
                    let sym = mir_analyzer::Name::method(fqn.as_str(), "__construct");
                    let short = fqn_short_name(&class_name).to_owned();
                    let candidate_urls = self.docs.candidate_urls_for(&short);
                    let files: Vec<Arc<str>> = candidate_urls
                        .iter()
                        .map(|u| Arc::from(u.as_str()))
                        .collect();
                    let docs = Arc::clone(&self.docs);
                    let locations = tokio::task::spawn_blocking(move || {
                        let (_interactive, cancel_rev) = docs.settled_write_rev_guard();
                        let mut locs: Vec<Location> = docs
                            .indexed_references(&sym, &files, incl_decl, Some(cancel_rev))
                            .into_iter()
                            .filter_map(session_tuple_to_location)
                            .collect();
                        dedup_ref_locations(&mut locs);
                        locs
                    })
                    .await
                    .unwrap_or_default();
                    return Ok((!locations.is_empty()).then_some(locations));
                }
                // Cannot determine the owning class — return empty rather than
                // falling through to the unscoped method-reference path.
                return Ok(None);
            }

            // Variables: scope-aware search within the enclosing function/method.
            // The general-purpose reference walker only matches identifiers, not
            // `ExprKind::Variable`, so variables would otherwise return nothing.
            // Skip this path for promoted properties — they need the general
            // property-reference search (which also finds `$this->name` accesses).
            // Skip also when var_spans is empty (class/static property declarations,
            // top-level declarations) so the general path can handle them.
            if word.starts_with('$')
                && let Some(doc) = self.get_doc(uri)
            {
                let is_promoted =
                    promoted_property_at_cursor(doc.source(), &doc.program().stmts, position)
                        .is_some();
                if !is_promoted {
                    let bare = word.trim_start_matches('$');
                    let byte_off = doc.view().byte_of_position(position) as usize;
                    let mut var_spans = Vec::new();
                    collect_var_refs_in_scope(&doc.program().stmts, bare, byte_off, &mut var_spans);
                    if !var_spans.is_empty() {
                        let name_with_sigil = format!("${bare}");
                        let name_utf16_len = utf16_code_units(&name_with_sigil);
                        let sv = doc.view();
                        let src = doc.source();
                        let locations: Vec<Location> = var_spans
                            .into_iter()
                            .map(|(span, _kind)| {
                                // param spans include type annotation; narrow to $var_name.
                                let precise_start = crate::document::ast::str_offset_in_range(
                                    src,
                                    span,
                                    &name_with_sigil,
                                )
                                .unwrap_or(span.start);
                                let start = sv.position_of(precise_start);
                                Location {
                                    uri: uri.clone(),
                                    range: Range {
                                        start,
                                        end: Position {
                                            line: start.line,
                                            character: start.character + name_utf16_len,
                                        },
                                    },
                                }
                            })
                            .collect();
                        return Ok(Some(locations));
                    }
                }
            }
            // Fall through to the general reference path for:
            // - promoted properties (need cross-method $this->prop search)
            // - class/static property declarations (var_spans empty)
            // - any other $word the scope walker didn't find

            let doc_opt = self.get_doc(uri);

            // Usage-site cursor: mir's per-file analysis already resolved the
            // symbol under the cursor (receiver types, aliases, namespaces) —
            // its `ReferenceKind` maps 1:1 onto the index key.
            let usage_symbol: Option<mir_analyzer::Name> = {
                let analysis = self.cached_analysis_async(uri).await;
                analysis.as_deref().and_then(|a| {
                    let doc = doc_opt.as_ref()?;
                    let off = crate::text::word_range_at(&source, position)
                        .map(|r| doc.view().byte_of_position(r.start))?;
                    a.symbol_at(off).and_then(|s| s.kind.to_name())
                })
            };

            // Declaration-site cursor (or no analysis): classify the cursor
            // context and resolve the owner/target FQN.
            let (word, kind, constant_owner) =
                resolve_reference_symbol(doc_opt.as_ref(), &source, position, word);
            let symbol = match usage_symbol {
                Some(sym) => sym,
                None => {
                    let target_fqn = self.resolve_reference_target_fqn(
                        uri,
                        doc_opt.as_ref(),
                        &word,
                        kind,
                        position,
                        constant_owner.clone(),
                    );
                    match build_mir_symbol(
                        &word,
                        kind,
                        target_fqn.as_deref(),
                        constant_owner.is_some(),
                    ) {
                        Some(sym) => sym,
                        None => return Ok(None),
                    }
                }
            };

            // Visibility scoping: a private/protected method can only be
            // referenced from its declaring class file (+ subtype files for
            // protected). When that scope is known, use it directly as the
            // candidate set — skipping the whole-workspace text sweep in
            // `candidate_urls_for`. Public symbols fall back to the
            // text-filtered workspace scope, unioned over the cursor word and
            // the symbol's short name (they differ under `use ... as` aliases).
            let method_scope = if let mir_analyzer::Name::Method { class, name } = &symbol {
                self.docs.method_reference_scope(class, name)
            } else {
                None
            };
            let candidate_urls = method_scope.unwrap_or_else(|| {
                let mut urls = self.docs.candidate_urls_for(&word);
                let short = match &symbol {
                    mir_analyzer::Name::Class(f)
                    | mir_analyzer::Name::Function(f)
                    | mir_analyzer::Name::GlobalConstant(f) => {
                        fqn_short_name(f.trim_start_matches('\\')).to_string()
                    }
                    mir_analyzer::Name::Method { name, .. }
                    | mir_analyzer::Name::Property { name, .. }
                    | mir_analyzer::Name::ClassConstant { name, .. } => name.to_string(),
                };
                if short != word.as_str() {
                    let mut extra = self.docs.candidate_urls_for(&short);
                    urls.append(&mut extra);
                    urls.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
                    urls.dedup();
                }
                urls
            });
            let files: Vec<Arc<str>> = candidate_urls
                .iter()
                .map(|u| Arc::from(u.as_str()))
                .collect();

            // Declaration coverage comes from mir's definitions index (the
            // `include_declaration` flag below) — never from the raw cursor
            // span, which on a `use` import line is not a reference at all.
            let docs = Arc::clone(&self.docs);
            let locations = tokio::task::spawn_blocking(move || {
                // Pause the background scan and snapshot a settled revision so
                // only a genuine user edit cancels the search.
                let (_interactive, cancel_rev) = docs.settled_write_rev_guard();
                let mut locs: Vec<Location> = docs
                    .indexed_references(&symbol, &files, include_declaration, Some(cancel_rev))
                    .into_iter()
                    .filter_map(session_tuple_to_location)
                    .collect();
                dedup_ref_locations(&mut locs);
                locs
            })
            .await
            .unwrap_or_default();

            Ok((!locations.is_empty()).then_some(locations))
        })
        .await
    }

    pub(crate) async fn handle_linked_editing_range(
        &self,
        params: LinkedEditingRangeParams,
    ) -> Result<Option<LinkedEditingRanges>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let source = self.get_open_text(uri).unwrap_or_default();
        let doc = match self.get_doc(uri) {
            Some(d) => d,
            None => return Ok(None),
        };
        let word = match crate::text::word_at_position(&source, position) {
            Some(w) => w,
            None => return Ok(None),
        };
        let is_variable = word.starts_with('$');
        let cursor_word_range = match crate::text::word_range_at(&source, position) {
            Some(r) => r,
            None => return Ok(None),
        };

        let highlights = document_highlights(&source, &doc, position);
        if highlights.is_empty() {
            return Ok(None);
        }

        if !highlights.iter().any(|h| h.range == cursor_word_range) {
            return Ok(None);
        }

        let scope_to_class = !is_variable
            && crate::types::type_map::enclosing_class_at(&source, &doc, position).as_deref()
                != Some(word.as_str());
        let other_class_ranges: Vec<Range> = if scope_to_class {
            let cursor_class = crate::types::type_map::enclosing_class_range_at(&doc, position);
            crate::types::type_map::collect_all_class_ranges(&doc)
                .into_iter()
                .filter(|r| Some(*r) != cursor_class)
                .collect()
        } else {
            Vec::new()
        };
        let ranges: Vec<Range> = highlights
            .into_iter()
            .map(|h| h.range)
            .filter(|r| !other_class_ranges.iter().any(|ocr| range_within(*r, *ocr)))
            .collect();
        if ranges.is_empty() {
            return Ok(None);
        }

        let word_pattern = if is_variable {
            "\\$[a-zA-Z_\\u00A0-\\uFFFF][a-zA-Z0-9_\\u00A0-\\uFFFF]*".to_string()
        } else {
            "[a-zA-Z_\\u00A0-\\uFFFF][a-zA-Z0-9_\\u00A0-\\uFFFF]*".to_string()
        };
        Ok(Some(LinkedEditingRanges {
            ranges,
            word_pattern: Some(word_pattern),
        }))
    }
}

fn expand_alias_prefix(word: &str, imports: &std::collections::HashMap<String, String>) -> String {
    if let Some((first, rest)) = word.split_once('\\')
        && let Some(ns_prefix) = imports.get(first)
    {
        return format!("{}\\{}", ns_prefix, rest);
    }
    word.to_string()
}

/// Resolve a `parent::__construct()` call site to the FQN of the class named
/// in the enclosing class's `extends` clause. Looks up the `extends` name
/// (same-file, as written in source) and confirms it against an indexed
/// class across the workspace, since the raw `extends` text alone doesn't
/// tell us whether it's an external/vendor class we can't scope to. Returns
/// `None` when no such class is indexed, so the caller can fall back to the
/// enclosing-class heuristic.
fn resolve_parent_construct_class(
    doc: &crate::document::ast::ParsedDoc,
    position: Position,
    files: &[(Url, Arc<crate::index::file_index::FileIndex>)],
) -> Option<String> {
    let child_short = enclosing_class_at(doc.source(), doc, position)?;
    let raw_parent = crate::types::type_map::parent_class_name(doc, &child_short)?;
    let parent_short = fqn_short_name(&raw_parent);
    let parent_bare = raw_parent.trim_start_matches('\\');
    files.iter().find_map(|(_, idx)| {
        idx.classes
            .iter()
            .find(|cls| {
                cls.name.as_ref() == parent_short || cls.fqn.trim_start_matches('\\') == parent_bare
            })
            .map(|cls| cls.fqn.trim_start_matches('\\').to_owned())
    })
}
