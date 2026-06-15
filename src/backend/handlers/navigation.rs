use std::sync::Arc;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::analysis::document_highlight::document_highlights;
use crate::navigation::definition::{
    find_declaration_range, find_method_in_class_hierarchy, find_method_range_in_class,
};
use crate::navigation::references::{SymbolKind, find_references, find_references_with_target};
use crate::text::{fqn_short_name, word_at_position};
use crate::types::type_map::{TypeMap, enclosing_class_at};

use super::super::helpers::{class_name_at_construct_decl, range_within};
use super::super::panic_guard::guard_async_result;
use super::super::{Backend, build_mir_symbol, resolve_reference_symbol};

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
                let resolved_class = analysis.as_deref().and_then(|a| {
                    let off = crate::text::word_range_at(&source, position)
                        .map(|r| doc.view().byte_of_position(r.start))?;
                    let sym = a.symbol_at(off)?;
                    match &sym.kind {
                        mir_analyzer::ReferenceKind::MethodCall { class, .. }
                        | mir_analyzer::ReferenceKind::StaticCall { class, .. } => {
                            Some(fqn_short_name(class).to_string())
                        }
                        _ => None,
                    }
                });
                if let Some(cls) = resolved_class {
                    let all_indexes = self.docs.all_indexes();
                    if let Some(loc) = find_method_in_class_hierarchy(&cls, &word, &all_indexes) {
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
                    let all_indexes = self.docs.all_indexes();
                    if let Some(loc) =
                        find_method_in_class_hierarchy(&first_cls, &word, &all_indexes)
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
            tokio::task::yield_now().await;
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
                && let Some(class_name) =
                    class_name_at_construct_decl(doc.source(), &doc.program().stmts, position)
            {
                let locations = self.construct_references(
                    uri,
                    &source,
                    position,
                    &class_name,
                    include_declaration,
                );
                return Ok((!locations.is_empty()).then_some(locations));
            }

            let doc_opt = self.get_doc(uri);
            let (word, kind, constant_owner) =
                resolve_reference_symbol(doc_opt.as_ref(), &source, position, word);
            let target_fqn = self.resolve_reference_target_fqn(
                uri,
                doc_opt.as_ref(),
                &word,
                kind,
                position,
                constant_owner,
            );

            let candidate_docs = self.docs.candidate_docs_for(&word);

            if matches!(kind, Some(SymbolKind::Method)) {
                let candidate_urls = self.docs.candidate_urls_mentioning(&word);
                let docs = Arc::clone(&self.docs);
                let _ = tokio::task::spawn_blocking(move || {
                    docs.ensure_files_ingested(&candidate_urls)
                })
                .await;
            }
            let owner_short: Option<String> = if matches!(kind, Some(SymbolKind::Method)) {
                target_fqn
                    .as_deref()
                    .map(|fqn| fqn_short_name(fqn.trim_start_matches('\\')).to_string())
            } else {
                None
            };

            let session_method_refs = self.session_method_references(
                &word,
                kind,
                target_fqn.as_deref(),
                owner_short.as_deref(),
            );

            let mut locations = if let Some(session_locs) =
                session_method_refs.filter(|l| !l.is_empty())
            {
                let mut combined = session_locs;
                if include_declaration {
                    let range =
                        crate::text::word_range_at(&source, position).unwrap_or_else(|| Range {
                            start: position,
                            end: Position {
                                line: position.line,
                                character: position.character + word.encode_utf16().count() as u32,
                            },
                        });
                    combined.push(Location {
                        uri: uri.clone(),
                        range,
                    });
                    crate::references::dedup_ref_locations(&mut combined);
                }
                combined
            } else {
                match target_fqn.as_deref() {
                    Some(t) => find_references_with_target(
                        &word,
                        &candidate_docs,
                        include_declaration,
                        kind,
                        t,
                    ),
                    None => find_references(&word, &candidate_docs, include_declaration, kind),
                }
            };

            if !matches!(kind, Some(SymbolKind::Method) | Some(SymbolKind::Property))
                && let Some(sym) = build_mir_symbol(&word, kind, target_fqn.as_deref())
            {
                let extra = self.docs.session_references_to(&sym);
                if !extra.is_empty() {
                    let mut seen: std::collections::HashSet<(String, u32, u32, u32)> = locations
                        .iter()
                        .map(crate::references::ref_location_key)
                        .collect();
                    for loc in extra
                        .into_iter()
                        .filter_map(crate::references::session_tuple_to_location)
                    {
                        if seen.insert(crate::references::ref_location_key(&loc)) {
                            locations.push(loc);
                        }
                    }
                }
            }

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
