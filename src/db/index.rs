//! `file_index` salsa query — derives a compact `FileIndex` from a parsed
//! document. Depends on `parsed_doc`, so editing a file reparses once and the
//! index re-extracts from the new AST.

use std::sync::Arc;

use crate::index::file_index::FileIndex;

/// Arc wrapper for `FileIndex`. Uses structural equality on the inner
/// `FileIndex` so salsa can short-circuit downstream queries (e.g.
/// `workspace_index`) when a body-only edit produces an identical index.
#[derive(Clone, PartialEq, Debug)]
pub struct IndexArc(pub Arc<FileIndex>);

impl IndexArc {
    pub fn get(&self) -> &FileIndex {
        &self.0
    }
}

// SAFETY: writes through `old_pointer` only when returning `true`. Uses
// structural equality on `FileIndex` so that body-only edits (no declaration
// change) return `false` and don't cascade to `workspace_index`.
unsafe impl salsa::Update for IndexArc {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_ref = unsafe { &mut *old_pointer };
        if *old_ref.0 == *new_value.0 {
            false
        } else {
            *old_ref = new_value;
            true
        }
    }
}
