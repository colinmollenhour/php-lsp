/// `textDocument/implementation` — find all classes that implement an interface
/// or extend a class with the given name.
use std::sync::Arc;

use php_ast::{ExprKind, NamespaceBody, Stmt, StmtKind};
use tower_lsp::lsp_types::{Location, Url};

use crate::document::ast::{ParsedDoc, SourceView};

/// Returns `true` when the name written in an `extends`/`implements` clause
/// (given as its `to_string_repr()` string) refers to the symbol we are
/// searching for.
///
/// Three forms are accepted:
/// - Short-name match: `repr == word`
///   Covers the common case where both files use the same unqualified name.
/// - FQN match: `repr` (with any leading `\` stripped) `== fqn`
///   Covers files that write the fully-qualified form (`\App\Animal` or
///   `App\Animal`) while the cursor file imports the class with a `use`
///   statement and the cursor sits on the short alias.
/// - Global-namespace backslash match: `repr.trim_start_matches('\\') == word`
///   when `fqn` is `None` and `word` has no namespace separator.
///   Covers the case where a class writes `extends \Animal` (explicit global-
///   namespace form) and the cursor sits on a global-namespace `Animal`
///   interface with no `use` import.
#[inline]
pub(crate) fn name_matches(repr: &str, word: &str, fqn: Option<&str>) -> bool {
    repr == word
        || fqn.is_some_and(|f| repr.trim_start_matches('\\') == f)
        || (fqn.is_none() && !word.contains('\\') && repr.trim_start_matches('\\') == word)
}

/// Return all `Location`s where a class declares `extends Name` or
/// `implements Name`.
///
/// `fqn` is the fully-qualified name of the symbol (e.g. `"App\\Animal"`),
/// resolved from the calling file's `use` imports. When provided, extends/
/// implements clauses that spell out the FQN form (`\App\Animal` or
/// `App\Animal`) are also matched, in addition to the bare `word`.
pub fn find_implementations(
    word: &str,
    fqn: Option<&str>,
    all_docs: &[(Url, Arc<ParsedDoc>)],
) -> Vec<Location> {
    let mut locations = Vec::new();
    for (uri, doc) in all_docs {
        let sv = doc.view();
        collect_implementations(&doc.program().stmts, word, fqn, sv, uri, &mut locations);
    }
    locations
}

fn collect_implementations(
    stmts: &[Stmt<'_, '_>],
    word: &str,
    fqn: Option<&str>,
    sv: SourceView<'_>,
    uri: &Url,
    out: &mut Vec<Location>,
) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Class(c) => {
                let extends_match = c
                    .extends
                    .as_ref()
                    .map(|e| name_matches(e.to_string_repr().as_ref(), word, fqn))
                    .unwrap_or(false);

                let implements_match = c
                    .implements
                    .iter()
                    .any(|iface| name_matches(iface.to_string_repr().as_ref(), word, fqn));

                if extends_match || implements_match {
                    let range = if let Some(class_name) = c.name {
                        sv.name_range_in_span(class_name.or_error(), stmt.span)
                    } else {
                        // Anonymous class (`new class {}`): point to the `class` keyword.
                        sv.name_range_in_span("class", stmt.span)
                    };
                    out.push(Location {
                        uri: uri.clone(),
                        range,
                    });
                }
            }
            StmtKind::Enum(e) => {
                let implements_match = e
                    .implements
                    .iter()
                    .any(|iface| name_matches(iface.to_string_repr().as_ref(), word, fqn));
                if implements_match {
                    out.push(Location {
                        uri: uri.clone(),
                        range: sv.name_range_in_span(e.name.or_error(), stmt.span),
                    });
                }
            }
            StmtKind::Interface(i) => {
                let extends_match = i
                    .extends
                    .iter()
                    .any(|base| name_matches(base.to_string_repr().as_ref(), word, fqn));
                if extends_match {
                    out.push(Location {
                        uri: uri.clone(),
                        range: sv.name_range_in_span(i.name.or_error(), stmt.span),
                    });
                }
            }
            StmtKind::Expression(expr) => {
                collect_anon_class_in_expr(expr, word, fqn, sv, stmt.span, uri, out);
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body {
                    collect_implementations(&inner.stmts, word, fqn, sv, uri, out);
                }
            }
            _ => {}
        }
    }
}

/// Returns `true` when `name` is a use-import alias in `use_imports` that
/// resolves to `fqn`. Handles both `Ns\Name` and `\Ns\Name` stored forms.
pub(crate) fn alias_resolves_to(
    name: &str,
    fqn: &str,
    use_imports: &[(Box<str>, Box<str>)],
) -> bool {
    use_imports.iter().any(|(alias, resolved)| {
        alias.as_ref() == name
            && (resolved.as_ref() == fqn || resolved.trim_start_matches('\\') == fqn)
    })
}

/// Returns `true` when `written` — a name from an `extends`/`implements`/`use`
/// clause in `idx`'s file — refers to `target_fqn`.
///
/// Unlike [`name_matches`], this does not treat a bare short-name match as
/// automatically correct: when `written` has no explicit FQN form, it is
/// resolved through `idx.use_imports` first, and an entry found there is
/// authoritative even if its resolved FQN differs from `target_fqn` (an
/// explicit import always shadows a same-named symbol elsewhere). Only when
/// no import shadows `written` does it fall back to implicit same-namespace
/// resolution, mirroring how PHP resolves an unqualified class name with no
/// matching `use` statement to `<current-namespace>\<written>`.
///
/// This is the disambiguation the raw `subtypes_of` short-name prefilter
/// cannot do on its own: many unrelated classes across a large workspace can
/// share both a short name (e.g. `Factory`) and a same-named `use` alias
/// (e.g. `FactoryContract`) while resolving to entirely different FQNs.
pub(crate) fn resolves_to_fqn(
    written: &str,
    target_fqn: &str,
    idx: &crate::index::file_index::FileIndex,
) -> bool {
    if written.contains('\\') {
        return written.trim_start_matches('\\') == target_fqn;
    }
    if let Some((_, resolved)) = idx
        .use_imports
        .iter()
        .find(|(alias, _)| alias.as_ref() == written)
    {
        return resolved.as_ref() == target_fqn || resolved.trim_start_matches('\\') == target_fqn;
    }
    match idx.namespace.as_deref() {
        Some(ns) => format!("{ns}\\{written}") == target_fqn,
        None => written == target_fqn,
    }
}

/// Recurse into an expression to find `new class {}` anonymous class declarations
/// that implement or extend the target interface/class.
fn collect_anon_class_in_expr(
    expr: &php_ast::Expr<'_, '_>,
    word: &str,
    fqn: Option<&str>,
    sv: SourceView<'_>,
    stmt_span: php_ast::Span,
    uri: &Url,
    out: &mut Vec<Location>,
) {
    match &expr.kind {
        ExprKind::AnonymousClass(c) => {
            let extends_match = c
                .extends
                .as_ref()
                .map(|e| name_matches(e.to_string_repr().as_ref(), word, fqn))
                .unwrap_or(false);
            let implements_match = c
                .implements
                .iter()
                .any(|iface| name_matches(iface.to_string_repr().as_ref(), word, fqn));
            if extends_match || implements_match {
                // Emit the `class` keyword within the expression span as the location.
                out.push(Location {
                    uri: uri.clone(),
                    range: sv.name_range_in_span("class", stmt_span),
                });
            }
        }
        ExprKind::New(n) => {
            collect_anon_class_in_expr(n.class, word, fqn, sv, stmt_span, uri, out);
        }
        ExprKind::Assign(a) => {
            collect_anon_class_in_expr(a.value, word, fqn, sv, stmt_span, uri, out);
        }
        _ => {}
    }
}
