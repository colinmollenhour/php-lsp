use std::collections::HashMap;
use std::sync::Arc;

use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

use crate::ast::ParsedDoc;
use crate::text::byte_to_utf16;

/// Build a `WorkspaceEdit` that updates every `use` import referencing `old_fqn`
/// to `new_fqn` across all indexed documents.
pub fn use_edits_for_rename(
    old_fqn: &str,
    new_fqn: &str,
    all_docs: &[(Url, Arc<ParsedDoc>)],
) -> WorkspaceEdit {
    if old_fqn == new_fqn {
        return WorkspaceEdit::default();
    }

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    for (uri, doc) in all_docs {
        let edits = use_edits_in_source(doc.source(), old_fqn, new_fqn);
        if !edits.is_empty() {
            changes.insert(uri.clone(), edits);
        }
    }

    WorkspaceEdit {
        changes: if changes.is_empty() {
            None
        } else {
            Some(changes)
        },
        ..Default::default()
    }
}

/// Build a `WorkspaceEdit` that removes every `use` import referencing `fqn`
/// across all indexed documents.  Called by `workspace/willDeleteFiles` so that
/// deleting a PHP file automatically cleans up dangling imports.
pub fn use_edits_for_delete(fqn: &str, all_docs: &[(Url, Arc<ParsedDoc>)]) -> WorkspaceEdit {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    for (uri, doc) in all_docs {
        let edits = delete_use_in_source(doc.source(), fqn);
        if !edits.is_empty() {
            changes.insert(uri.clone(), edits);
        }
    }

    WorkspaceEdit {
        changes: if changes.is_empty() {
            None
        } else {
            Some(changes)
        },
        ..Default::default()
    }
}

/// Return `TextEdit`s that delete the entire `use FQN;` line from `source`.
fn delete_use_in_source(source: &str, fqn: &str) -> Vec<TextEdit> {
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
fn use_edits_in_source(source: &str, old_fqn: &str, new_fqn: &str) -> Vec<TextEdit> {
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
