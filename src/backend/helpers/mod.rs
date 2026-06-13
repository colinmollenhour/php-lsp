//! Backend support helpers, grouped by concern:
//! - [`position`] — character/offset math and the symbol-kind heuristic,
//! - [`cursor_decl`] — cursor-on-declaration detection,
//! - [`phpunit`] — the `vendor/bin/phpunit` runner.
//!
//! This module file keeps the LSP file-operation registration, the deferred
//! code-action machinery, and the non-blocking `Backend` wrappers that don't
//! belong to any of the above.

use std::sync::Arc;

use tower_lsp::lsp_types::*;

use crate::document::ast::ParsedDoc;
use crate::navigation::definition::find_declaration_range;

use crate::actions::generate_action::{
    generate_constructor_actions, generate_getters_setters_actions,
};
use crate::actions::implement_action::implement_missing_actions;
use crate::actions::phpdoc_action::phpdoc_actions;
use crate::actions::promote_action::promote_constructor_actions;
use crate::actions::type_action::add_return_type_actions;

use super::Backend;

mod cursor_decl;
mod phpunit;
mod position;

pub(super) use cursor_decl::*;
pub(super) use phpunit::*;
pub(super) use position::*;

pub(super) fn php_file_op() -> FileOperationRegistrationOptions {
    FileOperationRegistrationOptions {
        filters: vec![FileOperationFilter {
            scheme: Some("file".to_string()),
            pattern: FileOperationPattern {
                glob: "**/*.php".to_string(),
                matches: Some(FileOperationPatternKind::File),
                options: None,
            },
        }],
    }
}

/// Strip the `edit` from each `CodeAction` and attach a `data` payload so the
/// client can request the edit lazily via `codeAction/resolve`.
pub(super) fn defer_actions(
    actions: Vec<CodeActionOrCommand>,
    kind_tag: &str,
    uri: &Url,
    range: Range,
) -> Vec<CodeActionOrCommand> {
    actions
        .into_iter()
        .map(|a| match a {
            CodeActionOrCommand::CodeAction(mut ca) => {
                ca.edit = None;
                ca.data = Some(serde_json::json!({
                    "php_lsp_resolve": kind_tag,
                    "uri": uri.to_string(),
                    "range": range,
                }));
                CodeActionOrCommand::CodeAction(ca)
            }
            other => other,
        })
        .collect()
}

/// Tags for deferred code actions (resolved lazily via `codeAction/resolve`).
/// Iteration order controls the order items appear in the client menu.
pub(super) const DEFERRED_ACTION_TAGS: &[&str] = &[
    "phpdoc",
    "implement",
    "constructor",
    "getters_setters",
    "return_type",
    "promote",
];

impl Backend {
    /// Run [`crate::document::document_store::DocumentStore::cached_analysis`] without
    /// blocking the async executor. The warm path (cache entry current for the
    /// file's text) resolves synchronously; the cold path — mir Pass 1 + Pass 2,
    /// which can take hundreds of ms on large files and is hit after every
    /// keystroke because edits clear the analysis cache — runs on the blocking
    /// pool so it doesn't stall other in-flight requests.
    pub(super) async fn cached_analysis_async(
        &self,
        uri: &Url,
    ) -> Option<Arc<mir_analyzer::FileAnalysis>> {
        if let Some(hit) = self.docs.cached_analysis_if_fresh(uri) {
            return Some(hit);
        }
        let docs = Arc::clone(&self.docs);
        let uri = uri.clone();
        tokio::task::spawn_blocking(move || docs.cached_analysis(&uri))
            .await
            .unwrap_or(None)
    }

    /// Fetch the salsa-memoized workspace aggregate without blocking the async
    /// executor. A warm memo returns quickly, but the cold rebuild after any
    /// file change walks every `FileIndex` in the workspace — run it on the
    /// blocking pool.
    pub(super) async fn workspace_index_async(
        &self,
    ) -> Arc<crate::db::workspace_index::WorkspaceIndexData> {
        let docs = Arc::clone(&self.docs);
        match tokio::task::spawn_blocking(move || docs.get_workspace_index_salsa()).await {
            Ok(wi) => wi,
            // JoinError (panicked/cancelled blocking task): retry inline so a
            // panic surfaces through the caller's panic guard.
            Err(_) => self.docs.get_workspace_index_salsa(),
        }
    }

    /// Tag → generator mapping for deferred code actions.
    pub(super) fn generate_deferred_actions(
        &self,
        tag: &str,
        source: &str,
        doc: &Arc<ParsedDoc>,
        range: Range,
        uri: &Url,
    ) -> Vec<CodeActionOrCommand> {
        match tag {
            "phpdoc" => phpdoc_actions(uri, doc, source, range),
            "implement" => {
                let imports = self.file_imports(uri);
                implement_missing_actions(
                    source,
                    doc,
                    &self
                        .docs
                        .doc_with_others(uri, Arc::clone(doc), &self.open_urls()),
                    range,
                    uri,
                    &imports,
                )
            }
            "constructor" => generate_constructor_actions(source, doc, range, uri),
            "getters_setters" => generate_getters_setters_actions(source, doc, range, uri),
            "return_type" => add_return_type_actions(source, doc, range, uri),
            "promote" => promote_constructor_actions(source, doc, range, uri),
            _ => Vec::new(),
        }
    }

    /// Try to resolve a fully-qualified name via the PSR-4 map.
    /// Indexes the file on-demand if it is not already in the document store.
    pub(super) async fn psr4_goto(&self, fqn: &str) -> Option<Location> {
        let path = self.psr4.load().resolve(fqn)?;

        let file_uri = Url::from_file_path(&path).ok()?;

        // Index on-demand if the file was not picked up by the workspace scan.
        // Use `get_doc_salsa_any` (ignores open-file gating): after `ingest()`
        // the file is mirrored but background-only, and the call site needs
        // the AST regardless of whether the editor has the file open.
        if self.docs.get_doc_salsa(&file_uri).is_none() {
            let text = tokio::fs::read_to_string(&path).await.ok()?;
            self.ingest_if_not_open(file_uri.clone(), &text);
        }

        let doc = self.docs.get_doc_salsa(&file_uri)?;

        // Classes are declared by their short (unqualified) name, e.g. `class Foo`
        // not `class App\Services\Foo`.
        let short_name = fqn.split('\\').next_back()?;
        let range = find_declaration_range(doc.source(), &doc, short_name)?;

        Some(Location {
            uri: file_uri,
            range,
        })
    }

    /// Request the client to apply a workspace edit.
    /// Returns true if the edit was successfully applied, false otherwise.
    pub async fn apply_workspace_edit(&self, edit: WorkspaceEdit) -> bool {
        self.client
            .apply_edit(edit)
            .await
            .ok()
            .map(|result| result.applied)
            .unwrap_or(false)
    }
}
