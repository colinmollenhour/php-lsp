//! The mir-backed resolved-type provider: the additive accessor that sits beside
//! the legacy `TypeMap` string path (it does **not** replace it).
//!
//! LSP feature handlers call [`resolved_type_at`] first; when it returns `Some`,
//! they render the generic-aware [`mir_types::Type`] via
//! [`crate::generics::render_type`]. When it returns `None` (no resolved symbol,
//! stale cache after an edit, or a position with no recorded symbol), the caller
//! falls back to the unchanged `TypeMap`/`String` path. This gating is what keeps
//! the non-generic behaviour byte-identical to today.

use std::sync::Arc;

use mir_types::Type;
use tower_lsp::lsp_types::Url;

use crate::document_store::DocumentStore;

/// Resolve the generic-aware type of the innermost expression at `byte_off` in
/// `uri`, using only the resolved-symbol cache populated by the last diagnostics
/// pass.
///
/// `text_arc` must be the caller's current source pointer (typically
/// `doc.source_arc()`); the cache validates it against the analysed pointer with
/// `Arc::ptr_eq` and returns `None` on any mismatch. This call **never** runs
/// analysis and never blocks — on `None`, the caller uses the legacy path.
///
/// `byte_off` is a byte offset into the source, e.g. from
/// [`crate::ast::SourceView::byte_of_position`].
pub fn resolved_type_at(
    store: &DocumentStore,
    uri: &Url,
    text_arc: &Arc<str>,
    byte_off: u32,
) -> Option<Type> {
    store
        .resolved_symbol_cache()
        .type_at(uri, text_arc, byte_off)
}
