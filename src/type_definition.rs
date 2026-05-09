/// `textDocument/typeDefinition` — jump to the class declaration of the type
/// of the symbol under the cursor.
///
/// Works for variables assigned via `$var = new ClassName()` (leverages `TypeMap`)
/// and for function parameters with a declared type hint.
use std::sync::Arc;

use php_ast::{ClassMemberKind, EnumMemberKind, NamespaceBody, Stmt, StmtKind};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::ast::{MethodReturnsMap, ParsedDoc, SourceView, format_type_hint, str_offset_in_range};
use crate::type_map::TypeMap;
use crate::util::word_at_position;

/// Given the cursor position, resolve the type of the symbol and return the
/// location of that type's class/interface declaration.
pub fn goto_type_definition(
    source: &str,
    doc: &ParsedDoc,
    doc_returns: Option<&MethodReturnsMap>,
    all_docs: &[(Url, Arc<ParsedDoc>)],
    position: Position,
) -> Option<Location> {
    let word = word_at_position(source, position)?;

    let type_map = TypeMap::from_doc_with_meta(doc, None, doc_returns);
    let class_name = if word.starts_with('$') {
        type_map.get(&word)?.to_string()
    } else {
        param_type_for(&doc.program().stmts, &word)?
    };

    for candidate in type_candidates(&class_name) {
        for (uri, other_doc) in all_docs {
            let other_sv = other_doc.view();
            if let Some(range) = find_class_range(other_sv, &other_doc.program().stmts, candidate) {
                return Some(Location {
                    uri: uri.clone(),
                    range,
                });
            }
        }
    }
    None
}

/// Decompose a formatted type hint into searchable class-name candidates.
/// `"?Foo"` → `["Foo"]`, `"Foo|Bar"` → `["Foo", "Bar"]`, `"Foo&Bar"` → `["Foo", "Bar"]`.
fn type_candidates(type_hint: &str) -> Vec<&str> {
    let hint = type_hint.strip_prefix('?').unwrap_or(type_hint);
    hint.split(['|', '&'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Look up the declared type hint for a parameter named `word` in any function/method.
/// Note: Returns the type hint as-is from format_type_hint. Unqualified type names
/// in non-global namespaces are not automatically qualified with namespace context.
/// This is a known limitation: resolving `Logger` in `namespace App\Service` to
/// `App\Service\Logger` would require source context to extract namespace names.
fn param_type_for(stmts: &[Stmt<'_, '_>], word: &str) -> Option<String> {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f) => {
                for p in f.params.iter() {
                    if p.name == word
                        && let Some(type_hint) = &p.type_hint
                    {
                        return Some(format_type_hint(type_hint));
                    }
                }
            }
            StmtKind::Class(c) => {
                for member in c.members.iter() {
                    if let ClassMemberKind::Method(m) = &member.kind {
                        for p in m.params.iter() {
                            if p.name == word
                                && let Some(type_hint) = &p.type_hint
                            {
                                return Some(format_type_hint(type_hint));
                            }
                        }
                    }
                }
            }
            StmtKind::Interface(i) => {
                for member in i.members.iter() {
                    if let ClassMemberKind::Method(m) = &member.kind {
                        for p in m.params.iter() {
                            if p.name == word
                                && let Some(type_hint) = &p.type_hint
                            {
                                return Some(format_type_hint(type_hint));
                            }
                        }
                    }
                }
            }
            StmtKind::Trait(trait_) => {
                for member in trait_.members.iter() {
                    if let ClassMemberKind::Method(m) = &member.kind {
                        for p in m.params.iter() {
                            if p.name == word
                                && let Some(type_hint) = &p.type_hint
                            {
                                return Some(format_type_hint(type_hint));
                            }
                        }
                    }
                }
            }
            StmtKind::Enum(e) => {
                for member in e.members.iter() {
                    if let EnumMemberKind::Method(m) = &member.kind {
                        for p in m.params.iter() {
                            if p.name == word
                                && let Some(type_hint) = &p.type_hint
                            {
                                return Some(format_type_hint(type_hint));
                            }
                        }
                    }
                }
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body
                    && let Some(type_hint) = param_type_for(inner, word)
                {
                    return Some(type_hint);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find the range of the class or interface declaration named `name`.
fn find_class_range(sv: SourceView<'_>, stmts: &[Stmt<'_, '_>], name: &str) -> Option<Range> {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Class(c)
                if c.name.as_ref().map(|n| n.to_string()) == Some(name.to_string()) =>
            {
                // Use statement span to find the name within the declaration context,
                // not the first occurrence in the file (which might be a different use).
                let stmt_range = sv.range_of(stmt.span);
                let name_in_source = c
                    .name
                    .as_ref()
                    .map(|n| n.to_string())
                    .expect("match guard ensures Some");
                if let Some(pos) = str_offset_in_range(sv.source(), stmt.span, &name_in_source) {
                    return Some(Range {
                        start: sv.position_of(pos),
                        end: sv.position_of(pos + name_in_source.len() as u32),
                    });
                }
                return Some(stmt_range);
            }
            StmtKind::Interface(i) if i.name == name => {
                // Use statement span to find the name within the declaration context.
                if let Some(pos) = str_offset_in_range(sv.source(), stmt.span, &i.name.to_string())
                {
                    return Some(Range {
                        start: sv.position_of(pos),
                        end: sv.position_of(pos + i.name.to_string().len() as u32),
                    });
                }
                return Some(sv.range_of(stmt.span));
            }
            StmtKind::Trait(t) if t.name == name => {
                // Use statement span to find the name within the declaration context.
                if let Some(pos) = str_offset_in_range(sv.source(), stmt.span, &t.name.to_string())
                {
                    return Some(Range {
                        start: sv.position_of(pos),
                        end: sv.position_of(pos + t.name.to_string().len() as u32),
                    });
                }
                return Some(sv.range_of(stmt.span));
            }
            StmtKind::Enum(e) if e.name == name => {
                // Use statement span to find the name within the declaration context.
                if let Some(pos) = str_offset_in_range(sv.source(), stmt.span, &e.name.to_string())
                {
                    return Some(Range {
                        start: sv.position_of(pos),
                        end: sv.position_of(pos + e.name.to_string().len() as u32),
                    });
                }
                return Some(sv.range_of(stmt.span));
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body
                    && let Some(r) = find_class_range(sv, inner, name)
                {
                    return Some(r);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find a type definition using `FileIndex` entries.
pub fn goto_type_definition_from_index(
    source: &str,
    doc: &ParsedDoc,
    doc_returns: Option<&MethodReturnsMap>,
    indexes: &[(Url, std::sync::Arc<crate::file_index::FileIndex>)],
    position: Position,
) -> Option<Location> {
    use crate::util::word_at_position;
    let word = word_at_position(source, position)?;

    let type_map = TypeMap::from_doc_with_meta(doc, None, doc_returns);
    let class_name = if word.starts_with('$') {
        type_map.get(&word)?.to_string()
    } else {
        param_type_for(&doc.program().stmts, &word)?
    };

    let line_range = |line: u32| -> Range {
        let p = Position { line, character: 0 };
        Range { start: p, end: p }
    };

    // First pass: look for exact FQN match (high priority)
    for candidate in type_candidates(&class_name) {
        for (uri, idx) in indexes {
            for cls in &idx.classes {
                if cls.name.as_ref() == candidate {
                    return Some(Location {
                        uri: uri.clone(),
                        range: line_range(cls.start_line),
                    });
                }
            }
        }
    }

    // Second pass: look for short name match (lower priority, may be ambiguous)
    for candidate in type_candidates(&class_name) {
        let cn_short = candidate.rsplit('\\').next().unwrap_or(candidate);
        for (uri, idx) in indexes {
            for cls in &idx.classes {
                let short = cls
                    .name
                    .as_ref()
                    .rsplit('\\')
                    .next()
                    .unwrap_or(cls.name.as_ref());
                if short == cn_short {
                    return Some(Location {
                        uri: uri.clone(),
                        range: line_range(cls.start_line),
                    });
                }
            }
        }
    }
    None
}
