//! Document lifecycle: the arena-backed parsed document, the salsa-input
//! document store, and the open-file (editor buffer) state.

pub mod ast;
pub mod document_store;

pub(crate) mod open_files;
