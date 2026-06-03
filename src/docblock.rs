/// Docblock (`/** ... */`) parser.
///
/// Delegates to [`mir_analyzer::DocblockParser`] for type parsing and
/// [`php_rs_parser::phpdoc`] for description extraction.
use std::collections::HashMap;

use mir_analyzer::DocblockParser;
use mir_analyzer::parser::docblock::parse_type_string;
use mir_types::{Type, Variance};
use php_rs_parser::phpdoc;

use crate::generics::{ImportCtx, render_type};

/// Flatten a `phpdoc::PhpDocText` (sequence of text segments + inline tags)
/// into a single string. Inline tags are rendered as `{@name body}`.
fn flatten_phpdoc_text(t: &phpdoc::PhpDocText) -> String {
    let mut s = String::new();
    for seg in &t.segments {
        match seg {
            phpdoc::TextSegment::Text(txt) => s.push_str(txt),
            phpdoc::TextSegment::InlineTag(it) => {
                s.push('{');
                s.push('@');
                s.push_str(&it.name);
                if let Some(b) = &it.body {
                    s.push(' ');
                    s.push_str(b);
                }
                s.push('}');
            }
        }
    }
    s
}

/// Parse a `@param Type $name description` body: skip the type hint, find the
/// `$name` token, take everything after as the description.
fn parse_param_body(s: &str) -> Option<(String, String)> {
    let mut iter = s.split_whitespace();
    let mut name = None;
    for tok in iter.by_ref() {
        if let Some(n) = tok.strip_prefix('$') {
            name = Some(n.to_string());
            break;
        }
    }
    let desc: Vec<&str> = iter.collect();
    name.map(|n| (n, desc.join(" ").trim().to_string()))
}

/// For `@return` / `@throws` bodies: first whitespace-separated token is the
/// type hint, everything after is the description.
fn body_after_type_hint(s: &str) -> Option<String> {
    let trimmed = s.trim_start();
    let mut split = trimmed.splitn(2, char::is_whitespace);
    let _type = split.next()?;
    Some(split.next().unwrap_or("").trim().to_string())
}

/// For `@var Type [$name] description`: skip the type hint and an optional
/// `$name`, then take the rest as description.
fn body_after_type_and_var(s: &str) -> Option<String> {
    let mut iter = s.split_whitespace();
    let _type = iter.next()?;
    let next = iter.next();
    let rest: Vec<&str> = if let Some(tok) = next {
        if tok.starts_with('$') {
            iter.collect()
        } else {
            std::iter::once(tok).chain(iter).collect()
        }
    } else {
        Vec::new()
    };
    Some(rest.join(" ").trim().to_string())
}

#[derive(Debug, Default, PartialEq)]
pub struct Docblock {
    /// Free-text description (lines before the first `@` tag).
    pub description: String,
    /// `@param  TypeHint  $name  description`
    pub params: Vec<DocParam>,
    /// `@return  TypeHint  description`
    pub return_type: Option<DocReturn>,
    /// `@var  TypeHint` or `@var  TypeHint  $varName`
    pub var_type: Option<String>,
    /// Variable name from `@var TypeHint $varName`, if present.
    pub var_name: Option<String>,
    /// Free-text description after the type in `@var TypeHint description`.
    pub var_description: Option<String>,
    /// `@deprecated  message`  — `Some("")` when present without a message.
    pub deprecated: Option<String>,
    /// `@throws  ClassName  description`
    pub throws: Vec<DocThrows>,
    /// `@see target` and `@link url`
    pub see: Vec<String>,
    /// `@template T` or `@template T of BaseClass`
    pub templates: Vec<DocTemplate>,
    /// `@extends Base<T>` — generic parent (structured mir type), if present.
    /// Carried for later hover declaration rendering.
    pub extends: Option<Type>,
    /// `@implements Iface<T>` — generic interfaces (structured mir types).
    /// Carried for later hover declaration rendering.
    pub implements: Vec<Type>,
    /// `@mixin ClassName`
    pub mixins: Vec<String>,
    /// `true` when the doc is `{@inheritDoc}` / `@inheritDoc` with no other content.
    pub is_inherit_doc: bool,
    /// `@psalm-type Alias = TypeExpr` / `@phpstan-type Alias = TypeExpr`
    pub type_aliases: Vec<DocTypeAlias>,
    /// `@property Type $name` / `@property-read Type $name` / `@property-write Type $name`
    pub properties: Vec<DocProperty>,
    /// `@method [static] ReturnType name([params])`
    pub methods: Vec<DocMethod>,
}

#[derive(Debug, PartialEq)]
pub struct DocProperty {
    pub type_hint: String,
    pub name: String,    // without $
    pub read_only: bool, // true for @property-read
}

#[derive(Debug, PartialEq)]
pub struct DocMethod {
    pub return_type: String,
    pub name: String,
    pub is_static: bool,
}

#[derive(Debug, PartialEq)]
pub struct DocTypeAlias {
    /// Alias name, e.g. `UserId`.
    pub name: String,
    /// Right-hand side type expression, e.g. `string|int`.
    pub type_expr: String,
}

#[derive(Debug, PartialEq)]
pub struct DocTemplate {
    /// Template parameter name, e.g. `T`.
    pub name: String,
    /// Optional upper bound rendered as a short display string, e.g. `Base`
    /// from `@template T of Base`. Kept for back-compat display.
    pub bound: Option<String>,
    /// Structured upper-bound type from mir, if present (additive).
    pub bound_ty: Option<Type>,
    /// Declaration-site variance: `Invariant` for `@template`, `Covariant` for
    /// `@template-covariant`, `Contravariant` for `@template-contravariant`.
    pub variance: Variance,
}

#[derive(Debug, PartialEq)]
pub struct DocParam {
    pub type_hint: String,
    pub name: String,
    pub description: String,
    /// Structured mir type for the param, carried additively so generic args
    /// (`Collection<User>`, `array<K, V>`) survive instead of being flattened.
    /// `None` when the docblock had no `@param` type (e.g. constructed by hand).
    pub ty: Option<Type>,
}

#[derive(Debug, PartialEq)]
pub struct DocReturn {
    pub type_hint: String,
    pub description: String,
    /// Structured mir type for the return, carried additively (see [`DocParam::ty`]).
    pub ty: Option<Type>,
}

#[derive(Debug, PartialEq)]
pub struct DocThrows {
    pub class: String,
    pub description: String,
}

impl Docblock {
    /// Returns `true` if the `@deprecated` tag is present.
    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }

    /// Format as a Markdown string suitable for LSP hover content.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();

        if let Some(msg) = &self.deprecated {
            if msg.is_empty() {
                out.push_str("> **Deprecated**\n\n");
            } else {
                out.push_str(&format!("> **Deprecated**: {}\n\n", msg));
            }
        }

        if !self.description.is_empty() {
            out.push_str(&self.description);
            out.push_str("\n\n");
        }
        if let Some(vt) = &self.var_type {
            out.push_str(&format!("**@var** `{}`", vt));
            if let Some(vd) = &self.var_description
                && !vd.is_empty()
            {
                out.push_str(&format!(" — {}", vd));
            }
            out.push('\n');
        }
        if let Some(ret) = &self.return_type {
            out.push_str(&format!("**@return** `{}`", ret.type_hint));
            if !ret.description.is_empty() {
                out.push_str(&format!(" — {}", ret.description));
            }
            out.push('\n');
        }
        for p in &self.params {
            out.push_str(&format!(
                "**@param** `{}` `{}`",
                p.type_hint,
                &p.name.to_string()
            ));
            if !p.description.is_empty() {
                out.push_str(&format!(" — {}", p.description));
            }
            out.push('\n');
        }
        for t in &self.throws {
            out.push_str(&format!("**@throws** `{}`", t.class));
            if !t.description.is_empty() {
                out.push_str(&format!(" — {}", t.description));
            }
            out.push('\n');
        }
        for s in &self.see {
            out.push_str(&format!("**@see** {}\n", s));
        }
        for t in &self.templates {
            if let Some(bound) = &t.bound {
                out.push_str(&format!("**@template** `{}` of `{}`\n", t.name, bound));
            } else {
                out.push_str(&format!("**@template** `{}`\n", &t.name.to_string()));
            }
        }
        for m in &self.mixins {
            out.push_str(&format!("**@mixin** `{}`\n", m));
        }
        for ta in &self.type_aliases {
            if ta.type_expr.is_empty() {
                out.push_str(&format!("**@type** `{}`\n", &ta.name.to_string()));
            } else {
                out.push_str(&format!("**@type** `{}` = `{}`\n", ta.name, ta.type_expr));
            }
        }
        out.trim_end().to_string()
    }
}

/// Parse a raw docblock string (the full `/** ... */` text, or just the
/// inner content — either form is handled).
///
/// Delegates to [`mir_analyzer::DocblockParser`] for type resolution and
/// [`php_rs_parser::phpdoc`] for description fields.
pub fn parse_docblock(raw: &str) -> Docblock {
    let is_inherit_doc = {
        let stripped = raw
            .trim_start_matches("/**")
            .trim_end_matches("*/")
            .replace('*', "")
            .replace(['{', '}'], "")
            .trim()
            .to_lowercase();
        stripped == "@inheritdoc"
    };

    let mir = DocblockParser::parse(raw);
    let raw_doc = phpdoc::parse(raw);

    // Collect descriptions from the raw tags (mir discards them).
    let mut param_descs: HashMap<String, String> = HashMap::new();
    let mut return_desc = String::new();
    let mut throws_descs: Vec<String> = Vec::new();
    let mut var_desc: Option<String> = None;

    for tag in &raw_doc.tags {
        let body = tag
            .body
            .as_ref()
            .map(flatten_phpdoc_text)
            .unwrap_or_default();
        match tag.name.as_str() {
            "param" => {
                // Body shape: "TypeHint $name description". Find the `$name`
                // token, then take everything after as the description.
                if let Some((name, desc)) = parse_param_body(&body)
                    && !desc.is_empty()
                {
                    param_descs.insert(name, desc);
                }
            }
            "return" => {
                // Body shape: "TypeHint description"
                if let Some(d) = body_after_type_hint(&body)
                    && !d.is_empty()
                {
                    return_desc = d;
                }
            }
            "throws" => {
                // Body shape: "ClassName description"
                let mut parts = body.split_whitespace();
                if let Some(class) = parts.next()
                    && !class.is_empty()
                {
                    let desc = parts.collect::<Vec<_>>().join(" ");
                    throws_descs.push(desc);
                }
            }
            "var" => {
                // Body shape: "TypeHint [$name] description"
                if let Some(d) = body_after_type_and_var(&body)
                    && !d.is_empty()
                {
                    var_desc = Some(d);
                }
            }
            _ => {}
        }
    }

    // Carry the structured mir `Type` additively (so generic args survive),
    // while keeping the back-compat `type_hint` String. The String must stay
    // byte-identical to today's output, which used mir's `Display`.
    let params: Vec<DocParam> = mir
        .params
        .iter()
        .map(|(name, union)| {
            let description = param_descs.get(name.as_str()).cloned().unwrap_or_default();
            DocParam {
                type_hint: union.to_string(),
                name: format!("${}", name),
                description,
                ty: Some(union.clone()),
            }
        })
        .collect();

    let return_type = mir.return_type.as_ref().map(|union| DocReturn {
        type_hint: union.to_string(),
        description: return_desc,
        ty: Some(union.clone()),
    });

    let throws: Vec<DocThrows> = mir
        .throws
        .iter()
        .enumerate()
        .map(|(i, class)| DocThrows {
            class: class.clone(),
            description: throws_descs.get(i).cloned().unwrap_or_default(),
        })
        .collect();

    let deprecated = if mir.is_deprecated {
        Some(mir.deprecated.as_deref().unwrap_or("").to_string())
    } else {
        None
    };

    // Use mir's structured `templates` (name, bound `Type`, `Variance`).
    // mir 0.31 (E1) parses single-line docblocks cleanly: `@template T @param T
    // $x @return T` now yields a template `name` of exactly `T` (the bound, if
    // any, lands in its own field), so no name sanitisation is needed — the
    // mir-provided name is used directly. The multi-line over-read bug is also
    // fixed (see the `template_bound_no_over_read` guard test). mir still does
    // not capture the `@psalm-template` / `@phpstan-template` aliases, so those
    // are parsed from the raw doc and appended below.
    let render_ctx = ImportCtx::short();
    let mut templates: Vec<DocTemplate> = mir
        .templates
        .iter()
        .map(|(name, bound, variance)| DocTemplate {
            name: name.clone(),
            bound: bound.as_ref().map(|b| render_type(b, &render_ctx)),
            bound_ty: bound.clone(),
            variance: *variance,
        })
        .collect();

    // Supplement with psalm/phpstan template aliases that mir does not
    // recognise. Preserve the prior LSP coverage for these tags.
    for t in &raw_doc.tags {
        if t.name != "psalm-template" && t.name != "phpstan-template" {
            continue;
        }
        // php-rs-parser sometimes leaks subsequent `@tag` lines into the body of
        // an earlier tag; truncate at the first `@`-prefixed token.
        let body_full = t.body.as_ref().map(flatten_phpdoc_text).unwrap_or_default();
        let body: String = body_full
            .split_whitespace()
            .take_while(|tok| !tok.starts_with('@'))
            .collect::<Vec<_>>()
            .join(" ");
        let mut iter = body.split_whitespace();
        let Some(name) = iter.next().map(|s| s.to_string()) else {
            continue;
        };
        let bound_raw = match iter.next() {
            Some("of" | "as") => iter.next().map(|s| s.to_string()),
            Some(other) => Some(other.to_string()),
            None => None,
        };
        // Parse the bound into a structured `Type` the same way mir does for
        // `@template`, so FQCN bounds (`@phpstan-template T of \App\Base`) get
        // shortened and rendered consistently with mir-parsed `@template` bounds
        // and `bound_ty` is populated for structured consumers. Fall back to the
        // raw token when rendering yields nothing.
        let bound_ty = bound_raw.as_deref().map(parse_type_string);
        let bound = match (bound_ty.as_ref(), bound_raw) {
            (Some(ty), raw) => {
                let rendered = render_type(ty, &render_ctx);
                if rendered.is_empty() {
                    raw
                } else {
                    Some(rendered)
                }
            }
            (None, raw) => raw,
        };
        templates.push(DocTemplate {
            name,
            bound,
            bound_ty,
            variance: Variance::Invariant,
        });
    }

    let properties: Vec<DocProperty> = mir
        .properties
        .iter()
        .map(|p| DocProperty {
            type_hint: p.type_hint.clone(),
            name: p.name.clone(),
            read_only: p.read_only,
        })
        .collect();

    let methods: Vec<DocMethod> = mir
        .methods
        .iter()
        .map(|m| DocMethod {
            return_type: m.return_type.clone(),
            name: m.name.clone(),
            is_static: m.is_static,
        })
        .collect();

    let type_aliases: Vec<DocTypeAlias> = mir
        .type_aliases
        .iter()
        .map(|ta| DocTypeAlias {
            name: ta.name.clone(),
            type_expr: ta.type_expr.clone(),
        })
        .collect();

    // Pull the var type from the raw `@var` body directly: mir 0.22's
    // `var_type` may swallow the trailing description as part of the type
    // string. The body's first non-`$` whitespace token is the type hint.
    let (var_type_from_body, var_name_from_body) = raw_doc
        .tags
        .iter()
        .find(|t| t.name == "var" || t.name == "psalm-var" || t.name == "phpstan-var")
        .and_then(|t| t.body.as_ref())
        .map(flatten_phpdoc_text)
        .map(|body| {
            let mut ty = None;
            let mut name = None;
            for tok in body.split_whitespace() {
                if let Some(n) = tok.strip_prefix('$') {
                    if name.is_none() {
                        name = Some(n.to_string());
                    }
                } else if ty.is_none() {
                    ty = Some(tok.to_string());
                }
                if ty.is_some() && name.is_some() {
                    break;
                }
            }
            (ty, name)
        })
        .unwrap_or((None, None));

    Docblock {
        description: mir.description.clone(),
        params,
        return_type,
        var_type: var_type_from_body.or_else(|| mir.var_type.as_ref().map(|u| u.to_string())),
        var_name: var_name_from_body.or_else(|| mir.var_name.clone()),
        var_description: var_desc,
        deprecated,
        throws,
        see: mir.see.clone(),
        templates,
        extends: mir.extends.clone(),
        implements: mir.implements.clone(),
        mixins: mir.mixins.clone(),
        type_aliases,
        properties,
        methods,
        is_inherit_doc,
    }
}

/// Scan `source` for a `/** ... */` docblock that ends immediately before
/// `node_start` (byte offset). Whitespace between the `*/` and the node is
/// allowed; non-whitespace text in between disqualifies the block.
pub fn docblock_before(source: &str, node_start: u32) -> Option<String> {
    // Find a `/** ... */` block ending immediately before `node_start`,
    // allowing only whitespace between the closing `*/` and `node_start`.
    let prefix = source.get(..node_start as usize)?;
    let trimmed_end = prefix.trim_end();
    let close = trimmed_end.strip_suffix("*/")?;
    let open_idx = close.rfind("/**")?;
    Some(format!(
        "{}*/",
        &trimmed_end[open_idx..trimmed_end.len() - 2]
    ))
}

/// Walk an AST and return the parsed docblock for the declaration named `word`.
pub fn find_docblock(
    source: &str,
    stmts: &[php_ast::Stmt<'_, '_>],
    word: &str,
) -> Option<Docblock> {
    use php_ast::{ClassMemberKind, EnumMemberKind, NamespaceBody, StmtKind};
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f) if f.name == word => {
                let raw = docblock_before(source, stmt.span.start)?;
                return Some(parse_docblock(&raw));
            }
            StmtKind::Class(c)
                if c.name.as_ref().map(|n| n.to_string()) == Some(word.to_string()) =>
            {
                let raw = docblock_before(source, stmt.span.start)?;
                return Some(parse_docblock(&raw));
            }
            StmtKind::Interface(i) if i.name == word => {
                let raw = docblock_before(source, stmt.span.start)?;
                return Some(parse_docblock(&raw));
            }
            StmtKind::Trait(t) if t.name == word => {
                let raw = docblock_before(source, stmt.span.start)?;
                return Some(parse_docblock(&raw));
            }
            StmtKind::Enum(e) if e.name == word => {
                let raw = docblock_before(source, stmt.span.start)?;
                return Some(parse_docblock(&raw));
            }
            StmtKind::Class(c) => {
                for member in c.body.members.iter() {
                    match &member.kind {
                        ClassMemberKind::Method(m) if m.name == word => {
                            let raw = docblock_before(source, member.span.start)?;
                            return Some(parse_docblock(&raw));
                        }
                        ClassMemberKind::ClassConst(k) if k.name == word => {
                            let raw = docblock_before(source, member.span.start)?;
                            return Some(parse_docblock(&raw));
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::Interface(i) => {
                for member in i.body.members.iter() {
                    match &member.kind {
                        ClassMemberKind::Method(m) if m.name == word => {
                            let raw = docblock_before(source, member.span.start)?;
                            return Some(parse_docblock(&raw));
                        }
                        ClassMemberKind::ClassConst(k) if k.name == word => {
                            let raw = docblock_before(source, member.span.start)?;
                            return Some(parse_docblock(&raw));
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::Trait(t) => {
                for member in t.body.members.iter() {
                    if let ClassMemberKind::Method(m) = &member.kind
                        && m.name == word
                    {
                        let raw = docblock_before(source, member.span.start)?;
                        return Some(parse_docblock(&raw));
                    }
                }
            }
            StmtKind::Enum(e) => {
                for member in e.body.members.iter() {
                    match &member.kind {
                        EnumMemberKind::Method(m) if m.name == word => {
                            let raw = docblock_before(source, member.span.start)?;
                            return Some(parse_docblock(&raw));
                        }
                        EnumMemberKind::Case(c) if c.name == word => {
                            let raw = docblock_before(source, member.span.start)?;
                            return Some(parse_docblock(&raw));
                        }
                        EnumMemberKind::ClassConst(k) if k.name == word => {
                            let raw = docblock_before(source, member.span.start)?;
                            return Some(parse_docblock(&raw));
                        }
                        _ => {}
                    }
                }
            }
            StmtKind::Namespace(ns) => {
                if let NamespaceBody::Braced(inner) = &ns.body
                    && let Some(db) = find_docblock(source, &inner.stmts, word)
                {
                    return Some(db);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_description() {
        let raw = "/** Does something useful. */";
        let db = parse_docblock(raw);
        assert_eq!(db.description, "Does something useful.");
    }

    #[test]
    fn parses_return_tag() {
        let raw = "/**\n * @return string The greeting\n */";
        let db = parse_docblock(raw);
        let ret = db.return_type.unwrap();
        assert_eq!(ret.type_hint, "string");
        assert_eq!(ret.description, "The greeting");
    }

    #[test]
    fn parses_param_tag() {
        let raw = "/**\n * @param string $name The user name\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(db.params[0].type_hint, "string");
        assert_eq!(db.params[0].name, "$name");
        assert_eq!(db.params[0].description, "The user name");
    }

    #[test]
    fn parses_var_tag() {
        let raw = "/** @var string */";
        let db = parse_docblock(raw);
        assert_eq!(db.var_type.as_deref(), Some("string"));
    }

    #[test]
    fn parses_var_tag_with_description() {
        let raw = "/** @var string The user's name */";
        let db = parse_docblock(raw);
        assert_eq!(db.var_type.as_deref(), Some("string"));
        assert_eq!(db.var_description.as_deref(), Some("The user's name"));
    }

    #[test]
    fn to_markdown_shows_var_type() {
        let db = Docblock {
            var_type: Some("string".to_string()),
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(
            md.contains("@var"),
            "expected @var in markdown, got: {}",
            md
        );
        assert!(
            md.contains("string"),
            "expected type in markdown, got: {}",
            md
        );
    }

    #[test]
    fn to_markdown_shows_var_type_with_description() {
        let db = Docblock {
            var_type: Some("string".to_string()),
            var_description: Some("The user's name".to_string()),
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(
            md.contains("@var"),
            "expected @var in markdown, got: {}",
            md
        );
        assert!(
            md.contains("string"),
            "expected type in markdown, got: {}",
            md
        );
        assert!(
            md.contains("The user's name"),
            "expected description in markdown, got: {}",
            md
        );
    }

    #[test]
    fn multiple_params() {
        let raw = "/**\n * @param int $a First\n * @param int $b Second\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 2);
        assert_eq!(db.params[0].name, "$a");
        assert_eq!(db.params[1].name, "$b");
    }

    #[test]
    fn to_markdown_includes_description_and_return() {
        let db = Docblock {
            description: "Greets the user.".to_string(),
            params: vec![],
            return_type: Some(DocReturn {
                type_hint: "string".to_string(),
                description: "The greeting".to_string(),
                ty: None,
            }),
            var_type: None,
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(md.contains("Greets the user."));
        assert!(md.contains("@return"));
        assert!(md.contains("string"));
    }

    #[test]
    fn find_docblock_from_ast() {
        use crate::ast::ParsedDoc;
        let src = "<?php\n/** Greets someone. */\nfunction greet() {}";
        let doc = ParsedDoc::parse(src.to_string());
        let db = find_docblock(src, &doc.program().stmts, "greet");
        assert!(db.is_some(), "expected docblock for greet");
        assert!(db.unwrap().description.contains("Greets"));
    }

    #[test]
    fn find_docblock_returns_none_without_docblock() {
        use crate::ast::ParsedDoc;
        let src = "<?php\nfunction greet() {}";
        let doc = ParsedDoc::parse(src.to_string());
        let db = find_docblock(src, &doc.program().stmts, "greet");
        assert!(db.is_none());
    }

    #[test]
    fn empty_docblock_gives_defaults() {
        let db = parse_docblock("/** */");
        assert_eq!(db.description, "");
        assert!(db.return_type.is_none());
        assert!(db.params.is_empty());
    }

    #[test]
    fn parses_deprecated_with_message() {
        let raw = "/**\n * @deprecated Use newMethod() instead\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.deprecated.as_deref(), Some("Use newMethod() instead"));
        assert!(db.is_deprecated());
    }

    #[test]
    fn parses_deprecated_without_message() {
        let raw = "/** @deprecated */";
        let db = parse_docblock(raw);
        assert_eq!(db.deprecated.as_deref(), Some(""));
        assert!(db.is_deprecated());
    }

    #[test]
    fn not_deprecated_when_tag_absent() {
        let raw = "/** Does stuff. */";
        let db = parse_docblock(raw);
        assert!(!db.is_deprecated());
    }

    #[test]
    fn parses_throws_tag() {
        let raw = "/**\n * @throws RuntimeException When something fails\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.throws.len(), 1);
        assert_eq!(db.throws[0].class, "RuntimeException");
        assert_eq!(db.throws[0].description, "When something fails");
    }

    #[test]
    fn parses_multiple_throws() {
        let raw =
            "/**\n * @throws InvalidArgumentException\n * @throws RuntimeException Bad state\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.throws.len(), 2);
        assert_eq!(db.throws[0].class, "InvalidArgumentException");
        assert_eq!(db.throws[1].class, "RuntimeException");
    }

    #[test]
    fn parses_see_tag() {
        let raw = "/**\n * @see OtherClass::method()\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.see.len(), 1);
        assert_eq!(db.see[0], "OtherClass::method()");
    }

    #[test]
    fn parses_link_tag() {
        let raw = "/**\n * @link https://example.com/docs\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.see.len(), 1);
        assert_eq!(db.see[0], "https://example.com/docs");
    }

    #[test]
    fn to_markdown_shows_deprecated_banner() {
        let db = Docblock {
            deprecated: Some("Use bar() instead".to_string()),
            description: "Does foo.".to_string(),
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(
            md.contains("> **Deprecated**"),
            "expected deprecated banner, got: {}",
            md
        );
        assert!(
            md.contains("Use bar() instead"),
            "expected deprecation message, got: {}",
            md
        );
    }

    #[test]
    fn to_markdown_shows_throws() {
        let db = Docblock {
            throws: vec![DocThrows {
                class: "RuntimeException".to_string(),
                description: "On failure".to_string(),
            }],
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(
            md.contains("@throws"),
            "expected @throws in markdown, got: {}",
            md
        );
        assert!(
            md.contains("RuntimeException"),
            "expected class name, got: {}",
            md
        );
    }

    #[test]
    fn to_markdown_shows_see() {
        let db = Docblock {
            see: vec!["https://example.com".to_string()],
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(
            md.contains("@see"),
            "expected @see in markdown, got: {}",
            md
        );
        assert!(
            md.contains("https://example.com"),
            "expected url, got: {}",
            md
        );
    }

    #[test]
    fn parses_template_tag() {
        let raw = "/**\n * @template T\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.templates.len(), 1);
        assert_eq!(db.templates[0].name, "T");
        assert!(db.templates[0].bound.is_none());
    }

    #[test]
    fn parses_template_with_bound() {
        let raw = "/**\n * @template T of BaseClass\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.templates.len(), 1);
        assert_eq!(db.templates[0].name, "T");
        assert_eq!(db.templates[0].bound.as_deref(), Some("BaseClass"));
    }

    #[test]
    fn template_bound_no_over_read() {
        // Guard: mir 0.22 over-read the @template bound, swallowing subsequent
        // tag lines (`@template T of Base` followed by `@param int $x` parsed the
        // bound as `Base @param int $x`). On mir 0.30 the bound is exactly `Base`.
        let raw = "/**\n * @template T of Base\n * @param int $x\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.templates.len(), 1, "expected exactly one @template");
        assert_eq!(db.templates[0].name, "T");
        assert_eq!(
            db.templates[0].bound.as_deref(),
            Some("Base"),
            "bound over-read subsequent tags: {:?}",
            db.templates[0].bound
        );
        // The bound must be a single named type, not a multi-atomic over-read.
        let bound_ty = db.templates[0].bound_ty.as_ref().expect("structured bound");
        assert_eq!(
            bound_ty.types.len(),
            1,
            "bound should be a single atomic, got: {:?}",
            bound_ty
        );
        // The following @param must still parse cleanly and independently.
        assert_eq!(db.params.len(), 1);
        assert_eq!(db.params[0].name, "$x");
        assert_eq!(db.params[0].type_hint, "int");
    }

    #[test]
    fn template_name_no_single_line_over_read() {
        // E1: a single-line docblock where several tags share one line
        // (`@template T @param T $x @return T`) parses cleanly. mir 0.31 yields a
        // template `name` of exactly `T` (no over-read of the trailing tags into
        // the name) and no bound — proven without any php-lsp-side sanitisation.
        let raw = "/** @template T @param T $x @return T */";
        let db = parse_docblock(raw);
        assert_eq!(db.templates.len(), 1);
        assert_eq!(
            db.templates[0].name, "T",
            "template name over-read: {:?}",
            db.templates[0].name
        );
        assert!(db.templates[0].bound.is_none());
    }

    #[test]
    fn template_single_line_with_param_parses_clean_name() {
        // E1 (engine enhancement): the exact single-line shape
        // `/** @template T @param T $x */` parses cleanly — mir 0.31 no longer
        // over-reads the trailing `@param T $x` into the template name, so the
        // template name is exactly `T` (with no bound). Verified WITHOUT any
        // php-lsp-side `sanitize_template_name` workaround (removed when consuming
        // the enhanced mir).
        let raw = "/** @template T @param T $x */";
        let db = parse_docblock(raw);
        assert_eq!(db.templates.len(), 1, "expected exactly one @template");
        assert_eq!(
            db.templates[0].name, "T",
            "single-line @template name over-read: {:?}",
            db.templates[0].name
        );
        assert!(
            db.templates[0].bound.is_none(),
            "single-line @template must have no bound, got: {:?}",
            db.templates[0].bound
        );
    }

    #[test]
    fn template_records_variance() {
        let raw = "/**\n * @template-covariant TOut\n * @template-contravariant TIn of Base\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.templates.len(), 2);
        assert_eq!(db.templates[0].name, "TOut");
        assert_eq!(db.templates[0].variance, Variance::Covariant);
        assert!(db.templates[0].bound.is_none());
        assert_eq!(db.templates[1].name, "TIn");
        assert_eq!(db.templates[1].variance, Variance::Contravariant);
        assert_eq!(db.templates[1].bound.as_deref(), Some("Base"));
    }

    #[test]
    fn param_carries_structured_generic_type() {
        let raw = "/**\n * @param Collection<User> $items\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        // Back-compat String hint preserved.
        assert_eq!(db.params[0].type_hint, "Collection<User>");
        // Structured type carried additively, with the generic arg intact.
        let ty = db.params[0].ty.as_ref().expect("structured param type");
        assert!(
            matches!(
                &ty.types[0],
                mir_types::Atomic::TNamedObject { type_params, .. } if !type_params.is_empty()
            ),
            "expected generic TNamedObject, got: {:?}",
            ty
        );
    }

    #[test]
    fn return_carries_structured_type() {
        let raw = "/**\n * @return list<int>\n */";
        let db = parse_docblock(raw);
        let ret = db.return_type.as_ref().expect("return");
        assert_eq!(ret.type_hint, "list<int>");
        let ty = ret.ty.as_ref().expect("structured return type");
        assert!(
            matches!(&ty.types[0], mir_types::Atomic::TList { .. }),
            "expected TList, got: {:?}",
            ty
        );
    }

    #[test]
    fn docblock_captures_extends_and_implements() {
        let raw = "/**\n * @extends BaseRepo<User>\n * @implements IteratorAggregate<User>\n */";
        let db = parse_docblock(raw);
        let extends = db.extends.as_ref().expect("extends");
        assert!(
            matches!(
                &extends.types[0],
                mir_types::Atomic::TNamedObject { fqcn, type_params }
                    if fqcn.as_str() == "BaseRepo" && !type_params.is_empty()
            ),
            "expected BaseRepo<User>, got: {:?}",
            extends
        );
        assert_eq!(db.implements.len(), 1);
    }

    #[test]
    fn parses_mixin_tag() {
        let raw = "/**\n * @mixin SomeTrait\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.mixins.len(), 1);
        assert_eq!(db.mixins[0], "SomeTrait");
    }

    #[test]
    fn parses_callable_param() {
        let raw = "/**\n * @param callable(int, string): void $fn The callback\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(db.params[0].type_hint, "callable(int, string): void");
        assert_eq!(db.params[0].name, "$fn");
        assert_eq!(db.params[0].description, "The callback");
    }

    #[test]
    fn to_markdown_shows_template() {
        let db = Docblock {
            templates: vec![DocTemplate {
                name: "T".to_string(),
                bound: Some("Base".to_string()),
                bound_ty: None,
                variance: Variance::Invariant,
            }],
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(
            md.contains("@template"),
            "expected @template in markdown, got: {}",
            md
        );
        assert!(md.contains("T"), "expected T in markdown");
        assert!(md.contains("Base"), "expected Base in markdown");
    }

    #[test]
    fn to_markdown_shows_mixin() {
        let db = Docblock {
            mixins: vec!["SomeTrait".to_string()],
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(
            md.contains("@mixin"),
            "expected @mixin in markdown, got: {}",
            md
        );
        assert!(md.contains("SomeTrait"), "expected SomeTrait in markdown");
    }

    #[test]
    fn parses_psalm_type_alias() {
        let raw = "/**\n * @psalm-type UserId = string|int\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.type_aliases.len(), 1);
        assert_eq!(db.type_aliases[0].name, "UserId");
        assert_eq!(db.type_aliases[0].type_expr, "string|int");
    }

    #[test]
    fn parses_phpstan_type_alias() {
        let raw = "/** @phpstan-type Row = array{id: int, name: string} */";
        let db = parse_docblock(raw);
        assert_eq!(db.type_aliases.len(), 1);
        assert_eq!(db.type_aliases[0].name, "Row");
        assert!(db.type_aliases[0].type_expr.contains("array"));
    }

    #[test]
    fn to_markdown_shows_type_alias() {
        let db = Docblock {
            type_aliases: vec![DocTypeAlias {
                name: "Status".to_string(),
                type_expr: "string".to_string(),
            }],
            ..Default::default()
        };
        let md = db.to_markdown();
        assert!(md.contains("Status"), "expected alias name in markdown");
        assert!(md.contains("string"), "expected type expr in markdown");
    }

    #[test]
    fn parses_property_tag() {
        let src = "/** @property string $name */";
        let db = parse_docblock(src);
        assert_eq!(db.properties.len(), 1);
        assert_eq!(db.properties[0].name, "name");
        assert_eq!(db.properties[0].type_hint, "string");
        assert!(!db.properties[0].read_only);
    }

    #[test]
    fn parses_property_read_tag() {
        let src = "/** @property-read Carbon $createdAt */";
        let db = parse_docblock(src);
        assert_eq!(db.properties[0].name, "createdAt");
        assert!(db.properties[0].read_only);
    }

    #[test]
    fn parses_method_tag() {
        let src = "/** @method User find(int $id) */";
        let db = parse_docblock(src);
        assert_eq!(db.methods.len(), 1);
        assert_eq!(db.methods[0].name, "find");
        assert_eq!(db.methods[0].return_type, "User");
        assert!(!db.methods[0].is_static);
    }

    #[test]
    fn parses_static_method_tag() {
        let src = "/** @method static Builder where(string $col, mixed $val) */";
        let db = parse_docblock(src);
        assert!(db.methods[0].is_static);
        assert_eq!(db.methods[0].name, "where");
    }

    #[test]
    fn psalm_param_alias_parsed_as_param() {
        let raw = "/**\n * @psalm-param string $x The value\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(db.params[0].type_hint, "string");
        assert_eq!(db.params[0].name, "$x");
    }

    #[test]
    fn phpstan_param_alias_parsed_as_param() {
        let raw = "/**\n * @phpstan-param int $count\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(db.params[0].type_hint, "int");
        assert_eq!(db.params[0].name, "$count");
    }

    #[test]
    fn psalm_return_alias_parsed_as_return() {
        let raw = "/**\n * @psalm-return non-empty-string\n */";
        let db = parse_docblock(raw);
        assert_eq!(
            db.return_type.as_ref().map(|r| r.type_hint.as_str()),
            Some("non-empty-string")
        );
    }

    #[test]
    fn phpstan_return_alias_parsed_as_return() {
        let raw = "/**\n * @phpstan-return array<int, string>\n */";
        let db = parse_docblock(raw);
        assert_eq!(
            db.return_type.as_ref().map(|r| r.type_hint.as_str()),
            Some("array<int, string>")
        );
    }

    #[test]
    fn psalm_var_alias_parsed_as_var() {
        let raw = "/** @psalm-var Foo $item */";
        let db = parse_docblock(raw);
        assert_eq!(db.var_type.as_deref(), Some("Foo"));
        assert_eq!(db.var_name.as_deref(), Some("item"));
    }

    #[test]
    fn phpstan_var_alias_parsed_as_var() {
        let raw = "/** @phpstan-var string */";
        let db = parse_docblock(raw);
        assert_eq!(db.var_type.as_deref(), Some("string"));
    }

    #[test]
    fn param_without_description_parses_correctly() {
        let raw = "/**\n * @param string $x\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(
            db.params[0].type_hint, "string",
            "type_hint should be 'string'"
        );
        assert_eq!(db.params[0].name, "$x", "name should be '$x'");
        assert_eq!(
            db.params[0].description, "",
            "description should be empty when absent"
        );
    }

    #[test]
    fn union_type_param_parsed() {
        let raw = "/**\n * @param Foo|Bar $x Some value\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(
            db.params[0].type_hint, "Foo|Bar",
            "union type should be 'Foo|Bar', got: {}",
            db.params[0].type_hint
        );
        assert_eq!(db.params[0].name, "$x");
    }

    #[test]
    fn nullable_type_param_parsed() {
        // `?Foo` is normalized to the canonical `Foo|null` form.
        let raw = "/**\n * @param ?Foo $x\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(
            db.params[0].type_hint, "Foo|null",
            "nullable type should be 'Foo|null', got: {}",
            db.params[0].type_hint
        );
        assert_eq!(db.params[0].name, "$x");
    }

    #[test]
    fn method_tag_extracts_return_type() {
        let raw = "/**\n * @method string getName()\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.methods.len(), 1);
        assert_eq!(
            db.methods[0].return_type, "string",
            "return_type should be 'string', got: {}",
            db.methods[0].return_type
        );
        assert_eq!(
            db.methods[0].name, "getName",
            "name should be 'getName', got: {}",
            db.methods[0].name
        );
        assert!(!db.methods[0].is_static, "should not be static");
    }

    #[test]
    fn advanced_type_non_empty_string() {
        // mir resolves psalm/phpstan special types; non-empty-string must round-trip.
        let raw = "/**\n * @return non-empty-string\n */";
        let db = parse_docblock(raw);
        assert_eq!(
            db.return_type.as_ref().map(|r| r.type_hint.as_str()),
            Some("non-empty-string"),
            "non-empty-string should be preserved, got: {:?}",
            db.return_type
        );
    }

    #[test]
    fn advanced_type_generic_array() {
        // array<K, V> generic syntax must round-trip through mir's Union display.
        let raw = "/**\n * @param array<int, string> $map\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.params.len(), 1);
        assert_eq!(
            db.params[0].type_hint, "array<int, string>",
            "generic array type should be preserved, got: {}",
            db.params[0].type_hint
        );
    }

    #[test]
    fn param_and_return_descriptions_preserved() {
        // Descriptions from @param and @return are captured via php-rs-parser
        // (mir discards them). Verify they survive the full parse_docblock() call.
        let raw = "/**\n * @param string $name The user name\n * @return int The age\n */";
        let db = parse_docblock(raw);
        assert_eq!(
            db.params[0].description, "The user name",
            "param description should be preserved"
        );
        assert_eq!(
            db.return_type.as_ref().map(|r| r.description.as_str()),
            Some("The age"),
            "return description should be preserved"
        );
    }

    #[test]
    fn throws_description_preserved() {
        // @throws description must survive the adapter (mir only stores the class).
        let raw = "/**\n * @throws RuntimeException When the server is down\n */";
        let db = parse_docblock(raw);
        assert_eq!(db.throws.len(), 1);
        assert_eq!(db.throws[0].class, "RuntimeException");
        assert_eq!(
            db.throws[0].description, "When the server is down",
            "throws description should be preserved"
        );
    }
}
