//! The `parsed_doc` salsa query: parses a `SourceFile` into an `Arc<ParsedDoc>`
//! under salsa memoization. Downstream queries (file_index, symbol_map,
//! semantic diagnostics) depend on this one, so each file is parsed at most
//! once per revision.
//!
//! `ParsedDoc` owns a self-referential bumpalo arena and cannot safely
//! implement the structural `Update` trait — instead we wrap in a `ParsedArc`
//! newtype whose `Update` impl uses `Arc::ptr_eq`. Every reparse produces a
//! new `Arc`, so pointer equality is a correct (if conservative) "changed"
//! signal: salsa never falsely backdates, and downstream queries re-run after
//! every input text change.

use std::sync::Arc;

use crate::document::ast::ParsedDoc;

/// Opaque handle to a parsed document. Cheap to clone (refcount bump); never
/// compared structurally. See module docs for the `Update` contract.
///
/// No `Debug` impl because `ParsedDoc` isn't `Debug` (it owns raw pointers
/// into a bumpalo arena). Salsa doesn't require `Debug` on tracked returns
/// when `no_eq` is used.
#[derive(Clone)]
pub struct ParsedArc(pub Arc<ParsedDoc>);

impl ParsedArc {
    pub fn get(&self) -> &ParsedDoc {
        &self.0
    }
}

// SAFETY: The `ptr_eq` short-circuit returns `false` without writing, matching
// salsa's "no observable change" contract. `ParsedDoc` is already `Send + Sync`
// (see `ast.rs:98`).
crate::impl_arc_update!(ParsedArc);
