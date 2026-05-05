#![allow(dead_code, unused_imports)]

#[path = "common/mod.rs"]
mod common;

pub use common::render::{
    assert_linked_editing_ranges_share_text, assert_selection_range_invariant,
};
pub use common::{
    TestServer, lines_of, render_completion, render_document_symbols, render_hover,
    render_inlay_hints, render_locations, render_semantic_tokens, render_workspace_symbols,
};

#[path = "analysis/feature_code_lens.rs"]
mod feature_code_lens;
#[path = "analysis/feature_diagnostics.rs"]
mod feature_diagnostics;
#[path = "analysis/feature_inlay_hints.rs"]
mod feature_inlay_hints;
#[path = "analysis/feature_inline_value.rs"]
mod feature_inline_value;
#[path = "analysis/feature_semantic_tokens.rs"]
mod feature_semantic_tokens;
