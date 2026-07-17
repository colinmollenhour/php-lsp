//! The `parsed_doc` salsa query: parses a `SourceFile` into an `Arc<ParsedDoc>`
//! under salsa memoization. Downstream queries (file_index, symbol_map,
//! semantic diagnostics) depend on this one, so each file is parsed at most
//! once per revision.
//!
//! `ParsedDoc` owns a self-referential bumpalo arena and isn't `PartialEq`, so
//! `parsed_doc` is `#[salsa::tracked(no_eq)]`: salsa skips backdating and
//! downstream queries re-run after every input text change.

use std::sync::Arc;

use crate::document::ast::ParsedDoc;

/// Opaque handle to a parsed document. Cheap to clone (refcount bump); never
/// compared structurally — see module docs for the `no_eq` contract.
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
