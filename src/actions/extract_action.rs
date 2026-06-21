/// Code action: "Extract variable" — wraps the selected expression in a `$extracted` variable.
use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::text::selected_text_range;

/// When the selection is non-empty and appears to be an expression, offer to
/// extract it into a local variable.  The generated variable name is `$extracted`
/// (a safe, unambiguous placeholder that the user can then rename with the LSP
/// rename action).
pub fn extract_variable_actions(source: &str, range: Range, uri: &Url) -> Vec<CodeActionOrCommand> {
    if range.start == range.end {
        return vec![];
    }
    let selected = selected_text_range(source, range);
    if selected.is_empty() || selected.trim().is_empty() {
        return vec![];
    }
    let trimmed = selected.trim();
    if trimmed.starts_with('$')
        && trimmed
            .chars()
            .skip(1)
            .all(|c| c.is_alphanumeric() || c == '_')
    {
        return vec![];
    }

    let indent = line_indent(source, range.start.line);

    let insert_pos = Position {
        line: range.start.line,
        character: 0,
    };
    let insert_text = format!("{indent}$extracted = {trimmed};\n");

    let replace_text = "$extracted".to_string();

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![
            TextEdit {
                range: Range {
                    start: insert_pos,
                    end: insert_pos,
                },
                new_text: insert_text,
            },
            TextEdit {
                range,
                new_text: replace_text,
            },
        ],
    );

    vec![CodeActionOrCommand::CodeAction(CodeAction {
        title: "Extract variable".to_string(),
        kind: Some(CodeActionKind::REFACTOR_EXTRACT),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        ..Default::default()
    })]
}

fn line_indent(source: &str, line: u32) -> String {
    source
        .lines()
        .nth(line as usize)
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).collect())
        .unwrap_or_default()
}
