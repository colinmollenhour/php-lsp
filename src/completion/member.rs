use std::sync::Arc;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat, Position};

use crate::ast::ParsedDoc;
use crate::generics::{ImportCtx, render_type};
use crate::inlay_hints::TypeResolver;
use crate::stubs::builtin_class_members;
use crate::type_map::{
    enclosing_class_at, is_backed_enum, is_enum, members_of_class, mixin_classes_of,
    parent_class_name,
};
use crate::util::utf16_offset_to_byte;

use mir_analyzer::db::MirDbStorage;
use mir_types::{Atomic, Type};

use super::callable_item;

/// Merge the instance members of every `|`-separated class name in `class_names`
/// into a single completion list, keeping the first occurrence of each label.
///
/// This is the single implementation of the "union receiver" member merge used by
/// every `->` completion branch (the `Some(">")` arm, the no-trigger arrow path,
/// and the generic path's per-constituent loop), so the dedup behaviour can never
/// drift between them.
pub(super) fn merge_union_members(
    class_names: &str,
    doc: &ParsedDoc,
    other_docs: &[Arc<ParsedDoc>],
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for class_name in class_names.split('|') {
        for item in all_instance_members(class_name.trim(), doc, other_docs) {
            if seen.insert(item.label.clone()) {
                items.push(item);
            }
        }
    }
    items
}

pub(super) fn all_instance_members(
    class_name: &str,
    doc: &ParsedDoc,
    other_docs: &[Arc<ParsedDoc>],
) -> Vec<CompletionItem> {
    let all: Vec<&ParsedDoc> = std::iter::once(doc)
        .chain(other_docs.iter().map(|d| d.as_ref()))
        .collect();
    let mut items = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Queue: class names to process (inheritance chain + mixin chains).
    let mut queue: Vec<String> = vec![class_name.to_string()];
    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        let mut parent: Option<String> = None;
        let mut found_in_docs = false;
        // PHP defines a class in exactly one file, so stop scanning once the
        // defining doc is hit. Without the early break, member completion
        // walks every workspace doc for every class in the inheritance chain.
        for d in &all {
            let members = members_of_class(d, &current);
            if !members.found {
                continue;
            }
            found_in_docs = true;
            parent = members.parent.clone();
            for (name, is_static) in members.methods {
                if !is_static && seen_names.insert(name.clone()) {
                    // Method params unknown here; use has_params=true so
                    // snippet cursor lands inside parens.
                    items.push(callable_item(&name, CompletionItemKind::METHOD, true));
                }
            }
            for (name, is_static) in &members.properties {
                if !is_static {
                    let label = format!("${name}");
                    if seen_names.insert(label.clone()) {
                        let is_readonly = members.readonly_properties.contains(name);
                        items.push(CompletionItem {
                            label,
                            kind: Some(CompletionItemKind::PROPERTY),
                            detail: if is_readonly {
                                Some("readonly".to_string())
                            } else {
                                None
                            },
                            ..Default::default()
                        });
                    }
                }
            }
            // Built-in enum properties: every enum case has `->name: string`
            // and backed enums also have `->value`.
            if is_enum(d, &current) {
                if seen_names.insert("name".to_string()) {
                    items.push(CompletionItem {
                        label: "name".to_string(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some("string".to_string()),
                        ..Default::default()
                    });
                }
                if is_backed_enum(d, &current) && seen_names.insert("value".to_string()) {
                    items.push(CompletionItem {
                        label: "value".to_string(),
                        kind: Some(CompletionItemKind::PROPERTY),
                        detail: Some("string|int".to_string()),
                        ..Default::default()
                    });
                }
            }
            // Collect @mixin classes for this class in this doc.
            for mixin in mixin_classes_of(d, &current) {
                queue.push(mixin);
            }
            // Queue trait names so their members are also included.
            for trait_name in members.trait_uses {
                queue.push(trait_name);
            }
            break;
        }
        // Built-in stubs only apply when the class is not defined in any user
        // document — a user class shadowing a built-in name wins.
        if !found_in_docs && let Some(stub) = builtin_class_members(&current) {
            if parent.is_none() {
                parent = stub.parent.clone();
            }
            for (name, is_static) in &stub.methods {
                if !is_static && seen_names.insert(name.clone()) {
                    items.push(callable_item(name, CompletionItemKind::METHOD, true));
                }
            }
            for (name, is_static) in &stub.properties {
                if !is_static {
                    let label = format!("${name}");
                    if seen_names.insert(label.clone()) {
                        items.push(CompletionItem {
                            label,
                            kind: Some(CompletionItemKind::PROPERTY),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        if let Some(p) = parent {
            queue.push(p);
        }
    }
    items
}

pub(super) fn all_static_members(
    class_name: &str,
    doc: &ParsedDoc,
    other_docs: &[Arc<ParsedDoc>],
) -> Vec<CompletionItem> {
    let all: Vec<&ParsedDoc> = std::iter::once(doc)
        .chain(other_docs.iter().map(|d| d.as_ref()))
        .collect();
    let mut items = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: Vec<String> = vec![class_name.to_string()];
    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        let mut parent: Option<String> = None;
        let mut found_in_docs = false;
        for d in &all {
            let members = members_of_class(d, &current);
            if !members.found {
                continue;
            }
            found_in_docs = true;
            parent = members.parent.clone();
            for (name, is_static) in members.methods {
                if is_static && seen_names.insert(name.clone()) {
                    items.push(callable_item(&name, CompletionItemKind::METHOD, true));
                }
            }
            for (name, is_static) in members.properties {
                if is_static {
                    let label = format!("${name}");
                    if seen_names.insert(label.clone()) {
                        items.push(CompletionItem {
                            label,
                            kind: Some(CompletionItemKind::PROPERTY),
                            ..Default::default()
                        });
                    }
                }
            }
            for name in members.constants {
                if seen_names.insert(name.clone()) {
                    items.push(CompletionItem {
                        label: name,
                        kind: Some(CompletionItemKind::CONSTANT),
                        ..Default::default()
                    });
                }
            }
            // Queue trait names so their static members are also included.
            for trait_name in members.trait_uses {
                queue.push(trait_name);
            }
            break;
        }
        // Built-in stubs only apply when the class is not defined in any user
        // document — a user class shadowing a built-in name wins.
        if !found_in_docs && let Some(stub) = builtin_class_members(&current) {
            if parent.is_none() {
                parent = stub.parent.clone();
            }
            for (name, is_static) in &stub.methods {
                if *is_static && seen_names.insert(name.clone()) {
                    items.push(callable_item(name, CompletionItemKind::METHOD, true));
                }
            }
            for (name, is_static) in &stub.properties {
                if *is_static {
                    let label = format!("${name}");
                    if seen_names.insert(label.clone()) {
                        items.push(CompletionItem {
                            label,
                            kind: Some(CompletionItemKind::PROPERTY),
                            ..Default::default()
                        });
                    }
                }
            }
            for name in &stub.constants {
                if seen_names.insert(name.clone()) {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::CONSTANT),
                        ..Default::default()
                    });
                }
            }
        }
        if let Some(p) = parent {
            queue.push(p);
        }
    }
    items
}

/// Resolve `ClassName::` or the aliases `self::`, `static::`, `parent::`.
pub(super) fn resolve_static_receiver(
    source: &str,
    doc: &ParsedDoc,
    other_docs: &[Arc<ParsedDoc>],
    position: Position,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let col = utf16_offset_to_byte(line, position.character as usize);
    let before = &line[..col];
    let before = before.strip_suffix("::").unwrap_or(before);
    let name: String = before
        .chars()
        .rev()
        .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '\\')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    match name.as_str() {
        "" => None,
        "self" | "static" => enclosing_class_at(source, doc, position),
        "parent" => {
            let enclosing = enclosing_class_at(source, doc, position)?;
            // Look for the parent class in current doc then other docs
            if let Some(p) = parent_class_name(doc, &enclosing) {
                return Some(p);
            }
            for other in other_docs {
                if let Some(p) = parent_class_name(other, &enclosing) {
                    return Some(p);
                }
            }
            None
        }
        _ => Some(name),
    }
}

const PHP_MAGIC_METHODS: &[(&str, &str)] = &[
    (
        "__construct",
        "public function __construct($1)\n{\n    $2\n}",
    ),
    ("__destruct", "public function __destruct()\n{\n    $1\n}"),
    (
        "__get",
        "public function __get(string $name): mixed\n{\n    $1\n}",
    ),
    (
        "__set",
        "public function __set(string $name, mixed $value): void\n{\n    $1\n}",
    ),
    (
        "__isset",
        "public function __isset(string $name): bool\n{\n    $1\n}",
    ),
    (
        "__unset",
        "public function __unset(string $name): void\n{\n    $1\n}",
    ),
    (
        "__call",
        "public function __call(string $name, array $arguments): mixed\n{\n    $1\n}",
    ),
    (
        "__callStatic",
        "public static function __callStatic(string $name, array $arguments): mixed\n{\n    $1\n}",
    ),
    (
        "__toString",
        "public function __toString(): string\n{\n    $1\n}",
    ),
    (
        "__invoke",
        "public function __invoke($1): mixed\n{\n    $2\n}",
    ),
    ("__clone", "public function __clone(): void\n{\n    $1\n}"),
    ("__sleep", "public function __sleep(): array\n{\n    $1\n}"),
    ("__wakeup", "public function __wakeup(): void\n{\n    $1\n}"),
    (
        "__serialize",
        "public function __serialize(): array\n{\n    $1\n}",
    ),
    (
        "__unserialize",
        "public function __unserialize(array $data): void\n{\n    $1\n}",
    ),
    (
        "__debugInfo",
        "public function __debugInfo(): ?array\n{\n    $1\n}",
    ),
];

pub(super) fn magic_method_completions() -> Vec<CompletionItem> {
    PHP_MAGIC_METHODS
        .iter()
        .map(|(name, snippet)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::METHOD),
            insert_text: Some(snippet.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            detail: Some("magic method".to_string()),
            ..Default::default()
        })
        .collect()
}

pub(super) fn resolve_receiver_class(
    source: &str,
    doc: &ParsedDoc,
    position: Position,
    type_map: &crate::type_map::TypeMap,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let col = utf16_offset_to_byte(line, position.character as usize);
    let before = &line[..col];
    // Try ?-> first (longer pattern) so `$s?->` doesn't get stripped to `$s?` by the `->` rule.
    let before = before
        .strip_suffix("?->")
        .or_else(|| before.strip_suffix("->"))
        .unwrap_or(before);

    // Handle (new ClassName()) before ->
    if let Some(class_name) = extract_new_class_before_arrow(before) {
        return Some(class_name);
    }

    let var_name: String = before
        .chars()
        .rev()
        .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '$')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if var_name.is_empty() {
        return None;
    }
    let var_name = if var_name.starts_with('$') {
        var_name
    } else {
        format!("${var_name}")
    };
    if var_name == "$this" {
        // Prefer the enclosing class (standard method context).
        // Fall back to type_map for top-level bound closures where
        // Closure::bind / bindTo / call injected a $this mapping.
        return enclosing_class_at(source, doc, position)
            .or_else(|| type_map.get("$this").map(|s| s.to_string()));
    }
    type_map.get(&var_name).map(|s| s.to_string())
}

/// Extract the class name from `(new ClassName(...))` or `new ClassName(...)` text
/// appearing immediately before `->`.
fn extract_new_class_before_arrow(text: &str) -> Option<String> {
    let text = text.trim_end();
    // Strip optional closing paren wrapping: `(new Foo())`
    let inner = if let Some(without_last) = text.strip_suffix(')') {
        // Find matching open paren — look for `(new` pattern
        if let Some(pos) = without_last.rfind("(new ") {
            &without_last[pos + 1..]
        } else if let Some(pos) = without_last.rfind("(new\t") {
            &without_last[pos + 1..]
        } else {
            text
        }
    } else {
        text
    };
    // Now inner should start with `new ClassName(...)`
    let inner = inner.trim();
    if !inner.starts_with("new ") && !inner.starts_with("new\t") {
        return None;
    }
    let after_new = inner[3..].trim_start();
    // Extract class name (alphanumeric + _ + \)
    let class: String = after_new
        .chars()
        .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '\\')
        .collect();
    if class.is_empty() {
        return None;
    }
    // Return short name
    Some(class.rsplit('\\').next().unwrap_or(&class).to_string())
}

// ---------------------------------------------------------------------------
// WP3 — generic-aware receiver completion (PHP generics)
// ---------------------------------------------------------------------------
//
// All behaviour here is gated behind the resolved-type being available
// (`resolved_type_at`). When that is absent / `None` (cold cache, partial
// expression, inference cycle, `mixed`), every entry point returns `None` and
// the caller falls back to the legacy `resolve_receiver_class` short-name path
// — keeping non-generic completion byte-identical to today (regression safety).

/// Context needed to compute generic-aware `->` member completions.
///
/// Carries the read-only resolved-type resolver (backed by the WP2 resolved-
/// symbol cache; never analyses) and the mir database snapshot used for the C3
/// substitution. Both are optional; when either is absent the generic path is
/// skipped entirely and the legacy path runs unchanged.
#[derive(Clone, Copy, Default)]
pub(super) struct GenericReceiverCtx<'a> {
    /// `Fn(byte_off) -> Option<Type>`. The byte offset is **document-space**
    /// (matches mir's recorded spans). `None` ⇒ no resolved symbol.
    pub resolver: Option<&'a TypeResolver<'a>>,
    /// mir database snapshot used to look up template declarations, inherited
    /// `@extends` bindings and member return types for substitution.
    pub codebase: Option<&'a MirDbStorage>,
}

/// Compute the **document-space** byte offset of the last resolvable token of a
/// receiver expression appearing in `before` (the text on `position.line` up to
/// — but not including — the `->`/`?->`).
///
/// For `$c->` the token is the variable `$c`; for `$c->first()->` it is the
/// method-name `first` (mir records the `MethodCall` symbol at the method-name
/// span, see `call/method.rs`). For wrappers like `(new Foo())` there is no
/// trailing identifier so this returns `None` and the caller falls back.
/// Find the byte index of the `(` that opens the call whose `)` is the last char
/// of `trimmed` (which must already be `trim_end`-ed). Matched by paren depth so
/// nested argument parens (`foo(bar())`) are skipped. Returns `None` when
/// `trimmed` does not end with `)` or the parens are unbalanced.
///
/// Single source of truth for the reverse paren scan shared by
/// [`receiver_resolve_offset`] and [`receiver_is_call_chain`]; both share the same
/// string/comment-unaware blind spots, so keeping one scanner means one place to
/// fix.
fn matching_open_paren(trimmed: &str) -> Option<usize> {
    if !trimmed.ends_with(')') {
        return None;
    }
    let bytes = trimmed.as_bytes();
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().rev() {
        match b {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn receiver_resolve_offset(doc: &ParsedDoc, position: Position, before: &str) -> Option<u32> {
    // Strip a trailing call `(...)` so a chained `$c->first()` resolves at the
    // method name. Matched by paren depth to skip nested argument parens.
    let trimmed = before.trim_end();
    let head = if trimmed.ends_with(')') {
        // Text before the matching `(` (e.g. `$c->first`).
        &trimmed[..matching_open_paren(trimmed)?]
    } else {
        trimmed
    };
    let head = head.trim_end();
    // The last word character of `head` is inside the receiver token (the
    // method name or the variable name). Resolving there hits the innermost
    // recorded symbol (the method call / the variable).
    let last_word_byte = head
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric() || *c == '_')
        .map(|(i, _)| i)?;
    // Convert the line-relative byte index to a document-space byte offset via
    // the line-start table (matches `SourceView::byte_of_position`).
    let line_start = *doc.view().line_starts().get(position.line as usize)? as usize;
    Some((line_start + last_word_byte) as u32)
}

/// Map a resolved receiver [`Type`] (per M7) to the set of base class FQCNs plus
/// their generic `type_params`, ready for member lookup + substitution.
///
/// Unions (`Foo|Bar`) and intersections (`A&B`) contribute **every** object-like
/// constituent so the union/intersection member-merging behaviour is preserved
/// exactly as today; nullable (`T|null`) drops the `null` constituent. Returns an
/// empty vec for types that cannot drive a class-member lookup (`mixed`,
/// scalars, `class-string<T>`, unresolved templates, …) so the caller falls back
/// to the legacy path. Never panics.
fn receiver_bases(ty: &Type, out: &mut Vec<(String, Vec<Type>)>) {
    for atomic in ty.types.iter() {
        match atomic {
            Atomic::TNamedObject { fqcn, type_params } => {
                out.push((fqcn.as_str().to_string(), type_params.to_vec()));
            }
            // `self`/`static`/`parent` carry the enclosing-class fqcn directly.
            Atomic::TSelf { fqcn } | Atomic::TStaticObject { fqcn } | Atomic::TParent { fqcn } => {
                out.push((fqcn.as_str().to_string(), Vec::new()));
            }
            // A template param resolves through its upper bound (`as_type`),
            // e.g. `T of Collection` → look up `Collection`'s members.
            Atomic::TTemplateParam { as_type, .. } => {
                receiver_bases(as_type, out);
            }
            // Intersection: contribute every part (`A&B` → both A and B).
            Atomic::TIntersection { parts } => {
                for part in parts.iter() {
                    receiver_bases(part, out);
                }
            }
            // Everything else (scalars, arrays, class-string, callables, …) has
            // no member list — skip this constituent / fall back.
            _ => {}
        }
    }
}

/// Build the template-binding map for `fqcn` given the receiver's resolved
/// `type_params`, then apply generic substitution to a class's member completion
/// items: for each method, look up its declared/inferred return type, substitute
/// the bindings, and set the item `detail` to the rendered concrete type (e.g.
/// `current()` gains detail `User`). Items whose return type does not change are
/// left as-is.
///
/// The binding map combines a local reimplementation of mir's (private)
/// `build_class_bindings` zip — `class_template_params` (the class's template
/// **declarations**) zipped with the receiver's resolved generic args — with the
/// inherited `@extends<...>` bindings underneath (the direct zip wins on
/// conflict). The map's type is kept local so the `rustc_hash` map type returned
/// by `inherited_template_bindings` is never named in a signature.
///
/// Returns `true` when generic bindings were present for `fqcn` (i.e. real
/// generics are in play for this receiver), so the caller can gate engaging the
/// generic path versus deferring to the byte-identical legacy path.
fn substitute_member_returns(
    items: &mut [CompletionItem],
    codebase: &MirDbStorage,
    fqcn: &str,
    type_params: &[Type],
) -> bool {
    // Start from the inherited `@extends<...>` chain so a subclass without its
    // own template params (e.g. `UserRepo extends BaseRepo<User>`) still gets
    // `{ T -> User }`.
    let mut bindings = mir_analyzer::db::inherited_template_bindings(codebase, fqcn);
    if let Some(decls) = mir_analyzer::db::class_template_params(codebase, fqcn) {
        // Local 6-line zip (mir's `build_class_bindings` is `pub(crate)`).
        for (tp, ty) in decls.iter().zip(type_params.iter()) {
            bindings.insert(tp.name, ty.clone());
        }
    }
    if bindings.is_empty() {
        return false;
    }
    let ctx = ImportCtx::short();
    let here = mir_analyzer::db::Fqcn::from_str(codebase, fqcn);
    for item in items.iter_mut() {
        if item.kind != Some(CompletionItemKind::METHOD) {
            continue;
        }
        // The declared `@return` / native return type (mir's `MethodDef`) is the
        // authoritative source for substitution — it carries `T` directly,
        // whereas demand-inference may yield `mixed` for empty bodies / across
        // files. `find_method_in_chain` walks ancestors so inherited generic
        // methods (e.g. `UserRepo` inheriting `BaseRepo::find(): T`) resolve too.
        let Some((_, method)) = mir_analyzer::db::find_method_in_chain(codebase, here, &item.label)
        else {
            continue;
        };
        let Some(ret) = method.return_type.as_ref() else {
            continue;
        };
        // VF5: a method may declare its OWN `@template T` that shadows a
        // class/inherited binding of the same name. `substitute_templates` keys
        // purely on the template name, so without this guard the method-local
        // `T` would be wrongly replaced by the receiver's class binding. Drop any
        // binding whose name collides with one of this method's own template
        // declarations before substituting.
        let method_bindings = if method.template_params.is_empty() {
            None
        } else if method
            .template_params
            .iter()
            .any(|tp| bindings.contains_key(&tp.name))
        {
            let mut filtered = bindings.clone();
            for tp in &method.template_params {
                filtered.remove(&tp.name);
            }
            Some(filtered)
        } else {
            None
        };
        let active_bindings = method_bindings.as_ref().unwrap_or(&bindings);
        if active_bindings.is_empty() {
            continue;
        }
        let substituted = ret.substitute_templates(active_bindings);
        // VF7 (perf): when substitution changed nothing (the return type carries
        // no bound template), the `Type` is unchanged — skip both renders. This
        // also short-circuits the common case of a non-generic method on a
        // generic class.
        if substituted == **ret {
            continue;
        }
        // Only surface a detail when substitution actually resolved a template
        // (the rendered type differs from the unsubstituted one); otherwise the
        // detail would just repeat a non-generic return type, changing output
        // for the non-generic path.
        let rendered = render_type(&substituted, &ctx);
        let original = render_type(ret, &ctx);
        if rendered != original {
            item.detail = Some(rendered);
        }
    }
    true
}

/// Whether the receiver expression in `before` is a method/static *call chain*
/// (e.g. `$c->first()`, `Repo::all()`) rather than a bare variable / `new`
/// expression. Used to gate engaging the resolved-type path: chains are
/// receivers the legacy `resolve_receiver_class` cannot handle, so resolving
/// them adds new capability without changing any non-generic snapshot (which all
/// pin the legacy full-symbol fallback for unresolvable receivers).
fn receiver_is_call_chain(before: &str) -> bool {
    let trimmed = before.trim_end();
    // Strip the trailing call `(...)` (paren-depth matched) and look for an
    // arrow / `::` in the callee, which marks a method / static call.
    let Some(open) = matching_open_paren(trimmed) else {
        return false;
    };
    let head = &trimmed[..open];
    head.contains("->") || head.contains("?->") || head.contains("::")
}

/// Generic-aware `->` member completion for the receiver expression ending just
/// before `before`'s trailing `->`/`?->`.
///
/// Engages **only** when the resolved receiver genuinely needs the generic path:
/// either real generic bindings apply (so member return types are substituted —
/// e.g. `$c->current()` → `User`), or the receiver is a call chain the legacy
/// `resolve_receiver_class` cannot handle (e.g. `$c->first()->`). For a plain,
/// non-generic, non-chain receiver this returns `None` and the caller uses the
/// legacy path — keeping non-generic completion byte-identical to today.
///
/// Known limitation (VF15, PRE-EXISTING): the resolved FQCN is shortened to its
/// last `\`-segment before `all_instance_members`, and `members_of_class` matches
/// by short class name. Two same-short-name classes in different namespaces will
/// therefore collide — the SAME short-name limitation the legacy
/// `resolve_receiver_class` path already has, not introduced by the generic path.
/// Fixing it (FQCN-aware member lookup) is tracked separately.
pub(super) fn resolve_generic_member_completion(
    doc: &ParsedDoc,
    other_docs: &[Arc<ParsedDoc>],
    position: Position,
    before: &str,
    gctx: &GenericReceiverCtx<'_>,
) -> Option<Vec<CompletionItem>> {
    let resolver = gctx.resolver?;
    let codebase = gctx.codebase?;
    let off = receiver_resolve_offset(doc, position, before)?;
    let ty = resolver(off)?;
    let mut bases = Vec::new();
    receiver_bases(&ty, &mut bases);
    if bases.is_empty() {
        return None;
    }
    let is_chain = receiver_is_call_chain(before);
    // Merge members from every object constituent (union/intersection). The first
    // occurrence of each label wins for the item itself, but when later
    // constituents substitute a *different* return detail for the same label
    // (e.g. `Collection<User>|Collection<Order>` both expose `current(): T`
    // → `User` and `Order`), the details are merged into a union (`User|Order`)
    // rather than silently dropping all but the first (VF6).
    let mut items: Vec<CompletionItem> = Vec::new();
    let mut index_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut had_generics = false;
    for (fqcn, type_params) in &bases {
        let base_short = fqcn.rsplit('\\').next().unwrap_or(fqcn);
        let mut members = all_instance_members(base_short, doc, other_docs);
        had_generics |= substitute_member_returns(&mut members, codebase, fqcn, type_params);
        for item in members {
            match index_of.get(&item.label) {
                None => {
                    index_of.insert(item.label.clone(), items.len());
                    items.push(item);
                }
                Some(&idx) => {
                    // Same member from another union constituent: merge differing
                    // substituted details into a `|`-union, deduplicated.
                    if let Some(new_detail) = &item.detail {
                        let existing = &mut items[idx].detail;
                        match existing {
                            Some(cur) if !cur.split('|').any(|p| p == new_detail) => {
                                cur.push('|');
                                cur.push_str(new_detail);
                            }
                            None => *existing = Some(new_detail.clone()),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    // Gate: defer to the legacy path unless real generics applied or the
    // receiver is a call chain (which the legacy path cannot resolve at all).
    if !had_generics && !is_chain {
        return None;
    }
    if items.is_empty() {
        return None;
    }
    Some(items)
}
