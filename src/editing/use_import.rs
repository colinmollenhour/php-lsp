use std::collections::HashMap;

use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

use crate::document::ast::ParsedDoc;
use crate::index::file_index::FileIndex;

/// Find the fully-qualified name for a class with the given short `name` by
/// walking the ParsedDoc AST. Returns `Namespace\ClassName` when inside a namespace.
pub(crate) fn find_fqn_for_class(doc: &ParsedDoc, name: &str) -> Option<String> {
    use php_ast::{NamespaceBody, StmtKind};
    for stmt in doc.program().stmts.iter() {
        match &stmt.kind {
            StmtKind::Class(c)
                if c.name.as_ref().map(|n| n.to_string()) == Some(name.to_string()) =>
            {
                return Some(name.to_string());
            }
            StmtKind::Namespace(ns) => {
                let ns_name = ns.name.as_ref().map(|n| n.to_string_repr().to_string());
                if let NamespaceBody::Braced(inner) = &ns.body {
                    for inner_stmt in inner.stmts.iter() {
                        if let StmtKind::Class(c) = &inner_stmt.kind
                            && c.name.as_ref().map(|n| n.to_string()) == Some(name.to_string())
                        {
                            return Some(match ns_name {
                                Some(ref ns) => format!("{ns}\\{name}"),
                                None => name.to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Build a `WorkspaceEdit` that inserts `use FQN;` near the top of the file.
pub(crate) fn build_use_import_edit(source: &str, uri: &Url, fqn: &str) -> WorkspaceEdit {
    // Insert after the `<?php` line and any existing `use` / `namespace` lines
    let insert_line = find_use_insert_line(source);
    let insert_text = format!("use {fqn};\n");
    let pos = Position {
        line: insert_line,
        character: 0,
    };
    let edit = TextEdit {
        range: Range {
            start: pos,
            end: pos,
        },
        new_text: insert_text,
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }
}

/// Find a namespaced function FQN matching `name` in the workspace indexes.
/// Returns `Some(fqn)` only when the FQN is namespaced (contains `\`).
pub(crate) fn find_fqn_for_function(
    name: &str,
    indexes: &[(Url, std::sync::Arc<FileIndex>)],
) -> Option<String> {
    for (_uri, idx) in indexes {
        for func in &idx.functions {
            if func.name.as_ref() == name && func.fqn.contains('\\') {
                return Some(func.fqn.trim_start_matches('\\').to_string());
            }
        }
    }
    None
}

/// Build a `WorkspaceEdit` that inserts `use function FQN;` near the top of the file.
pub(crate) fn build_use_function_import_edit(source: &str, uri: &Url, fqn: &str) -> WorkspaceEdit {
    let insert_line = find_use_insert_line(source);
    let insert_text = format!("use function {fqn};\n");
    let pos = Position {
        line: insert_line,
        character: 0,
    };
    let edit = TextEdit {
        range: Range {
            start: pos,
            end: pos,
        },
        new_text: insert_text,
    };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }
}

pub(crate) fn find_use_insert_line(source: &str) -> u32 {
    let mut last_use_or_ns: u32 = 0;
    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("<?php")
            || trimmed.starts_with("namespace ")
            || trimmed.starts_with("use ")
        {
            last_use_or_ns = i as u32 + 1;
        }
    }
    last_use_or_ns
}
