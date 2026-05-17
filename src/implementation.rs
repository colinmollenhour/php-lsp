/// `textDocument/implementation` — find all classes that implement an interface
/// or extend a class with the given name.
use std::sync::Arc;

use php_ast::{NamespaceBody, Stmt, StmtKind};
use tower_lsp::lsp_types::{Location, Url};

use crate::ast::{ParsedDoc, SourceView};

/// Returns `true` when the name written in an `extends`/`implements` clause
/// (given as its `to_string_repr()` string) refers to the symbol we are
/// searching for.
///
/// Two forms are accepted:
/// - Short-name match: `repr == word`
///   Covers the common case where both files use the same unqualified name.
/// - FQN match: `repr` (with any leading `\` stripped) `== fqn`
///   Covers files that write the fully-qualified form (`\App\Animal` or
///   `App\Animal`) while the cursor file imports the class with a `use`
///   statement and the cursor sits on the short alias.
// TODO: when `fqn` is `None` and `word` contains no namespace separator,
// `\word` (a global-namespace backslash-qualified form) is not matched.
// Example: searching for bare `Animal` (cursor on a global-namespace interface
// with no `use` import) will miss any class that writes `extends \Animal`.
// Fix: treat `repr.trim_start_matches('\\') == word` as a match when `word`
// has no `\` and `fqn` is `None`.
#[inline]
fn name_matches(repr: &str, word: &str, fqn: Option<&str>) -> bool {
    repr == word || fqn.is_some_and(|f| repr.trim_start_matches('\\') == f)
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

/// Phase J — Find implementations via the salsa-memoized workspace aggregate.
/// Uses the pre-built `subtypes_of[word]` reverse map for O(matches) lookups,
/// with an additional pass over the FQN's `subtypes_of` entry when the caller
/// supplied one (covers classes that wrote out the fully-qualified form in
/// their `extends`/`implements` clause). Replaces the old
/// `find_implementations_from_index` which walked every file's classes.
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
                    let implements_match = cls
                        .implements
                        .iter()
                        .any(|iface| name_matches(iface.as_ref(), word, fqn));
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

                // TODO: anonymous classes (`c.name == None`) are silently skipped.
                // They implement interfaces but have no name to navigate to.
                // A future fix could emit the location of the `new class` keyword instead.
                if (extends_match || implements_match)
                    && let Some(class_name) = c.name
                {
                    out.push(Location {
                        uri: uri.clone(),
                        range: sv.name_range(&class_name.to_string()),
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
                        range: sv.name_range(&e.name.to_string()),
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
                        range: sv.name_range(&i.name.to_string()),
                    });
                }
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body {
                    collect_implementations(inner, word, fqn, sv, uri, out);
                }
            }
            _ => {}
        }
    }
}
