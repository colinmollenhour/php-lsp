/// Code action: "Extract interface from class"
///
/// Offered when the cursor is on the class declaration. Collects all public
/// non-constructor methods, generates a `NameInterface` above the class, and
/// adds `implements NameInterface` (or appends to an existing `implements`
/// list) to the class header.
use std::collections::HashMap;

use php_ast::{ClassMemberKind, NamespaceBody, Stmt, StmtKind, Visibility};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::document::ast::{ParsedDoc, SourceView, format_type_hint};
use crate::hover::format_params_str;

struct MethodSig {
    name: String,
    is_static: bool,
    by_ref: bool,
    params: String,
    return_type: Option<String>,
}

pub fn extract_interface_actions(
    source: &str,
    doc: &ParsedDoc,
    range: Range,
    uri: &Url,
) -> Vec<CodeActionOrCommand> {
    let sv = doc.view();
    let mut out = Vec::new();
    collect(&doc.program().stmts, source, sv, range, uri, &mut out);
    out
}

fn collect<'a>(
    stmts: &[Stmt<'a, 'a>],
    source: &str,
    sv: SourceView<'_>,
    range: Range,
    uri: &Url,
    out: &mut Vec<CodeActionOrCommand>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Class(c) => {
                let class_start_line = sv.position_of(stmt.span.start).line;
                let brace_line = sv.position_of(c.body.span.start).line;
                if range.start.line < class_start_line || range.start.line > brace_line {
                    continue;
                }

                let Some(class_name) = &c.name else { continue };
                let class_name_str = class_name.to_string();
                let interface_name = format!("{class_name_str}Interface");

                // Don't offer if the class already implements this interface.
                if c.implements.iter().any(|iface| {
                    let repr = iface.to_string_repr();
                    repr == interface_name || repr == format!("\\{interface_name}")
                }) {
                    continue;
                }

                let methods: Vec<MethodSig> = c
                    .body
                    .members
                    .iter()
                    .filter_map(|member| {
                        let ClassMemberKind::Method(m) = &member.kind else {
                            return None;
                        };
                        // Skip non-public and lifecycle methods.
                        if !matches!(m.visibility, Some(Visibility::Public) | None) {
                            return None;
                        }
                        let name = m.name.to_string();
                        if name == "__construct" || name == "__destruct" {
                            return None;
                        }
                        Some(MethodSig {
                            name,
                            is_static: m.is_static,
                            by_ref: m.by_ref,
                            params: format_params_str(&m.params),
                            return_type: m.return_type.as_ref().map(format_type_hint),
                        })
                    })
                    .collect();

                if methods.is_empty() {
                    continue;
                }

                // Insert interface text immediately before the class statement.
                let insert_pos = sv.position_of(stmt.span.start);
                let interface_text = generate_interface_text(&interface_name, &methods);
                let interface_edit = TextEdit {
                    range: Range {
                        start: insert_pos,
                        end: insert_pos,
                    },
                    new_text: format!("{interface_text}\n\n"),
                };

                // Add `implements InterfaceName` (or `, InterfaceName`) before the `{`.
                //
                // Find the last non-whitespace byte before `{` and insert there so the
                // clause stays on the same line regardless of whether `{` is on its own
                // line or not.
                let brace_byte = c.body.span.start as usize;
                let trimmed_len = source[..brace_byte]
                    .trim_end_matches(|ch: char| ch.is_ascii_whitespace())
                    .len();
                let insert_implements_pos = sv.position_of(trimmed_len as u32);
                let implements_text = if c.implements.is_empty() {
                    format!(" implements {interface_name}")
                } else {
                    format!(", {interface_name}")
                };
                let implements_edit = TextEdit {
                    range: Range {
                        start: insert_implements_pos,
                        end: insert_implements_pos,
                    },
                    new_text: implements_text,
                };

                let mut changes = HashMap::new();
                changes.insert(uri.clone(), vec![interface_edit, implements_edit]);

                out.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Extract interface '{interface_name}'"),
                    kind: Some(CodeActionKind::REFACTOR_EXTRACT),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body {
                    collect(&inner.stmts, source, sv, range, uri, out);
                }
            }
            _ => {}
        }
    }
}

fn generate_interface_text(name: &str, methods: &[MethodSig]) -> String {
    let mut text = format!("interface {name}\n{{\n");
    for m in methods {
        let static_kw = if m.is_static { "static " } else { "" };
        let by_ref = if m.by_ref { "&" } else { "" };
        let ret = match &m.return_type {
            Some(t) => format!(": {t}"),
            None => String::new(),
        };
        text.push_str(&format!(
            "    public {}function {}{}({}){ret};\n",
            static_kw, by_ref, m.name, m.params,
        ));
    }
    text.push('}');
    text
}
