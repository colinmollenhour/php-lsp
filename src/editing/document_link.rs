/// Document links: clickable paths in require/include expressions and @link/@see docblock tags.
use php_ast::{ExprKind, NamespaceBody, Stmt, StmtKind};
use tower_lsp::lsp_types::{DocumentLink, Position, Range, Url};

use crate::document::ast::{ParsedDoc, SourceView};
use crate::text::byte_to_utf16;

pub fn document_links(uri: &Url, doc: &ParsedDoc, _source: &str) -> Vec<DocumentLink> {
    let sv = doc.view();
    let mut links = Vec::new();
    collect_in_stmts(&doc.program().stmts, sv, uri, &mut links);
    collect_docblock_links(sv.source(), &mut links);
    links
}

fn collect_in_stmts(
    stmts: &[Stmt<'_, '_>],
    sv: SourceView<'_>,
    uri: &Url,
    out: &mut Vec<DocumentLink>,
) {
    for stmt in stmts {
        collect_in_stmt(stmt, sv, uri, out);
    }
}

fn collect_in_stmt(
    stmt: &Stmt<'_, '_>,
    sv: SourceView<'_>,
    uri: &Url,
    out: &mut Vec<DocumentLink>,
) {
    match &stmt.kind {
        StmtKind::Expression(e) => collect_in_expr(e, sv, uri, out),
        StmtKind::Return(Some(v)) => collect_in_expr(v, sv, uri, out),
        StmtKind::Echo(exprs) => {
            for expr in exprs.iter() {
                collect_in_expr(expr, sv, uri, out);
            }
        }
        StmtKind::Function(f) => collect_in_stmts(&f.body.stmts, sv, uri, out),
        StmtKind::Class(c) => {
            use php_ast::ClassMemberKind;
            for member in c.body.members.iter() {
                if let ClassMemberKind::Method(m) = &member.kind
                    && let Some(body) = &m.body
                {
                    collect_in_stmts(&body.stmts, sv, uri, out);
                }
            }
        }
        StmtKind::Namespace(ns) => {
            if let NamespaceBody::Braced(inner) = &ns.body {
                collect_in_stmts(&inner.stmts, sv, uri, out);
            }
        }
        _ => {}
    }
}

fn collect_in_expr(
    expr: &php_ast::Expr<'_, '_>,
    sv: SourceView<'_>,
    uri: &Url,
    out: &mut Vec<DocumentLink>,
) {
    if let ExprKind::Include(_, path_expr) = &expr.kind
        && let Some(link) = link_from_path_expr(path_expr, sv, uri)
    {
        out.push(link);
    }
}

fn link_from_path_expr(
    path_expr: &php_ast::Expr<'_, '_>,
    sv: SourceView<'_>,
    uri: &Url,
) -> Option<DocumentLink> {
    let ExprKind::String(s) = &path_expr.kind else {
        return None;
    };
    let raw: &str = s;
    if raw.is_empty() {
        return None;
    }
    // span.start points to the opening quote; content starts one byte after
    let quote_offset = path_expr.span.start;
    let content_offset = quote_offset + 1;
    let start = sv.position_of(content_offset);
    let end = Position {
        line: start.line,
        character: start.character + raw.chars().map(|c| c.len_utf16() as u32).sum::<u32>(),
    };
    let range = Range { start, end };

    let target = if std::path::Path::new(raw).is_absolute() {
        Url::from_file_path(raw).ok()
    } else {
        // Resolve relative to the document URI. Url::join strips the last
        // path segment (the filename) and appends `raw`, which is correct
        // for both real and synthetic (no drive letter) file:// URIs.
        uri.join(raw).ok()
    };

    Some(DocumentLink {
        range,
        target,
        tooltip: None,
        data: None,
    })
}

/// Scan sv.source() text for `@link` and `@see` tags with HTTP(S) URLs in docblock/line comments.
fn collect_docblock_links(source: &str, out: &mut Vec<DocumentLink>) {
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('*') && !trimmed.starts_with("/**") && !trimmed.starts_with("//") {
            continue;
        }
        for tag in &["@link ", "@see "] {
            if let Some(tag_start) = trimmed.find(tag) {
                let after = trimmed[tag_start + tag.len()..].trim_start();
                if !after.starts_with("http://") && !after.starts_with("https://") {
                    continue;
                }
                let url_str = after.split_whitespace().next().unwrap_or("");
                if url_str.is_empty() {
                    continue;
                }
                if let Ok(target) = Url::parse(url_str)
                    && let Some(col) = line.find(url_str)
                {
                    let start = Position {
                        line: line_idx as u32,
                        character: byte_to_utf16(line, col),
                    };
                    let end = Position {
                        line: line_idx as u32,
                        character: byte_to_utf16(line, col + url_str.len()),
                    };
                    out.push(DocumentLink {
                        range: Range { start, end },
                        target: Some(target),
                        tooltip: None,
                        data: None,
                    });
                }
            }
        }
    }
}
