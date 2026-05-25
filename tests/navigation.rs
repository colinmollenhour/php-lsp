#![allow(dead_code, unused_imports)]

#[path = "common/mod.rs"]
mod common;

// Re-export for submodules to use via `use super::*`
pub use common::render::{
    assert_linked_editing_ranges_share_text, assert_selection_range_invariant,
};
pub use common::{
    TestServer, canonicalize_workspace_edit, lines_of, render_completion, render_document_symbols,
    render_hover, render_inlay_hints, render_locations, render_semantic_tokens,
    render_workspace_symbols,
};

#[path = "navigation/feature_call_hierarchy.rs"]
mod feature_call_hierarchy;
#[path = "navigation/feature_declaration.rs"]
mod feature_declaration;
#[path = "navigation/feature_definition.rs"]
mod feature_definition;
#[path = "navigation/feature_highlight.rs"]
mod feature_highlight;
#[path = "navigation/feature_moniker.rs"]
mod feature_moniker;
#[path = "navigation/feature_references.rs"]
mod feature_references;
#[path = "navigation/feature_references_imports.rs"]
mod feature_references_imports;
#[path = "navigation/feature_rename.rs"]
mod feature_rename;
#[path = "navigation/feature_type_definition.rs"]
mod feature_type_definition;
#[path = "navigation/feature_type_hierarchy.rs"]
mod feature_type_hierarchy;
