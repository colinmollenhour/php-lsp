use std::cell::OnceCell;
use std::sync::Arc;

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Url};

use crate::document::ast::ParsedDoc;
use crate::lang::docblock::find_docblock;
use crate::lang::php_names::{is_php_builtin, php_doc_url};
use crate::text::{fqn_short_name, word_at_position, word_range_at};
use crate::types::resolve::{Declaration, resolve_declaration};
use crate::types::symbol_map::{SymbolMap, is_hoverable_kind};
use crate::types::type_map::TypeMap;

use super::closures::closure_hover;
use super::formatting::{declaration_signature, wrap_php};
use super::members::{
    find_property_info, resolve_method_docblock, scan_class_const_of_class,
    scan_enum_case_of_class, scan_method_of_class,
};
use super::named_args::{extract_named_arg_callee, is_named_arg_at, named_arg_hover_value};
use super::parsing::{extract_static_class_before_cursor, resolve_use_alias};

/// Hover handles every declaration kind except properties (covered by the
/// dedicated mir-primary path) and promoted parameters.
fn is_hoverable(decl: &Declaration<'_>) -> bool {
    !matches!(
        decl,
        Declaration::Property { .. } | Declaration::PromotedParam { .. }
    )
}

pub(crate) fn hover_info(
    source: &str,
    doc: &ParsedDoc,
    analysis: Option<&mir_analyzer::FileAnalysis>,
    position: Position,
    other_docs: &[(Url, Arc<ParsedDoc>)],
) -> Option<Hover> {
    hover_at(source, doc, analysis, other_docs, position, None)
}

/// Indexed variant: uses pre-computed [`SymbolMap`]s for the cross-file
/// declaration lookup (path 4/5), eliminating repeated AST walks on stable
/// files. All other paths (named-arg hover, mir-member hover, static-prop
/// hover) still use `other_docs` since they require full AST traversal.
pub fn hover_info_with_maps(
    source: &str,
    doc: &ParsedDoc,
    analysis: Option<&mir_analyzer::FileAnalysis>,
    position: Position,
    other_docs: &[(Url, Arc<ParsedDoc>)],
    other_maps: &[(Url, Arc<SymbolMap>)],
    session: Option<&mir_analyzer::AnalysisSession>,
) -> Option<Hover> {
    hover_at_core(
        source,
        doc,
        analysis,
        other_docs,
        position,
        session,
        |resolved_word| {
            for (_, sym_map) in other_maps {
                if let Some(entry) = sym_map.lookup(resolved_word, |e| is_hoverable_kind(e.kind))
                    && let Some(sig) = &entry.signature
                {
                    return Some((sig.clone(), entry.doc_markdown.clone()));
                }
            }
            None
        },
    )
}

/// Full hover: the cross-file declaration lookup walks `other_docs` with
/// [`resolve_declaration`]. See [`hover_at_core`] for the shared body.
pub fn hover_at(
    source: &str,
    doc: &ParsedDoc,
    analysis: Option<&mir_analyzer::FileAnalysis>,
    other_docs: &[(Url, Arc<ParsedDoc>)],
    position: Position,
    session: Option<&mir_analyzer::AnalysisSession>,
) -> Option<Hover> {
    hover_at_core(
        source,
        doc,
        analysis,
        other_docs,
        position,
        session,
        |resolved_word| {
            for (_, other) in other_docs {
                if let Some(sig) =
                    resolve_declaration(&other.program().stmts, resolved_word, &is_hoverable)
                        .and_then(|d| declaration_signature(&d, resolved_word))
                {
                    let md = find_docblock(&other.program().stmts, resolved_word)
                        .map(|db| db.to_markdown())
                        .filter(|md| !md.is_empty());
                    return Some((sig, md));
                }
            }
            None
        },
    )
}

fn builtin_class_hover(
    stub: crate::types::type_map::ClassMembers,
    name: &str,
    range: Option<tower_lsp::lsp_types::Range>,
) -> Hover {
    let method_names: Vec<&str> = stub
        .methods
        .iter()
        .filter(|(_, is_static)| !is_static)
        .map(|(n, _)| n.as_str())
        .take(8)
        .collect();
    let static_names: Vec<&str> = stub
        .methods
        .iter()
        .filter(|(_, is_static)| *is_static)
        .map(|(n, _)| n.as_str())
        .take(4)
        .collect();
    let mut lines = vec![format!("**{}** — built-in class", name)];
    if !method_names.is_empty() {
        lines.push(format!(
            "Methods: {}",
            method_names
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !static_names.is_empty() {
        lines.push(format!(
            "Static: {}",
            static_names
                .iter()
                .map(|n| format!("`{n}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(parent) = &stub.parent {
        lines.push(format!("Extends: `{parent}`"));
    }
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: lines.join("\n\n"),
        }),
        range,
    }
}

/// Shared hover implementation. Every path is identical between the two public
/// entry points except the cross-file declaration lookup, which the caller
/// supplies via `resolve_cross_file`: [`hover_at`] walks `other_docs` with
/// `resolve_declaration`; [`hover_info_with_maps`] does an O(1) `SymbolMap`
/// lookup. The closure returns the `(signature, doc_markdown)` to render.
fn hover_at_core(
    source: &str,
    doc: &ParsedDoc,
    analysis: Option<&mir_analyzer::FileAnalysis>,
    other_docs: &[(Url, Arc<ParsedDoc>)],
    position: Position,
    session: Option<&mir_analyzer::AnalysisSession>,
    resolve_cross_file: impl Fn(&str) -> Option<(String, Option<String>)>,
) -> Option<Hover> {
    let hover_range = word_range_at(source, position);

    if let Some(line_text) = source.lines().nth(position.line as usize) {
        let trimmed = line_text.trim();
        if trimmed.starts_with("use ") {
            let (prefix, content) = if trimmed.starts_with("use function ") {
                (
                    "use function ",
                    trimmed.strip_prefix("use function ").unwrap_or(""),
                )
            } else if trimmed.starts_with("use const ") {
                (
                    "use const ",
                    trimmed.strip_prefix("use const ").unwrap_or(""),
                )
            } else {
                ("use ", trimmed.strip_prefix("use ").unwrap_or(""))
            };
            let fqn = content.trim_end_matches(';').trim();
            if !fqn.is_empty() {
                let maybe_word = word_at_position(source, position);
                let alias = fqn_short_name(fqn);
                let matches = match &maybe_word {
                    Some(w) => w == alias || fqn.contains(w.as_str()),
                    None => true,
                };
                if matches {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("`{}{};`", prefix, fqn),
                        }),
                        range: hover_range,
                    });
                }
            }
        }
    }

    let word = word_at_position(source, position)?;

    if let Some(line_text) = source.lines().nth(position.line as usize)
        && extract_static_class_before_cursor(line_text, position.character as usize).is_none()
    {
        let keyword_doc: Option<&str> = match word.as_str() {
            "match" => Some("`match` — evaluates an expression against a set of arms (PHP 8.0)"),
            "null" => Some("`null` — the null value; a variable has no value"),
            "true" => Some("`true` — boolean true"),
            "false" => Some("`false` — boolean false"),
            "abstract" => Some(
                "`abstract` — declares an abstract class or method that must be implemented by a subclass",
            ),
            "readonly" => {
                Some("`readonly` — property or class that can only be initialised once (PHP 8.1)")
            }
            "yield" => Some("`yield` — produces a value from a generator function"),
            "never" => Some(
                "`never` — return type indicating the function always throws or exits (PHP 8.1)",
            ),
            "throw" => {
                Some("`throw` — throws an exception; can be used as an expression (PHP 8.0)")
            }
            _ => None,
        };
        if let Some(doc_str) = keyword_doc {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc_str.to_string(),
                }),
                range: hover_range,
            });
        }
    }

    let type_map_cell: OnceCell<TypeMap> = OnceCell::new();
    let type_map =
        || type_map_cell.get_or_init(|| TypeMap::from_doc_at_position(doc, None, position));

    if let Some(line_text) = source.lines().nth(position.line as usize)
        && !word.starts_with('$')
        && is_named_arg_at(line_text, position.character as usize, &word)
        && let Some(callee) = extract_named_arg_callee(line_text, position.character as usize)
        && let Some(value) = named_arg_hover_value(
            source,
            doc,
            other_docs,
            position,
            &callee,
            &word,
            analysis,
            &type_map_cell,
        )
    {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: hover_range,
        });
    }

    if word.starts_with('$') {
        if let Some(ty) = analysis.and_then(|a| {
            let off =
                word_range_at(source, position).map(|r| doc.view().byte_of_position(r.start))?;
            crate::types::type_query::type_at_offset(a, off)
        }) && !crate::types::type_query::class_names(ty).is_empty()
        {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("`{word}` `{ty}`"),
                }),
                range: hover_range,
            });
        }
        if let Some(class_name) = type_map().get(&word) {
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("`{}` `{}`", word, class_name),
                }),
                range: hover_range,
            });
        }
    }

    if word.starts_with('$')
        && let Some(line_text) = source.lines().nth(position.line as usize)
        && let Some(class_name) =
            extract_static_class_before_cursor(line_text, position.character as usize)
    {
        let prop_name = word.trim_start_matches('$');
        let effective_class = if class_name == "self" || class_name == "static" {
            crate::types::type_map::enclosing_class_at(source, doc, position)
                .unwrap_or(class_name.clone())
        } else {
            class_name.clone()
        };
        for d in std::iter::once(doc).chain(other_docs.iter().map(|(_, d)| d.as_ref())) {
            if let Some((modifiers, type_str, db)) =
                find_property_info(d, &effective_class, prop_name)
            {
                let sig = format!(
                    "(property) {}{}::${}{}",
                    modifiers,
                    effective_class,
                    prop_name,
                    if type_str.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", type_str)
                    }
                );
                let mut value = wrap_php(&sig);
                if let Some(doc) = db {
                    let md = doc.to_markdown();
                    if !md.is_empty() {
                        value.push_str("\n\n---\n\n");
                        value.push_str(&md);
                    }
                }
                return Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value,
                    }),
                    range: hover_range,
                });
            }
        }
    }

    // Property declaration hover: cursor on `$prop` inside a class body.
    // The mir path only fires for property ACCESSES; this covers the declaration
    // line itself where no access symbol exists.
    if word.starts_with('$')
        && let Some(class_name) = crate::types::type_map::enclosing_class_at(source, doc, position)
        && let prop_name = word.trim_start_matches('$')
        && let Some((modifiers, type_str, db)) = find_property_info(doc, &class_name, prop_name)
    {
        let sig = format!(
            "(property) {}{}::${}{}",
            modifiers,
            class_name,
            prop_name,
            if type_str.is_empty() {
                String::new()
            } else {
                format!(": {type_str}")
            }
        );
        let mut value = wrap_php(&sig);
        if let Some(doc_block) = db {
            let md = doc_block.to_markdown();
            if !md.is_empty() {
                value.push_str("\n\n---\n\n");
                value.push_str(&md);
            }
        }
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: hover_range,
        });
    }

    if !word.starts_with('$')
        && let Some(sym) = analysis.and_then(|a| {
            let off =
                word_range_at(source, position).map(|r| doc.view().byte_of_position(r.start))?;
            a.symbol_at(off)
        })
    {
        let mir_hover = mir_member_hover(sym, &word, doc, other_docs);
        if mir_hover.is_some() {
            return mir_hover.map(|value| Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: hover_range,
            });
        }
    }

    if (word == "function" || word == "fn")
        && let Some(sig) = closure_hover(source, doc, position, &word)
    {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: wrap_php(&sig),
            }),
            range: hover_range,
        });
    }

    let all_stmts = &*doc.program().stmts as &[_];
    let resolved_word = resolve_use_alias(all_stmts, &word).unwrap_or_else(|| word.clone());

    // Current-doc: still uses the AST walker (doc is already in memory, fast).
    let current_doc_found =
        resolve_declaration(&doc.program().stmts, &resolved_word, &is_hoverable)
            .and_then(|d| declaration_signature(&d, &resolved_word))
            .map(|sig| {
                let doc_md = find_docblock(&doc.program().stmts, &resolved_word)
                    .map(|db| db.to_markdown())
                    .filter(|md| !md.is_empty());
                (sig, doc_md)
            });

    // Cross-file: delegated to the caller's strategy (AST walk or symbol-map lookup).
    let found = current_doc_found.or_else(|| resolve_cross_file(&resolved_word));

    if let Some((sig, doc_md)) = found {
        let mut value = wrap_php(&sig);
        if let Some(md) = doc_md
            && !md.is_empty()
        {
            value.push_str("\n\n---\n\n");
            value.push_str(&md);
        }
        if is_php_builtin(&resolved_word) {
            value.push_str(&format!(
                "\n\n[php.net documentation]({})",
                php_doc_url(&resolved_word)
            ));
        }
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: hover_range,
        });
    }

    if is_php_builtin(&resolved_word) {
        let value = format!(
            "```php\nfunction {}()\n```\n\n[php.net documentation]({})",
            resolved_word,
            php_doc_url(&resolved_word)
        );
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: hover_range,
        });
    }

    if let Some(stub) =
        session.and_then(|s| crate::types::stub_members::stub_class_members(s, &resolved_word))
    {
        return Some(builtin_class_hover(stub, &resolved_word, hover_range));
    }

    None
}

/// Produce hover markdown for a member access resolved by mir. Returns the
/// hover value string (not the full `Hover` struct — the caller wraps it).
/// `None` means mir has no symbol here; the caller falls through to `resolve_declaration`.
fn mir_member_hover(
    sym: &mir_analyzer::ResolvedSymbol,
    word: &str,
    doc: &ParsedDoc,
    other_docs: &[(tower_lsp::lsp_types::Url, std::sync::Arc<ParsedDoc>)],
) -> Option<String> {
    let docs = || std::iter::once(doc).chain(other_docs.iter().map(|(_, d)| d.as_ref()));
    match &sym.kind {
        mir_analyzer::ReferenceKind::MethodCall { class, .. }
        | mir_analyzer::ReferenceKind::StaticCall { class, .. } => {
            let class_short = fqn_short_name(class);
            for d in docs() {
                if let Some(sig) = scan_method_of_class(&d.program().stmts, class_short, word) {
                    // Augment declared return type with mir's inferred type when richer.
                    let sig = augment_return_type(sig, &sym.resolved_type);
                    let mut value = wrap_php(&sig);
                    let all =
                        std::iter::once(doc).chain(other_docs.iter().map(|(_, d)| d.as_ref()));
                    if let Some(db) = resolve_method_docblock(all, class_short, word) {
                        let md = db.to_markdown();
                        if !md.is_empty() {
                            value.push_str("\n\n---\n\n");
                            value.push_str(&md);
                        }
                    }
                    return Some(value);
                }
            }
            None
        }
        mir_analyzer::ReferenceKind::PropertyAccess { class, property } => {
            let class_short = fqn_short_name(class);
            for d in docs() {
                if let Some((modifiers, declared_type, db)) =
                    find_property_info(d, class_short, property)
                {
                    // Use mir's resolved type when it's more specific than declared.
                    let type_str = augment_property_type(declared_type, &sym.resolved_type);
                    let sig = format!(
                        "(property) {}{}::${}{}",
                        modifiers,
                        class_short,
                        property,
                        if type_str.is_empty() {
                            String::new()
                        } else {
                            format!(": {}", type_str)
                        }
                    );
                    let mut value = wrap_php(&sig);
                    if let Some(doc_block) = db {
                        let md = doc_block.to_markdown();
                        if !md.is_empty() {
                            value.push_str("\n\n---\n\n");
                            value.push_str(&md);
                        }
                    }
                    return Some(value);
                }
            }
            // No declared property — dynamic access via __get. Show mir's resolved
            // type (the __get return type) rather than falling through to no hover.
            let ty_str = format!("{}", sym.resolved_type);
            if !matches!(ty_str.as_str(), "" | "void" | "never") {
                let sig = format!("(property) {}::${}: {}", class_short, property, ty_str);
                return Some(wrap_php(&sig));
            }
            None
        }
        mir_analyzer::ReferenceKind::ConstantAccess { class, constant } => {
            let class_short = fqn_short_name(class);
            for d in docs() {
                if let Some(sig) =
                    scan_enum_case_of_class(&d.program().stmts, class_short, constant)
                {
                    return Some(wrap_php(&sig));
                }
                if let Some(sig) =
                    scan_class_const_of_class(&d.program().stmts, class_short, constant)
                {
                    return Some(wrap_php(&sig));
                }
            }
            None
        }
        _ => None,
    }
}

/// Override the return-type portion of a method signature string with mir's
/// inferred type, when mir provides concrete (non-mixed, non-void) info.
/// `sig` has the form `"Class::method(params): OldType"` or no return type.
fn augment_return_type(sig: String, resolved: &mir_analyzer::Type) -> String {
    let ty_str = format!("{resolved}");
    if matches!(ty_str.as_str(), "mixed" | "void" | "never" | "null") {
        return sig;
    }
    let Some(paren) = sig.rfind(')') else {
        return sig;
    };
    let rest = &sig[paren + 1..];
    if let Some(colon_pos) = rest.find(": ") {
        let declared = rest[colon_pos + 2..].trim();
        // static/self/parent are late-static-binding — mir resolves them to a
        // concrete class, losing the polymorphic semantics. Keep declared.
        if matches!(declared, "static" | "self" | "parent") {
            return sig;
        }
        format!("{}: {}", &sig[..paren + 1 + colon_pos], ty_str)
    } else {
        format!("{}: {}", sig, ty_str)
    }
}

/// Choose between the declared property type and mir's resolved type. Uses
/// mir when it adds information (non-mixed, non-void).
fn augment_property_type(declared: String, resolved: &mir_analyzer::Type) -> String {
    let ty_str = format!("{resolved}");
    if matches!(ty_str.as_str(), "mixed" | "void" | "never") {
        return declared;
    }
    ty_str
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::cursor;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn word_at_extracts_from_middle_of_identifier() {
        let (src, p) = cursor("<?php\nfunction greet$0User() {}");
        let word = word_at_position(&src, p);
        assert_eq!(word.as_deref(), Some("greetUser"));
    }

    #[test]
    fn hover_on_builtin_class_requires_session() {
        // Without a session, built-in class hover returns None (stubs are
        // queried through the mir AnalysisSession). Full coverage lives in
        // the integration tests that use TestServer with a real session.
        let src = "<?php\n$pdo = new PDO('sqlite::memory:');\n$pdo->query('SELECT 1');";
        let doc = ParsedDoc::parse(src.to_string());
        let h = hover_at(src, &doc, None, &[], pos(1, 12), None);
        assert!(h.is_none(), "built-in class hover requires a session");
    }
}
