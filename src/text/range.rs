//! Zero-width LSP `Range`/`Location` constructors for line-level results.

use tower_lsp::lsp_types::{Location, Position, Range, Url};

/// Build a zero-width LSP `Range` at the start of `line` (character 0).
/// Used for index-backed features where only line-level precision is available.
pub(crate) fn zero_width_range(line: u32) -> Range {
    let pos = Position { line, character: 0 };
    Range {
        start: pos,
        end: pos,
    }
}

/// Build a `Location` pointing to the start of `line` in `uri` (character 0).
pub(crate) fn zero_width_location(uri: &Url, line: u32) -> Location {
    Location {
        uri: uri.clone(),
        range: zero_width_range(line),
    }
}
