use tower_lsp::lsp_types::{Position, Range, TextEdit};

use crate::text::byte_to_utf16;

/// Return `TextEdit`s that delete the entire `use FQN;` line from `source`.
pub fn delete_use_in_source(source: &str, fqn: &str) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let clean = fqn.trim_start_matches('\\');

    let lines: Vec<&str> = source.lines().collect();
    for (line_idx, &line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("use ") {
            continue;
        }

        let Some(use_pos) = line.find("use ") else {
            continue;
        };
        let after_use = use_pos + 4;

        let (_, fqn_str) = if line.as_bytes().get(after_use) == Some(&b'\\') {
            (after_use + 1, &line[after_use + 1..])
        } else {
            (after_use, &line[after_use..])
        };

        if !fqn_str.starts_with(clean) {
            continue;
        }

        let after_fqn = &fqn_str[clean.len()..];
        let is_boundary = after_fqn.is_empty()
            || matches!(after_fqn.as_bytes()[0], b';' | b' ' | b'\t' | b'{' | b',');
        if !is_boundary {
            continue;
        }

        // Delete the whole line including its newline.
        let line_u32 = line_idx as u32;
        let next_line = line_u32 + 1;
        edits.push(TextEdit {
            range: Range {
                start: Position {
                    line: line_u32,
                    character: 0,
                },
                end: Position {
                    line: next_line,
                    character: 0,
                },
            },
            new_text: String::new(),
        });
    }

    edits
}

/// Scan `source` for `use` statements that reference `old_fqn` and return
/// `TextEdit`s that replace `old_fqn` with `new_fqn` in each such line.
///
/// Handles:
/// - `use OldFqn;`
/// - `use \OldFqn;`
/// - `use OldFqn as Alias;`
pub fn use_edits_in_source(source: &str, old_fqn: &str, new_fqn: &str) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let old = old_fqn.trim_start_matches('\\');
    let new_clean = new_fqn.trim_start_matches('\\');

    for (line_idx, line) in source.lines().enumerate() {
        // Only process use-statement lines
        let trimmed = line.trim_start();
        if !trimmed.starts_with("use ") {
            continue;
        }

        let Some(use_pos) = line.find("use ") else {
            continue;
        };
        let after_use = use_pos + 4; // byte offset right after "use "

        // Skip an optional leading backslash in the source
        let (fqn_start, fqn_str) = if line.as_bytes().get(after_use) == Some(&b'\\') {
            (after_use + 1, &line[after_use + 1..])
        } else {
            (after_use, &line[after_use..])
        };

        if !fqn_str.starts_with(old) {
            continue;
        }

        // Confirm the match ends on a word boundary (`;`, space, `{`, `,`, end-of-string)
        let after_fqn = &fqn_str[old.len()..];
        let is_boundary = after_fqn.is_empty()
            || matches!(after_fqn.as_bytes()[0], b';' | b' ' | b'\t' | b'{' | b',');
        if !is_boundary {
            continue;
        }

        let line_u32 = line_idx as u32;
        edits.push(TextEdit {
            range: Range {
                start: Position {
                    line: line_u32,
                    character: byte_to_utf16(line, fqn_start),
                },
                end: Position {
                    line: line_u32,
                    character: byte_to_utf16(line, fqn_start + old.len()),
                },
            },
            new_text: new_clean.to_string(),
        });
    }

    edits
}
