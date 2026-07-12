use std::collections::HashMap;
use std::sync::Arc;

use tower_lsp::lsp_types::{Position, Range, TextEdit, Url, WorkspaceEdit};

use crate::document::ast::ParsedDoc;
use crate::navigation::references::{SymbolKind, find_references_with_use};
use crate::navigation::walk::{collect_var_refs_in_scope, property_refs_in_stmts};
use crate::text::utf16_code_units;

/// Compute a WorkspaceEdit that renames every occurrence of `word` to `new_name`
/// across all open documents (including the declaration site).
///
/// Equivalent to `rename_with_kind(word, new_name, all_docs, None, target_fqn)` —
/// kept as a stable, kind-agnostic entry point for callers (e.g. benchmarks) that
/// don't classify the symbol. Prefer `rename_with_kind` when the caller knows the
/// symbol is a class: it merges in `use`-import edits that this path can miss.
pub fn rename(
    word: &str,
    new_name: &str,
    all_docs: &[(Url, Arc<ParsedDoc>)],
    target_fqn: Option<&str>,
) -> WorkspaceEdit {
    rename_with_kind(word, new_name, all_docs, None, target_fqn)
}

/// Like `rename`, but takes the caller's classified `SymbolKind` (from
/// `symbol_kind_at`/`resolve_reference_symbol`) so a class rename can merge two
/// span sources that each cover only half the picture:
/// - the class-kind walker (`Some(SymbolKind::Class)`, via `class_refs_in_stmts`)
///   is type-hint aware — it catches type hints, `extends`/`implements`,
///   `instanceof`, `new`, and static-call class tokens — but never looks at `use`
///   statements;
/// - the general word walker used for every other rename catches `use` imports
///   (and declarations/`new` sites, which are `ExprKind::Identifier` nodes) but has
///   no `visit_type_hint` override, so it's blind to type hints.
///
/// Without merging both, renaming a class silently leaves type-hint occurrences
/// (`function greet(User $user)`) referring to a class that no longer exists.
pub fn rename_with_kind(
    word: &str,
    new_name: &str,
    all_docs: &[(Url, Arc<ParsedDoc>)],
    kind: Option<SymbolKind>,
    target_fqn: Option<&str>,
) -> WorkspaceEdit {
    use crate::navigation::references::{
        dedup_ref_locations, find_references_with_target, use_import_locations,
    };

    let locations = match (kind, target_fqn) {
        (Some(SymbolKind::Class), Some(fqn)) => {
            let mut locs =
                find_references_with_target(word, all_docs, true, Some(SymbolKind::Class), fqn);
            locs.extend(use_import_locations(word, all_docs));
            dedup_ref_locations(&mut locs);
            locs
        }
        (_, Some(fqn)) => find_references_with_target(word, all_docs, true, None, fqn),
        (_, None) => find_references_with_use(word, all_docs, true),
    };

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for loc in locations {
        changes.entry(loc.uri).or_default().push(TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        });
    }

    WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }
}

/// Returns the range of the word at `position` if it's a renameable symbol.
/// Used for `textDocument/prepareRename`.
pub fn prepare_rename(source: &str, position: Position) -> Option<Range> {
    use crate::text::word_at_position;
    let word = word_at_position(source, position)?;
    if word.contains('\\') {
        return None;
    }
    // PHP keywords cannot be renamed; return None so editors disable the action.
    if is_php_keyword(&word) {
        return None;
    }
    // PHP superglobals ($_GET, $_POST, etc.) are part of the language runtime;
    // renaming them breaks code, so we disable the action.
    if is_superglobal(&word) {
        return None;
    }
    let line = source.lines().nth(position.line as usize)?;
    let col = position.character as usize;
    let chars: Vec<char> = line.chars().collect();
    // `is_word` intentionally excludes `$` so the range covers only the bare
    // identifier name (not the sigil). `word_at` may return `$var` with the `$`,
    // so we strip it before computing the range length to avoid an off-by-one.
    let is_word = |c: char| c.is_alphanumeric() || c == '_';

    // Find the character index at or before the cursor position (in UTF-16 code units)
    let mut utf16_col = 0usize;
    let mut char_idx = 0usize;
    for (i, ch) in chars.iter().enumerate() {
        // Check if cursor is within this character's UTF-16 span
        let char_width = ch.len_utf16();
        if utf16_col + char_width > col {
            char_idx = i;
            break;
        }
        utf16_col += char_width;
        char_idx = i + 1;
    }

    // Find the start of the word by walking backwards
    let mut left = char_idx;
    while left > 0 && is_word(chars[left - 1]) {
        left -= 1;
    }

    let bare_word = word.trim_start_matches('$');
    let start_utf16: u32 = chars[..left].iter().map(|c| c.len_utf16() as u32).sum();
    let end_utf16: u32 = start_utf16 + utf16_code_units(bare_word);
    Some(Range {
        start: Position {
            line: position.line,
            character: start_utf16,
        },
        end: Position {
            line: position.line,
            character: end_utf16,
        },
    })
}

fn is_php_keyword(word: &str) -> bool {
    matches!(
        word,
        "abstract"
            | "and"
            | "array"
            | "as"
            | "bool"
            | "break"
            | "callable"
            | "case"
            | "catch"
            | "class"
            | "clone"
            | "const"
            | "continue"
            | "declare"
            | "default"
            | "die"
            | "do"
            | "echo"
            | "else"
            | "elseif"
            | "empty"
            | "enddeclare"
            | "endfor"
            | "endforeach"
            | "endif"
            | "endswitch"
            | "endwhile"
            | "enum"
            | "eval"
            | "exit"
            | "extends"
            | "false"
            | "final"
            | "finally"
            | "float"
            | "fn"
            | "for"
            | "foreach"
            | "function"
            | "global"
            | "goto"
            | "if"
            | "implements"
            | "include"
            | "include_once"
            | "instanceof"
            | "insteadof"
            | "int"
            | "interface"
            | "isset"
            | "iterable"
            | "list"
            | "match"
            | "mixed"
            | "namespace"
            | "never"
            | "new"
            | "null"
            | "object"
            | "or"
            | "parent"
            | "print"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "require"
            | "require_once"
            | "return"
            | "self"
            | "static"
            | "string"
            | "switch"
            | "throw"
            | "trait"
            | "true"
            | "try"
            | "use"
            | "var"
            | "void"
            | "while"
            | "xor"
            | "yield"
            | "__CLASS__"
            | "__DIR__"
            | "__FILE__"
            | "__FUNCTION__"
            | "__LINE__"
            | "__METHOD__"
            | "__NAMESPACE__"
            | "__TRAIT__"
    )
}

fn is_superglobal(word: &str) -> bool {
    matches!(
        word,
        "$_GET"
            | "$_POST"
            | "$_REQUEST"
            | "$_FILES"
            | "$_COOKIE"
            | "$_SESSION"
            | "$_SERVER"
            | "$_ENV"
            | "$GLOBALS"
            | "$this"
    )
}

/// Rename a `$variable` (or parameter) within its enclosing function/method scope.
/// Only produces edits within the single document `uri`; variables don't cross files.
pub fn rename_variable(
    var_name: &str,
    new_name: &str,
    uri: &Url,
    doc: &ParsedDoc,
    position: Position,
) -> WorkspaceEdit {
    let bare = var_name.trim_start_matches('$');
    let new_bare = new_name.trim_start_matches('$');
    let new_text = format!("${new_bare}");

    let stmts = &doc.program().stmts;
    let sv = doc.view();
    let byte_off = sv.byte_of_position(position) as usize;

    let mut spans = Vec::new();
    collect_var_refs_in_scope(stmts, bare, byte_off, &mut spans);

    let mut seen = std::collections::HashSet::new();
    let mut edits: Vec<TextEdit> = spans
        .into_iter()
        .filter_map(|(span, _)| {
            let start = sv.position_of(span.start);
            let end = sv.position_of(span.end);
            seen.insert((start.line, start.character))
                .then_some(TextEdit {
                    range: Range { start, end },
                    new_text: new_text.clone(),
                })
        })
        .collect();
    edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));

    let mut changes = HashMap::new();
    if !edits.is_empty() {
        changes.insert(uri.clone(), edits);
    }

    WorkspaceEdit {
        changes: if changes.is_empty() {
            None
        } else {
            Some(changes)
        },
        ..Default::default()
    }
}

/// Rename a property (`->prop` / `?->prop` / class declaration) across all indexed
/// documents.  Unlike variable rename, properties are not scope-bound and may appear
/// in many files.
pub fn rename_property(
    prop_name: &str,
    new_name: &str,
    all_docs: &[(Url, Arc<ParsedDoc>)],
) -> WorkspaceEdit {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for (uri, doc) in all_docs {
        let sv = doc.view();
        let mut spans = Vec::new();
        property_refs_in_stmts(
            sv.source(),
            &doc.program().stmts,
            prop_name,
            None,
            &mut spans,
        );
        if !spans.is_empty() {
            let mut seen = std::collections::HashSet::new();
            let mut edits: Vec<TextEdit> = spans
                .into_iter()
                .filter_map(|span| {
                    let start = sv.position_of(span.start);
                    let end = sv.position_of(span.end);
                    seen.insert((start.line, start.character))
                        .then_some(TextEdit {
                            range: Range { start, end },
                            new_text: new_name.to_string(),
                        })
                })
                .collect();
            edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
            changes.insert(uri.clone(), edits);
        }
    }
    WorkspaceEdit {
        changes: if changes.is_empty() {
            None
        } else {
            Some(changes)
        },
        ..Default::default()
    }
}
