use std::collections::HashMap;

use php_ast::{ClassMemberKind, ClosureExpr, Expr, ExprKind, NamespaceBody, Span, Stmt, StmtKind};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::document::ast::{ParsedDoc, SourceView};

/// Offer "Convert to arrow function" when the cursor is inside a closure whose
/// body is a single `return` statement and no use-by-ref captures are present.
/// The innermost convertible closure at the cursor position wins.
pub fn closure_to_arrow_function_actions(
    source: &str,
    doc: &ParsedDoc,
    range: Range,
    uri: &Url,
) -> Vec<CodeActionOrCommand> {
    let sv = doc.view();
    let cursor = sv.byte_of_position(range.start);
    let mut out = Vec::new();
    collect_in_stmts(&doc.program().stmts, source, cursor, uri, sv, &mut out);
    out
}

fn collect_in_stmts(
    stmts: &[Stmt<'_, '_>],
    source: &str,
    cursor: u32,
    uri: &Url,
    sv: SourceView<'_>,
    out: &mut Vec<CodeActionOrCommand>,
) -> bool {
    for stmt in stmts {
        if stmt.span.end < cursor || stmt.span.start > cursor {
            continue;
        }
        if collect_in_stmt(stmt, source, cursor, uri, sv, out) {
            return true;
        }
    }
    false
}

fn collect_in_stmt(
    stmt: &Stmt<'_, '_>,
    source: &str,
    cursor: u32,
    uri: &Url,
    sv: SourceView<'_>,
    out: &mut Vec<CodeActionOrCommand>,
) -> bool {
    match &stmt.kind {
        StmtKind::Expression(expr) | StmtKind::Throw(expr) => {
            collect_in_expr(expr, source, cursor, uri, sv, out)
        }
        StmtKind::Return(Some(expr)) => collect_in_expr(expr, source, cursor, uri, sv, out),
        StmtKind::Function(f) => collect_in_stmts(&f.body.stmts, source, cursor, uri, sv, out),
        StmtKind::Class(c) => {
            for member in c.body.members.iter() {
                if let ClassMemberKind::Method(m) = &member.kind
                    && let Some(body) = &m.body
                    && collect_in_stmts(&body.stmts, source, cursor, uri, sv, out)
                {
                    return true;
                }
            }
            false
        }
        StmtKind::Namespace(ns) => {
            if let NamespaceBody::Braced(inner) = &ns.body {
                collect_in_stmts(&inner.stmts, source, cursor, uri, sv, out)
            } else {
                false
            }
        }
        StmtKind::Block(b) => collect_in_stmts(&b.stmts, source, cursor, uri, sv, out),
        StmtKind::If(i) => {
            if collect_in_expr(&i.condition, source, cursor, uri, sv, out) {
                return true;
            }
            if collect_in_stmt(i.then_branch, source, cursor, uri, sv, out) {
                return true;
            }
            for ei in i.elseif_branches.iter() {
                if collect_in_expr(&ei.condition, source, cursor, uri, sv, out)
                    || collect_in_stmt(&ei.body, source, cursor, uri, sv, out)
                {
                    return true;
                }
            }
            if let Some(e) = &i.else_branch {
                collect_in_stmt(e, source, cursor, uri, sv, out)
            } else {
                false
            }
        }
        StmtKind::While(w) => {
            collect_in_expr(&w.condition, source, cursor, uri, sv, out)
                || collect_in_stmt(w.body, source, cursor, uri, sv, out)
        }
        StmtKind::DoWhile(d) => {
            collect_in_stmt(d.body, source, cursor, uri, sv, out)
                || collect_in_expr(&d.condition, source, cursor, uri, sv, out)
        }
        StmtKind::For(f) => {
            for e in f
                .init
                .iter()
                .chain(f.condition.iter())
                .chain(f.update.iter())
            {
                if collect_in_expr(e, source, cursor, uri, sv, out) {
                    return true;
                }
            }
            collect_in_stmt(f.body, source, cursor, uri, sv, out)
        }
        StmtKind::Foreach(f) => {
            collect_in_expr(&f.expr, source, cursor, uri, sv, out)
                || collect_in_stmt(f.body, source, cursor, uri, sv, out)
        }
        StmtKind::TryCatch(t) => {
            if collect_in_stmts(&t.body.stmts, source, cursor, uri, sv, out) {
                return true;
            }
            for catch in t.catches.iter() {
                if collect_in_stmts(&catch.body.stmts, source, cursor, uri, sv, out) {
                    return true;
                }
            }
            if let Some(finally) = &t.finally {
                collect_in_stmts(&finally.stmts, source, cursor, uri, sv, out)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn collect_in_expr(
    expr: &Expr<'_, '_>,
    source: &str,
    cursor: u32,
    uri: &Url,
    sv: SourceView<'_>,
    out: &mut Vec<CodeActionOrCommand>,
) -> bool {
    if expr.span.end < cursor || expr.span.start > cursor {
        return false;
    }
    match &expr.kind {
        ExprKind::Closure(c) => {
            // Prefer the innermost convertible closure: recurse into body first.
            if collect_in_stmts(&c.body.stmts, source, cursor, uri, sv, out) {
                return true;
            }
            if let Some(action) = build_action(c, expr.span, source, uri, sv) {
                out.push(action);
                return true;
            }
            false
        }
        ExprKind::ArrowFunction(af) => {
            // Walk into the arrow function body so nested closures are reachable.
            collect_in_expr(af.body, source, cursor, uri, sv, out)
        }
        ExprKind::Assign(a) => collect_in_expr(a.value, source, cursor, uri, sv, out),
        ExprKind::FunctionCall(fc) => {
            if collect_in_expr(fc.name, source, cursor, uri, sv, out) {
                return true;
            }
            for arg in fc.args.iter() {
                if collect_in_expr(&arg.value, source, cursor, uri, sv, out) {
                    return true;
                }
            }
            false
        }
        ExprKind::MethodCall(mc) | ExprKind::NullsafeMethodCall(mc) => {
            for arg in mc.args.iter() {
                if collect_in_expr(&arg.value, source, cursor, uri, sv, out) {
                    return true;
                }
            }
            false
        }
        ExprKind::StaticMethodCall(smc) => {
            for arg in smc.args.iter() {
                if collect_in_expr(&arg.value, source, cursor, uri, sv, out) {
                    return true;
                }
            }
            false
        }
        ExprKind::Parenthesized(inner) => collect_in_expr(inner, source, cursor, uri, sv, out),
        _ => false,
    }
}

fn build_action(
    closure: &ClosureExpr<'_, '_>,
    span: Span,
    source: &str,
    uri: &Url,
    sv: SourceView<'_>,
) -> Option<CodeActionOrCommand> {
    // Reject closures with by-ref `use` captures: arrow functions auto-capture
    // by value and have no equivalent for `use (&$x)`.
    if closure.use_vars.iter().any(|v| v.by_ref) {
        return None;
    }

    // Body must be exactly one `return` statement with an expression.
    if closure.body.stmts.len() != 1 {
        return None;
    }
    let StmtKind::Return(Some(return_expr)) = &closure.body.stmts[0].kind else {
        return None;
    };

    // Generator closures (`yield`) cannot become arrow functions.
    if is_yield(return_expr) {
        return None;
    }

    let new_text = build_arrow_text(closure, span, return_expr, source)?;

    let edit_range = Range {
        start: sv.position_of(span.start),
        end: sv.position_of(span.end),
    };

    let mut changes = HashMap::new();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: edit_range,
            new_text,
        }],
    );

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Convert to arrow function".to_string(),
        kind: Some(CodeActionKind::REFACTOR),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

/// Build the arrow-function source text for the given closure.
///
/// The params are extracted verbatim from source (preserving original
/// formatting); the return type and body expression are sliced the same way.
fn build_arrow_text(
    closure: &ClosureExpr<'_, '_>,
    span: Span,
    return_expr: &Expr<'_, '_>,
    source: &str,
) -> Option<String> {
    let closure_start = span.start as usize;

    // Find the opening `(` of the parameter list by scanning from the closure start.
    let open_paren = source[closure_start..].find('(')? + closure_start;
    let close_paren = find_close_paren(source.as_bytes(), open_paren)?;

    let params_text = &source[open_paren..=close_paren];

    let ret_text = if let Some(rt) = &closure.return_type {
        format!(
            ": {}",
            &source[rt.span.start as usize..rt.span.end as usize]
        )
    } else {
        String::new()
    };

    let expr_text = &source[return_expr.span.start as usize..return_expr.span.end as usize];

    let static_prefix = if closure.is_static { "static " } else { "" };
    let by_ref_marker = if closure.by_ref { "&" } else { "" };

    Some(format!(
        "{static_prefix}fn{by_ref_marker}{params_text}{ret_text} => {expr_text}"
    ))
}

/// Find the index of the `)` that closes the `(` at `open`.
/// Tracks paren depth to handle nested calls in default values.
fn find_close_paren(source: &[u8], open: usize) -> Option<usize> {
    debug_assert_eq!(source[open], b'(');
    let mut depth = 0i32;
    for (i, &b) in source[open..].iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Return `true` if the expression is a `yield` — generator closures cannot
/// be converted to arrow functions.
fn is_yield(expr: &Expr<'_, '_>) -> bool {
    matches!(expr.kind, ExprKind::Yield(_))
}
