/// `textDocument/implementation` — find all classes that implement an interface
/// or extend a class with the given name.
use std::collections::HashSet;
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

/// Find all concrete implementations of a METHOD across the subtypes of its
/// declaring class/interface.
///
/// When the cursor sits on a method name inside an interface or abstract class,
/// this returns the same-named method in every class that extends or implements
/// the declaring type. Uses the workspace aggregate's `subtypes_of` reverse map
/// for an O(subtypes) lookup instead of a full corpus walk.
pub fn find_method_implementations_from_workspace(
    method_name: &str,
    declaring_class: &str,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
) -> Vec<tower_lsp::lsp_types::Location> {
    let mut locations = Vec::new();
    if let Some(refs) = wi.subtypes_of.get(declaring_class) {
        for &class_ref in refs {
            if let Some((uri, cls)) = wi.at(class_ref)
                && let Some(method) = cls
                    .methods
                    .iter()
                    .find(|m| m.name.as_ref() == method_name && !m.is_abstract)
            {
                locations.push(tower_lsp::lsp_types::Location {
                    uri: uri.clone(),
                    range: crate::text::zero_width_range(method.start_line),
                });
            }
        }
    }
    locations.sort_by(|a, b| {
        a.uri
            .as_str()
            .cmp(b.uri.as_str())
            .then(a.range.start.line.cmp(&b.range.start.line))
    });
    locations.dedup_by(|a, b| a.uri == b.uri && a.range.start.line == b.range.start.line);
    locations
}

/// Phase J — Find implementations via the salsa-memoized workspace aggregate.
/// Uses the pre-built `subtypes_of[word]` reverse map for O(matches) lookups,
/// with an additional pass over the FQN's `subtypes_of` entry when the caller
/// supplied one (covers classes that wrote out the fully-qualified form in
/// their `extends`/`implements` clause).
pub fn find_implementations_from_workspace(
    word: &str,
    fqn: Option<&str>,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
) -> Vec<Location> {
    let mut locations = Vec::new();
    let mut push_refs = |key: &str| {
        if let Some(refs) = wi.subtypes_of.get(key) {
            for r in refs {
                if let Some((uri, cls)) = wi.at(*r) {
                    // Re-check with `name_matches` so a bare-name subtype_of
                    // entry survives an FQN-qualified search and vice versa.
                    let extends_match = cls
                        .parent
                        .as_deref()
                        .map(|p| name_matches(p, word, fqn))
                        .unwrap_or(false);
                    let implements_match = cls.implements.iter().any(|iface| {
                        if name_matches(iface.as_ref(), word, fqn) {
                            return true;
                        }
                        // The implements clause may use a use-import alias for `word`.
                        // e.g. `use A\B\Factory as FactoryContract` + `implements FactoryContract`
                        // → iface = "FactoryContract", word = "Factory"
                        if let Some((_, file_idx)) = wi.files.get(r.file as usize) {
                            file_idx.use_imports.iter().any(|(alias, resolved_fqn)| {
                                alias.as_ref() == iface.as_ref()
                                    && crate::text::fqn_short_name(resolved_fqn) == word
                            })
                        } else {
                            false
                        }
                    });
                    if extends_match || implements_match {
                        let pos = tower_lsp::lsp_types::Position {
                            line: cls.start_line,
                            character: 0,
                        };
                        locations.push(Location {
                            uri: uri.clone(),
                            range: tower_lsp::lsp_types::Range {
                                start: pos,
                                end: pos,
                            },
                        });
                    }
                }
            }
        }
    };
    push_refs(word);
    if let Some(f) = fqn
        && f != word
    {
        push_refs(f);
        // Cover `\App\Animal`-style leading-backslash forms.
        let trimmed = f.trim_start_matches('\\');
        if trimmed != f {
            push_refs(trimmed);
        }
    }
    // De-dup: a class may list both the bare name and the FQN of the same
    // parent (unlikely but cheap to guard against).
    locations.sort_by(|a, b| {
        a.uri
            .as_str()
            .cmp(b.uri.as_str())
            .then(a.range.start.line.cmp(&b.range.start.line))
    });
    locations.dedup_by(|a, b| a.uri == b.uri && a.range.start.line == b.range.start.line);
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

/// Mir-backed variant of [`find_implementations_from_workspace`].
///
/// `subtype_urls` is the set of files mir identified as containing subtypes of
/// the target (from `DocumentStore::class_subtype_urls`). Fixes aliased
/// `extends` (`use App\Base as X; class C extends X {}`) and FQN-qualified
/// forms that the raw-name `subtypes_of` map misses. Falls back to
/// [`find_implementations_from_workspace`] when `subtype_urls` is empty (cold
/// mir session or class not yet ingested).
pub fn find_implementations_mir_backed(
    word: &str,
    fqn: Option<&str>,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
    subtype_urls: &[Url],
) -> Vec<Location> {
    if subtype_urls.is_empty() {
        return find_implementations_from_workspace(word, fqn, wi);
    }
    let url_set: HashSet<&Url> = subtype_urls.iter().collect();
    let mut locations = Vec::new();
    for (uri, idx) in &wi.files {
        if !url_set.contains(uri) {
            continue;
        }
        for cls in &idx.classes {
            let extends_match = cls
                .parent
                .as_deref()
                .map(|p| {
                    name_matches(p, word, fqn)
                        || fqn.is_some_and(|f| alias_resolves_to(p, f, &idx.use_imports))
                })
                .unwrap_or(false);
            let implements_match = cls.implements.iter().any(|iface| {
                name_matches(iface.as_ref(), word, fqn)
                    || fqn.is_some_and(|f| alias_resolves_to(iface.as_ref(), f, &idx.use_imports))
            });
            if extends_match || implements_match {
                let pos = tower_lsp::lsp_types::Position {
                    line: cls.start_line,
                    character: 0,
                };
                locations.push(Location {
                    uri: uri.clone(),
                    range: tower_lsp::lsp_types::Range {
                        start: pos,
                        end: pos,
                    },
                });
            }
        }
    }
    locations.sort_by(|a, b| {
        a.uri
            .as_str()
            .cmp(b.uri.as_str())
            .then(a.range.start.line.cmp(&b.range.start.line))
    });
    locations.dedup_by(|a, b| a.uri == b.uri && a.range.start.line == b.range.start.line);
    locations
}

/// Mir-backed variant of [`find_method_implementations_from_workspace`].
///
/// `subtype_urls` is the set of files mir identified as containing subtypes of
/// the declaring class. Scopes the search to those files instead of walking
/// the raw-name `subtypes_of` map. Falls back to
/// [`find_method_implementations_from_workspace`] when `subtype_urls` is empty.
pub fn find_method_implementations_mir_backed(
    method_name: &str,
    declaring_class: &str,
    wi: &crate::db::workspace_index::WorkspaceIndexData,
    subtype_urls: &[Url],
) -> Vec<tower_lsp::lsp_types::Location> {
    if subtype_urls.is_empty() {
        return find_method_implementations_from_workspace(method_name, declaring_class, wi);
    }
    let url_set: HashSet<&Url> = subtype_urls.iter().collect();
    let mut locations = Vec::new();
    for (uri, idx) in &wi.files {
        if !url_set.contains(uri) {
            continue;
        }
        for cls in &idx.classes {
            if let Some(method) = cls
                .methods
                .iter()
                .find(|m| m.name.as_ref() == method_name && !m.is_abstract)
            {
                locations.push(tower_lsp::lsp_types::Location {
                    uri: uri.clone(),
                    range: crate::text::zero_width_range(method.start_line),
                });
            }
        }
    }
    locations.sort_by(|a, b| {
        a.uri
            .as_str()
            .cmp(b.uri.as_str())
            .then(a.range.start.line.cmp(&b.range.start.line))
    });
    locations.dedup_by(|a, b| a.uri == b.uri && a.range.start.line == b.range.start.line);
    locations
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
