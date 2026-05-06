/// `textDocument/declaration` — jump to the abstract or interface declaration of a symbol.
///
/// In PHP the distinction between declaration and definition matters for:
///   - Interface methods (declared but never given a body)
///   - Abstract class methods
///
/// For concrete symbols with no abstract counterpart this falls back to the same
/// result as go-to-definition so the request is never empty-handed.
use std::sync::Arc;

use php_ast::{ClassMemberKind, EnumMemberKind, NamespaceBody, Stmt, StmtKind};
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::ast::{ParsedDoc, SourceView};
use crate::util::word_at;

/// Find the abstract or interface declaration of `word`.
/// Prefers abstract/interface declarations; falls back to any declaration.
pub fn goto_declaration(
    source: &str,
    all_docs: &[(Url, Arc<ParsedDoc>)],
    position: Position,
) -> Option<Location> {
    let word = word_at(source, position)?;

    // First pass: look for an abstract or interface declaration
    for (uri, doc) in all_docs {
        let sv = doc.view();
        if let Some(range) = find_abstract_declaration(sv, &doc.program().stmts, &word) {
            return Some(Location {
                uri: uri.clone(),
                range,
            });
        }
    }

    // Second pass: any declaration (same as goto_definition)
    for (uri, doc) in all_docs {
        let sv = doc.view();
        if let Some(range) = find_any_declaration(sv, &doc.program().stmts, &word) {
            return Some(Location {
                uri: uri.clone(),
                range,
            });
        }
    }

    None
}

fn find_abstract_declaration(
    sv: SourceView<'_>,
    stmts: &[Stmt<'_, '_>],
    word: &str,
) -> Option<tower_lsp::lsp_types::Range> {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Interface(i) => {
                // Interface methods are declarations without bodies
                for member in i.members.iter() {
                    if let ClassMemberKind::Method(m) = &member.kind
                        && m.name == word
                    {
                        return Some(sv.name_range(m.name));
                    }
                }
                if i.name == word {
                    return Some(sv.name_range(i.name));
                }
            }
            StmtKind::Class(c) => {
                for member in c.members.iter() {
                    if let ClassMemberKind::Method(m) = &member.kind
                        && m.is_abstract
                        && m.name == word
                    {
                        return Some(sv.name_range(m.name));
                    }
                }
            }
            StmtKind::Trait(t) => {
                for member in t.members.iter() {
                    if let ClassMemberKind::Method(m) = &member.kind
                        && m.is_abstract
                        && m.name == word
                    {
                        return Some(sv.name_range(m.name));
                    }
                }
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body
                    && let Some(r) = find_abstract_declaration(sv, inner, word)
                {
                    return Some(r);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_any_declaration(
    sv: SourceView<'_>,
    stmts: &[Stmt<'_, '_>],
    word: &str,
) -> Option<tower_lsp::lsp_types::Range> {
    let bare = word.strip_prefix('$').unwrap_or(word);
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f) if f.name == word => {
                return Some(sv.name_range(f.name));
            }
            StmtKind::Class(c) if c.name == Some(word) => {
                return Some(sv.name_range(c.name.expect("match guard ensures Some")));
            }
            StmtKind::Class(c) => {
                for member in c.members.iter() {
                    match &member.kind {
                        ClassMemberKind::Method(m) if m.name == word => {
                            return Some(sv.name_range(m.name));
                        }
                        ClassMemberKind::ClassConst(cc) if cc.name == word => {
                            return Some(sv.name_range(cc.name));
                        }
                        ClassMemberKind::Property(p) if p.name == bare => {
                            return Some(sv.name_range(p.name));
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::Interface(i) => {
                if i.name == word {
                    return Some(sv.name_range(i.name));
                }
                for member in i.members.iter() {
                    match &member.kind {
                        ClassMemberKind::Method(m) if m.name == word => {
                            return Some(sv.name_range(m.name));
                        }
                        ClassMemberKind::ClassConst(cc) if cc.name == word => {
                            return Some(sv.name_range(cc.name));
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::Trait(t) => {
                if t.name == word {
                    return Some(sv.name_range(t.name));
                }
                for member in t.members.iter() {
                    match &member.kind {
                        ClassMemberKind::Method(m) if m.name == word => {
                            return Some(sv.name_range(m.name));
                        }
                        ClassMemberKind::ClassConst(cc) if cc.name == word => {
                            return Some(sv.name_range(cc.name));
                        }
                        ClassMemberKind::Property(p) if p.name == bare => {
                            return Some(sv.name_range(p.name));
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::Enum(e) if e.name == word => {
                return Some(sv.name_range(e.name));
            }
            StmtKind::Enum(e) => {
                for member in e.members.iter() {
                    match &member.kind {
                        EnumMemberKind::Case(c) if c.name == word => {
                            return Some(sv.name_range(c.name));
                        }
                        EnumMemberKind::Method(m) if m.name == word => {
                            return Some(sv.name_range(m.name));
                        }
                        EnumMemberKind::ClassConst(cc) if cc.name == word => {
                            return Some(sv.name_range(cc.name));
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body
                    && let Some(r) = find_any_declaration(sv, inner, word)
                {
                    return Some(r);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find abstract or interface declaration using `FileIndex` entries.
/// Returns line-only positions (character 0) for unopened files.
/// This is a limitation of the compact FileIndex — for opened files,
/// goto_declaration() provides precise name ranges.
pub fn goto_declaration_from_index(
    source: &str,
    indexes: &[(
        tower_lsp::lsp_types::Url,
        std::sync::Arc<crate::file_index::FileIndex>,
    )],
    position: tower_lsp::lsp_types::Position,
) -> Option<Location> {
    use crate::file_index::ClassKind;
    use crate::util::word_at;
    let word = word_at(source, position)?;
    let _bare = word.strip_prefix('$').unwrap_or(&word);

    let line_range = |line: u32| -> tower_lsp::lsp_types::Range {
        let p = tower_lsp::lsp_types::Position { line, character: 0 };
        tower_lsp::lsp_types::Range { start: p, end: p }
    };

    // First pass: abstract/interface declarations.
    for (uri, idx) in indexes {
        for cls in &idx.classes {
            match cls.kind {
                ClassKind::Interface => {
                    // Interface itself.
                    if cls.name == word {
                        return Some(Location {
                            uri: uri.clone(),
                            range: line_range(cls.start_line),
                        });
                    }
                    // Abstract method in interface.
                    for m in &cls.methods {
                        if m.name == word {
                            return Some(Location {
                                uri: uri.clone(),
                                range: line_range(m.start_line),
                            });
                        }
                    }
                }
                ClassKind::Trait => {
                    // Trait abstract methods.
                    for m in &cls.methods {
                        if m.is_abstract && m.name == word {
                            return Some(Location {
                                uri: uri.clone(),
                                range: line_range(m.start_line),
                            });
                        }
                    }
                }
                _ if cls.is_abstract => {
                    // Abstract methods in abstract classes.
                    for m in &cls.methods {
                        if m.is_abstract && m.name == word {
                            return Some(Location {
                                uri: uri.clone(),
                                range: line_range(m.start_line),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Second pass: any declaration.
    for (uri, idx) in indexes {
        // Top-level functions.
        for f in &idx.functions {
            if f.name == word {
                return Some(Location {
                    uri: uri.clone(),
                    range: line_range(f.start_line),
                });
            }
        }

        for cls in &idx.classes {
            // Class/Interface/Trait/Enum declarations.
            if cls.name == word {
                return Some(Location {
                    uri: uri.clone(),
                    range: line_range(cls.start_line),
                });
            }

            // Methods.
            for m in &cls.methods {
                if m.name == word {
                    return Some(Location {
                        uri: uri.clone(),
                        range: line_range(m.start_line),
                    });
                }
            }

            // TODO: Properties (Phase 2). Currently FileIndex stores properties per-class
            // but property lookup in unopened files requires finding the correct class context
            // first. Enable after extending FileIndex to store class-qualified names or adding
            // property line lookup.

            // Class/Interface/Trait/Enum constants.
            for c in &cls.constants {
                if c.as_str() == word {
                    return Some(Location {
                        uri: uri.clone(),
                        range: line_range(cls.start_line),
                    });
                }
            }

            // Enum cases (stored in separate `cases` field).
            if cls.kind == ClassKind::Enum {
                for case_name in &cls.cases {
                    if case_name.as_str() == word {
                        return Some(Location {
                            uri: uri.clone(),
                            range: line_range(cls.start_line),
                        });
                    }
                }
            }
        }
    }
    None
}
