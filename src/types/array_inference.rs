//! Targeted array-element type inference for the array_map/foreach pattern.
//!
//! mir's `array_map` stub returns plain `array`, so mir's flow analysis
//! propagates `mixed` to foreach value variables.  Until mir gains generic
//! inference for `array_map`, these helpers provide a direct AST-based path
//! that extracts the callback return type without building TypeMap's full
//! flat map.
//!
//! Call hierarchy:
//!   inlay_hints: foreach value/key offset → mir → fallback via `array_map_element_class`
//!   completion:  receiver var → mir → fallback via `find_foreach_array_var` +
//!                `array_map_element_class`

use php_ast::{Expr, ExprKind, Stmt, StmtKind, TypeHintKind};

use crate::text::fqn_short_name;

/// Given the expression on the right-hand side of an assignment, check whether
/// it is an `array_map` or `array_filter` call with a typed closure/arrow-
/// function callback.  Returns the short class name from the callback's return
/// type hint, or `None`.
///
/// Example: `array_map(fn($x): User => $x, $arr)` → `Some("User")`
pub(crate) fn array_map_element_class<'a>(expr: &'a Expr<'a, '_>) -> Option<String> {
    let ExprKind::FunctionCall(call) = &expr.kind else {
        return None;
    };
    let fn_name = match &call.name.kind {
        ExprKind::Identifier(n) => n.as_str(),
        _ => return None,
    };
    if fn_name != "array_map" && fn_name != "array_filter" {
        return None;
    }
    let callback_arg = call.args.first()?;
    callback_return_class(&callback_arg.value)
}

/// Extract the return-type class name from a Closure or ArrowFunction.
/// Returns the short name only when it starts with an uppercase letter (i.e.
/// looks like a class name, not a primitive like `int` or `string`).
fn callback_return_class(expr: &Expr<'_, '_>) -> Option<String> {
    let hint = match &expr.kind {
        ExprKind::Closure(c) => c.return_type.as_ref()?,
        ExprKind::ArrowFunction(af) => af.return_type.as_ref()?,
        _ => return None,
    };
    if let TypeHintKind::Named(name) = &hint.kind {
        let s = name.to_string_repr();
        let base = s.trim_start_matches('\\');
        let short = fqn_short_name(base);
        if short
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            return Some(short.to_string());
        }
    }
    None
}

/// Scan `stmts` for a simple assignment `$arr_var_name = array_map(fn(): T => ..., ...)`.
/// Returns the element class name `T` if found, `None` otherwise.
pub(crate) fn scan_array_map_return(stmts: &[Stmt<'_, '_>], arr_var_name: &str) -> Option<String> {
    for stmt in stmts {
        let StmtKind::Expression(expr) = &stmt.kind else {
            continue;
        };
        let ExprKind::Assign(assign) = &expr.kind else {
            continue;
        };
        let ExprKind::Variable(lhs) = &assign.target.kind else {
            continue;
        };
        if lhs.as_str() != arr_var_name {
            continue;
        }
        if let Some(class) = array_map_element_class(assign.value) {
            return Some(class);
        }
    }
    None
}

/// Walk `stmts` scanning for `$arr = array_map(fn(): T => ..., ...)` at the
/// expression-statement level and return a map of `arr_var_name → element_class`
/// (both without `$`).  Used by the inlay-hints pre-pass.
pub(crate) fn collect_array_map_returns(
    stmts: &[Stmt<'_, '_>],
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for stmt in stmts {
        let StmtKind::Expression(expr) = &stmt.kind else {
            continue;
        };
        let ExprKind::Assign(assign) = &expr.kind else {
            continue;
        };
        let ExprKind::Variable(lhs) = &assign.target.kind else {
            continue;
        };
        if let Some(class) = array_map_element_class(assign.value) {
            map.insert(lhs.as_str().to_string(), class);
        }
    }
    map
}

/// Walk `stmts` looking for a `foreach` that binds its value variable to
/// `val_var_name` and iterates over a simple variable.  Returns the name of
/// that iterated variable (without `$`) so the caller can then check
/// [`scan_array_map_return`].
///
/// `val_var_name` must be WITHOUT the leading `$`.
///
/// Used by the completion fallback: given `"item"` (from `$item`), find the
/// `foreach ($users as $item)` and return `"users"`.
pub(crate) fn find_foreach_array_var<'a>(
    stmts: &'a [Stmt<'_, '_>],
    val_var_name: &str,
) -> Option<&'a str> {
    find_foreach_array_var_in(stmts, val_var_name)
}

fn find_foreach_array_var_in<'a>(stmts: &'a [Stmt<'_, '_>], val_var_name: &str) -> Option<&'a str> {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Foreach(f) => {
                if let ExprKind::Variable(val_name) = &f.value.kind
                    && val_name.as_str() == val_var_name
                    && let ExprKind::Variable(arr_name) = &f.expr.kind
                {
                    return Some(arr_name.as_str());
                }
                // Recurse into body.
                if let Some(found) =
                    find_foreach_array_var_in(std::slice::from_ref(f.body), val_var_name)
                {
                    return Some(found);
                }
            }
            StmtKind::If(i) => {
                if let Some(found) =
                    find_foreach_array_var_in(std::slice::from_ref(i.then_branch), val_var_name)
                {
                    return Some(found);
                }
                for ei in i.elseif_branches.iter() {
                    if let Some(found) =
                        find_foreach_array_var_in(std::slice::from_ref(&ei.body), val_var_name)
                    {
                        return Some(found);
                    }
                }
                if let Some(else_branch) = &i.else_branch
                    && let Some(found) =
                        find_foreach_array_var_in(std::slice::from_ref(else_branch), val_var_name)
                {
                    return Some(found);
                }
            }
            StmtKind::While(w) => {
                if let Some(found) =
                    find_foreach_array_var_in(std::slice::from_ref(w.body), val_var_name)
                {
                    return Some(found);
                }
            }
            StmtKind::TryCatch(t) => {
                if let Some(found) = find_foreach_array_var_in(&t.body.stmts, val_var_name) {
                    return Some(found);
                }
                for catch in t.catches.iter() {
                    if let Some(found) = find_foreach_array_var_in(&catch.body.stmts, val_var_name)
                    {
                        return Some(found);
                    }
                }
                if let Some(finally) = &t.finally
                    && let Some(found) = find_foreach_array_var_in(&finally.stmts, val_var_name)
                {
                    return Some(found);
                }
            }
            StmtKind::Block(b) => {
                if let Some(found) = find_foreach_array_var_in(&b.stmts, val_var_name) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}
