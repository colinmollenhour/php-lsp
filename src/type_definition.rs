/// `textDocument/typeDefinition` — jump to the class declaration of the type
/// of the symbol under the cursor.
///
/// Works for variables assigned via `$var = new ClassName()` (leverages `TypeMap`)
/// and for function parameters with a declared type hint.
use std::sync::Arc;

use php_ast::{ClassMemberKind, NamespaceBody, Stmt, StmtKind};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::ast::{MethodReturnsMap, ParsedDoc, SourceView, format_type_hint, str_offset_in_range};
use crate::type_map::TypeMap;
use crate::util::word_at;

/// Given the cursor position, resolve the type of the symbol and return the
/// location of that type's class/interface declaration.
pub fn goto_type_definition(
    source: &str,
    doc: &ParsedDoc,
    doc_returns: Option<&MethodReturnsMap>,
    all_docs: &[(Url, Arc<ParsedDoc>)],
    position: Position,
) -> Option<Location> {
    let word = word_at(source, position)?;

    let type_map = TypeMap::from_doc_with_meta(doc, None, doc_returns);
    let class_name = if word.starts_with('$') {
        type_map.get(&word)?.to_string()
    } else {
        param_type_for(&doc.program().stmts, &word)?
    };

    for (uri, other_doc) in all_docs {
        let other_sv = other_doc.view();
        if let Some(range) = find_class_range(other_sv, &other_doc.program().stmts, &class_name) {
            return Some(Location {
                uri: uri.clone(),
                range,
            });
        }
    }
    None
}

/// Look up the declared type hint for a parameter named `word` in any function/method.
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
    use crate::util::word_at;
    let word = word_at(source, position)?;

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

    for (uri, idx) in indexes {
        for cls in &idx.classes {
            // Match by short name (last segment after `\`).
            let short = cls
                .name
                .as_ref()
                .rsplit('\\')
                .next()
                .unwrap_or(cls.name.as_ref());
            let cn_short = class_name
                .rsplit('\\')
                .next()
                .unwrap_or(class_name.as_str());
            if cls.name.as_ref() == class_name || short == cn_short {
                return Some(Location {
                    uri: uri.clone(),
                    range: line_range(cls.start_line),
                });
            }
        }
    }
    None
}

fn _offset_to_position_range(sv: SourceView<'_>, name_str: &str, _name: &str) -> Range {
    let start = sv.position_of(0);
    Range {
        start,
        end: Position {
            line: start.line,
            character: start.character
                + name_str.chars().map(|c| c.len_utf16() as u32).sum::<u32>(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(path: &str) -> Url {
        Url::parse(&format!("file://{path}")).unwrap()
    }

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn doc(path: &str, src: &str) -> (Url, Arc<ParsedDoc>) {
        (uri(path), Arc::new(ParsedDoc::parse(src.to_string())))
    }

    #[test]
    fn resolves_variable_type_to_class() {
        let src = "<?php\nclass Foo {}\n$obj = new Foo();\n$obj->bar();";
        let parsed = ParsedDoc::parse(src.to_string());
        let docs = vec![(uri("/a.php"), Arc::new(ParsedDoc::parse(src.to_string())))];
        let loc = goto_type_definition(src, &parsed, None, &docs, pos(3, 2));
        assert!(loc.is_some(), "expected type definition for $obj");
        assert_eq!(loc.unwrap().range.start.line, 1);
    }

    #[test]
    fn cross_file_type_definition() {
        let src = "<?php\n$obj = new Mailer();\n$obj->send();";
        let parsed = ParsedDoc::parse(src.to_string());
        let other_src = "<?php\nclass Mailer {}";
        let other_uri = uri("/mailer.php");
        let docs = vec![
            doc("/a.php", src),
            (
                other_uri.clone(),
                Arc::new(ParsedDoc::parse(other_src.to_string())),
            ),
        ];
        let loc = goto_type_definition(src, &parsed, None, &docs, pos(2, 2));
        assert!(loc.is_some());
        assert_eq!(loc.unwrap().uri, other_uri);
    }

    #[test]
    fn unknown_variable_returns_none() {
        let src = "<?php\n$unknown->foo();";
        let parsed = ParsedDoc::parse(src.to_string());
        let docs = vec![doc("/a.php", src)];
        let loc = goto_type_definition(src, &parsed, None, &docs, pos(1, 2));
        assert!(loc.is_none());
    }

    #[test]
    fn resolves_interface_type() {
        let src = "<?php\ninterface Countable {}\n$obj = new MyList();\nclass MyList implements Countable {}";
        let parsed = ParsedDoc::parse(src.to_string());
        let docs = vec![doc("/a.php", src)];
        let loc = goto_type_definition(src, &parsed, None, &docs, pos(2, 2));
        assert!(loc.is_some());
        assert_eq!(loc.unwrap().range.start.line, 3);
    }

    #[test]
    fn returns_none_for_non_variable_without_type() {
        let src = "<?php\nfunction greet() {}\ngreet();";
        let parsed = ParsedDoc::parse(src.to_string());
        let docs = vec![doc("/a.php", src)];
        let loc = goto_type_definition(src, &parsed, None, &docs, pos(2, 2));
        assert!(loc.is_none());
    }

    #[test]
    fn resolves_enum_typed_param() {
        // Cursor on `$s` in the function body — TypeMap infers Status from the typed param.
        let src = "<?php\nenum Status { case Active; }\nfunction process(Status $s): void { $s-> }";
        let parsed = ParsedDoc::parse(src.to_string());
        let docs = vec![doc("/a.php", src)];
        // "function process(Status $s): void { " is 37 chars, so $s is at col 37.
        let loc = goto_type_definition(src, &parsed, None, &docs, pos(2, 37));
        assert!(
            loc.is_some(),
            "expected type definition for Status-typed param"
        );
        assert_eq!(loc.unwrap().range.start.line, 1);
    }

    #[test]
    fn resolves_trait_typed_param() {
        // Cursor on `$l` in the function body — TypeMap infers Logger from the typed param.
        let src = "<?php\ntrait Logger {}\nfunction process(Logger $l): void { $l-> }";
        let parsed = ParsedDoc::parse(src.to_string());
        let docs = vec![doc("/a.php", src)];
        // "function process(Logger $l): void { " is 37 chars, so $l is at col 37.
        let loc = goto_type_definition(src, &parsed, None, &docs, pos(2, 37));
        assert!(
            loc.is_some(),
            "expected type definition for trait-typed param"
        );
        assert_eq!(loc.unwrap().range.start.line, 1);
    }

    // ── goto_type_definition_from_index ───────────────────────────────────────

    fn make_index(path: &str, src: &str) -> (Url, std::sync::Arc<crate::file_index::FileIndex>) {
        use crate::file_index::FileIndex;
        let u = uri(path);
        let d = ParsedDoc::parse(src.to_string());
        (u.clone(), std::sync::Arc::new(FileIndex::extract(&d)))
    }

    #[test]
    fn from_index_resolves_variable_to_cross_file_class() {
        // Current file infers $obj → Mailer via new Mailer().
        // Mailer class lives in mailer.php (background-indexed, not in open_docs).
        let src = "<?php\n$obj = new Mailer();\n$obj->send();";
        let parsed = ParsedDoc::parse(src.to_string());
        let (mailer_uri, mailer_idx) = make_index(
            "/mailer.php",
            "<?php\nclass Mailer { public function send(): void {} }",
        );
        let indexes = vec![(mailer_uri.clone(), mailer_idx)];
        // Cursor on $obj in "$obj->send();" — line 2, char 2.
        let loc = goto_type_definition_from_index(src, &parsed, None, &indexes, pos(2, 2));
        assert!(
            loc.is_some(),
            "expected type definition for $obj (Mailer) in index"
        );
        assert_eq!(loc.unwrap().uri, mailer_uri, "should point to mailer.php");
    }
}
