//! Type rendering bridge: `mir_types::Type` / `Atomic` → import-aware SHORT-name
//! display strings.
//!
//! This is the single place the LSP turns a mir `Type` into user-facing text.
//! It deliberately **never** pipes mir's own `Display` impl verbatim, because
//! mir `Display`:
//!   * emits the FULL FQCN for named objects (`App\Models\User` instead of
//!     `User`), and
//!   * renders `self`/`static`/`parent` as `self(X)`/`static(X)`/`parent(X)`.
//!
//! Both are wrong for IDE output. [`render_type`] reproduces mir's structure
//! (unions joined by `|`, generic args in `<...>`, intersections joined by `&`)
//! while shortening class names and emitting plain `self`/`static`/`parent`.
//!
//! ## `ImportCtx`
//! [`ImportCtx`] carries the namespace/use information needed to resolve a FQCN
//! to the shortest name that is unambiguous at the call site. It is intentionally
//! cheap to construct:
//!   * [`ImportCtx::short`] (or `ImportCtx::default()`) — no import data; every
//!     class name is shortened to its last `\`-segment. This matches the existing
//!     LSP behaviour (`type_map.rs` `fqcn.rsplit('\\').next()`), so callers that
//!     have no use-map (WP2 hover/inlay/type-def, WP3 completion) can start here
//!     and produce byte-identical output to today's non-generic rendering.
//!   * [`ImportCtx::with_uses`] — supply a use-map (`FQCN` → alias) so that an
//!     imported/aliased class renders with the in-file alias instead of the raw
//!     short segment. The map is borrowed, so construction is a single pointer
//!     copy.
//!
//! WP2/WP3 construct an `ImportCtx` directly at the call site; nothing in this
//! module needs a `DocumentStore`, AST, or analyzer handle.

use std::collections::HashMap;

use mir_types::{Atomic, Type, Variance};

/// Import / namespace context used to resolve an FQCN to a short, in-file name.
///
/// Construct with [`ImportCtx::short`] (the zero-config default that mirrors the
/// existing LSP short-name behaviour) or [`ImportCtx::with_uses`] when a use-map
/// is available. All fields are borrowed, so an `ImportCtx` is cheap to build
/// and carry around per request.
#[derive(Debug, Clone, Copy, Default)]
pub struct ImportCtx<'a> {
    /// Optional map of fully-qualified class name (no leading `\`) → the name to
    /// display (typically the in-file alias). When a lookup misses, the renderer
    /// falls back to the last `\`-separated segment of the FQCN.
    uses: Option<&'a HashMap<String, String>>,
}

impl<'a> ImportCtx<'a> {
    /// A context with no import data. Every class name is shortened to its final
    /// `\`-separated segment — identical to the existing LSP behaviour.
    pub fn short() -> Self {
        Self { uses: None }
    }

    /// A context backed by a use-map (`FQCN` → display name). The map is keyed by
    /// the FQCN **without** a leading backslash.
    pub fn with_uses(uses: &'a HashMap<String, String>) -> Self {
        Self { uses: Some(uses) }
    }

    /// Resolve a fully-qualified class name to its display string.
    ///
    /// Strips any leading `\`, consults the use-map if present, and otherwise
    /// returns the final `\`-separated segment.
    fn short_name(&self, fqcn: &str) -> String {
        let trimmed = fqcn.trim_start_matches('\\');
        if let Some(uses) = self.uses
            && let Some(alias) = uses.get(trimmed)
        {
            return alias.clone();
        }
        trimmed.rsplit('\\').next().unwrap_or(trimmed).to_string()
    }
}

/// Render a mir [`Type`] to an import-aware, SHORT-name display string.
///
/// Walks `Type.types`, rendering each [`Atomic`] and joining the results with
/// `|`. An empty union renders as `never` (matching mir's `Display`).
///
/// Generic arguments (`<...>`), array/list element types, intersection parts,
/// callable/closure signatures and conditional branches are all rendered
/// recursively through the same short-name path.
pub fn render_type(ty: &Type, ctx: &ImportCtx) -> String {
    if ty.types.is_empty() {
        return "never".to_string();
    }
    let parts: Vec<String> = ty.types.iter().map(|a| render_atomic(a, ctx)).collect();
    parts.join("|")
}

/// Render a single [`Atomic`].
fn render_atomic(atomic: &Atomic, ctx: &ImportCtx) -> String {
    match atomic {
        // --- Scalars / strings ---
        Atomic::TString => "string".to_string(),
        Atomic::TLiteralString(s) => format!("\"{s}\""),
        Atomic::TCallableString => "callable-string".to_string(),
        Atomic::TClassString(None) => "class-string".to_string(),
        Atomic::TClassString(Some(cls)) => {
            format!("class-string<{}>", ctx.short_name(cls.as_str()))
        }
        Atomic::TNonEmptyString => "non-empty-string".to_string(),
        Atomic::TNumericString => "numeric-string".to_string(),

        // --- Ints ---
        Atomic::TInt => "int".to_string(),
        Atomic::TLiteralInt(n) => n.to_string(),
        Atomic::TIntRange { min, max } => match (min, max) {
            (None, None) => "int".to_string(),
            (lo, hi) => {
                let lo = lo.map_or_else(|| "min".to_string(), |n| n.to_string());
                let hi = hi.map_or_else(|| "max".to_string(), |n| n.to_string());
                format!("int<{lo}, {hi}>")
            }
        },
        Atomic::TPositiveInt => "positive-int".to_string(),
        Atomic::TNegativeInt => "negative-int".to_string(),
        Atomic::TNonNegativeInt => "non-negative-int".to_string(),

        // --- Floats / bools / null ---
        Atomic::TFloat => "float".to_string(),
        Atomic::TLiteralFloat(high, low) => {
            let bits = ((*high as u64) << 32) | (*low as u32 as u64);
            let value = f64::from_bits(bits);
            format!("{value}")
        }
        Atomic::TBool => "bool".to_string(),
        Atomic::TTrue => "true".to_string(),
        Atomic::TFalse => "false".to_string(),
        Atomic::TNull => "null".to_string(),

        // --- Bottom / top ---
        Atomic::TVoid => "void".to_string(),
        Atomic::TNever => "never".to_string(),
        Atomic::TMixed => "mixed".to_string(),
        Atomic::TScalar => "scalar".to_string(),
        Atomic::TNumeric => "numeric".to_string(),

        // --- Objects ---
        Atomic::TObject => "object".to_string(),
        Atomic::TNamedObject { fqcn, type_params } => {
            let base = ctx.short_name(fqcn.as_str());
            if type_params.is_empty() {
                base
            } else {
                let args: Vec<String> = type_params.iter().map(|p| render_type(p, ctx)).collect();
                format!("{base}<{}>", args.join(", "))
            }
        }
        // mir Display renders these as `self(X)` / `static(X)` / `parent(X)`.
        // The LSP must show the bare keyword instead.
        Atomic::TSelf { .. } => "self".to_string(),
        Atomic::TStaticObject { .. } => "static".to_string(),
        Atomic::TParent { .. } => "parent".to_string(),

        // --- Callables ---
        Atomic::TCallable {
            params: None,
            return_type: None,
        } => "callable".to_string(),
        Atomic::TCallable {
            params,
            return_type,
        } => {
            let ps = render_fn_params(params.as_deref().unwrap_or(&[]), ctx);
            let ret = return_type
                .as_ref()
                .map_or_else(|| "mixed".to_string(), |r| render_type(r, ctx));
            format!("callable({ps}): {ret}")
        }
        Atomic::TClosure {
            params,
            return_type,
            ..
        } => {
            let ps = render_fn_params(params, ctx);
            format!("Closure({ps}): {}", render_type(return_type, ctx))
        }

        // --- Arrays ---
        Atomic::TArray { key, value } => {
            format!(
                "array<{}, {}>",
                render_type(key, ctx),
                render_type(value, ctx)
            )
        }
        Atomic::TList { value } => format!("list<{}>", render_type(value, ctx)),
        Atomic::TNonEmptyArray { key, value } => format!(
            "non-empty-array<{}, {}>",
            render_type(key, ctx),
            render_type(value, ctx)
        ),
        Atomic::TNonEmptyList { value } => {
            format!("non-empty-list<{}>", render_type(value, ctx))
        }
        Atomic::TKeyedArray { properties, .. } => {
            let entries: Vec<String> = properties
                .iter()
                .map(|(k, v)| {
                    let key_str = match k {
                        mir_types::ArrayKey::String(s) => format!("'{s}'"),
                        mir_types::ArrayKey::Int(n) => n.to_string(),
                    };
                    let opt = if v.optional { "?" } else { "" };
                    format!("{}{}: {}", key_str, opt, render_type(&v.ty, ctx))
                })
                .collect();
            format!("array{{{}}}", entries.join(", "))
        }

        // --- Generics / meta-types ---
        Atomic::TTemplateParam { name, .. } => name.as_str().to_string(),
        Atomic::TConditional {
            param_name,
            subject,
            if_true,
            if_false,
        } => {
            let st = render_type(subject, ctx);
            let t = render_type(if_true, ctx);
            let f = render_type(if_false, ctx);
            match param_name {
                Some(name) => format!("(${name} is {st} ? {t} : {f})"),
                None => format!("({st} is ? {t} : {f})"),
            }
        }

        // --- Special object strings ---
        Atomic::TInterfaceString => "interface-string".to_string(),
        Atomic::TEnumString => "enum-string".to_string(),
        Atomic::TTraitString => "trait-string".to_string(),

        // --- Enum cases ---
        Atomic::TLiteralEnumCase {
            enum_fqcn,
            case_name,
        } => {
            format!(
                "{}::{}",
                ctx.short_name(enum_fqcn.as_str()),
                case_name.as_str()
            )
        }

        // --- Intersection ---
        Atomic::TIntersection { parts } => {
            let rendered: Vec<String> = parts.iter().map(|p| render_type(p, ctx)).collect();
            rendered.join("&")
        }
    }
}

/// Whether a resolved [`Type`] is "generic-relevant" — i.e. carries information
/// that the legacy `TypeMap` string path cannot express and that the generics
/// feature is meant to surface (WP2 hover/inlay override).
///
/// Returns `true` when the type is (or recursively contains) an object/named
/// type or a template param / generic argument:
///   * a named/object type — `TNamedObject`, `TSelf`, `TStaticObject`,
///     `TParent`, `TTemplateParam` — at any nesting depth, OR
///   * a container (`TArray`/`TList`/`class-string<…>`/keyed-array/callable/
///     closure/conditional/intersection) whose inner type is itself
///     generic-relevant.
///
/// Returns `false` for a BARE scalar/literal/`mixed`/`void`/`null`/`bool`/
/// `object`/`callable` etc. carrying no generic information. Callers gate the
/// resolved-type override on this (in addition to "differs from legacy"), so a
/// non-generic variable like `$x = 1; $x;` never gets a resolved-type hover/inlay
/// that the legacy path wouldn't already produce.
pub fn is_generic_relevant(ty: &Type) -> bool {
    ty.types.iter().any(atomic_is_generic_relevant)
}

fn atomic_is_generic_relevant(atomic: &Atomic) -> bool {
    match atomic {
        // Object / named / template types are always generic-relevant: this is the
        // core VF2B case (`@return T` with `T = User` ⇒ `TNamedObject`).
        Atomic::TNamedObject { .. }
        | Atomic::TSelf { .. }
        | Atomic::TStaticObject { .. }
        | Atomic::TParent { .. }
        | Atomic::TTemplateParam { .. } => true,

        // Containers are relevant only when they carry a relevant inner type
        // (e.g. `list<User>`, `array<string, T>`, `class-string<T>`).
        Atomic::TClassString(Some(_)) => true,
        Atomic::TArray { key, value } | Atomic::TNonEmptyArray { key, value } => {
            is_generic_relevant(key) || is_generic_relevant(value)
        }
        Atomic::TList { value } | Atomic::TNonEmptyList { value } => is_generic_relevant(value),
        Atomic::TKeyedArray { properties, .. } => {
            properties.values().any(|p| is_generic_relevant(&p.ty))
        }
        Atomic::TCallable {
            params,
            return_type,
        } => {
            params
                .as_deref()
                .is_some_and(|ps| ps.iter().any(fn_param_is_generic_relevant))
                || return_type.as_deref().is_some_and(is_generic_relevant)
        }
        Atomic::TClosure {
            params,
            return_type,
            this_type,
        } => {
            params.iter().any(fn_param_is_generic_relevant)
                || is_generic_relevant(return_type)
                || this_type.as_deref().is_some_and(is_generic_relevant)
        }
        Atomic::TConditional {
            subject,
            if_true,
            if_false,
            ..
        } => {
            is_generic_relevant(subject)
                || is_generic_relevant(if_true)
                || is_generic_relevant(if_false)
        }
        Atomic::TIntersection { parts } => parts.iter().any(is_generic_relevant),

        // Everything else (scalars, literals, `mixed`/`void`/`null`/`object`/
        // bare `class-string`/`callable`/etc.) carries no generic information.
        _ => false,
    }
}

fn fn_param_is_generic_relevant(param: &mir_types::atomic::FnParam) -> bool {
    param
        .ty
        .as_ref()
        .is_some_and(|simple| is_generic_relevant(&simple.to_union()))
}

/// Render a callable/closure parameter list, short-name aware.
fn render_fn_params(params: &[mir_types::atomic::FnParam], ctx: &ImportCtx) -> String {
    let ps: Vec<String> = params
        .iter()
        .map(|p| match &p.ty {
            Some(simple) => render_type(&simple.to_union(), ctx),
            None => "mixed".to_string(),
        })
        .collect();
    ps.join(", ")
}

/// Render a declaration-site template parameter for hover/decl output, e.g.
/// `@template-covariant T of Base`. mir keeps `Variance` **only** on
/// declaration-site [`mir_types::TemplateParam`] (not on `TNamedObject` args),
/// so this is the single place a variance keyword is emitted.
///
/// Output shape (no leading `@template`):
///   * `T`
///   * `T of Base`            (invariant, bound present)
///   * `covariant T`          (covariant, no bound)
///   * `contravariant T of Base`
pub fn render_template_decl(
    name: &str,
    bound: Option<&Type>,
    variance: Variance,
    ctx: &ImportCtx,
) -> String {
    let mut out = String::new();
    match variance {
        Variance::Invariant => {}
        Variance::Covariant => out.push_str("covariant "),
        Variance::Contravariant => out.push_str("contravariant "),
    }
    out.push_str(name);
    if let Some(b) = bound {
        out.push_str(" of ");
        out.push_str(&render_type(b, ctx));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir_analyzer::parser::docblock::parse_type_string;

    /// Parse a PHPStan-style type string into a mir `Type` for testing.
    fn ty(s: &str) -> Type {
        parse_type_string(s)
    }

    fn render(s: &str) -> String {
        render_type(&ty(s), &ImportCtx::short())
    }

    // --- Golden generic tests (acceptance) ---

    #[test]
    fn renders_generic_named_object() {
        assert_eq!(render("Collection<User>"), "Collection<User>");
    }

    #[test]
    fn renders_generic_array() {
        assert_eq!(render("array<string, User>"), "array<string, User>");
    }

    #[test]
    fn renders_list() {
        assert_eq!(render("list<int>"), "list<int>");
    }

    #[test]
    fn renders_nullable_union() {
        assert_eq!(render("User|null"), "User|null");
    }

    #[test]
    fn renders_intersection() {
        assert_eq!(render("Iterator&Countable"), "Iterator&Countable");
    }

    #[test]
    fn renders_class_string_generic() {
        assert_eq!(render("class-string<User>"), "class-string<User>");
    }

    #[test]
    fn renders_bare_named_object() {
        assert_eq!(render("User"), "User");
    }

    #[test]
    fn renders_template_param_name() {
        // From a bare string, `T` parses to TNamedObject{fqcn:"T"} and renders
        // short as `T`. A real TTemplateParam renders via its `name` too.
        assert_eq!(render("T"), "T");
        let tpl = Type::single(Atomic::TTemplateParam {
            name: "T".into(),
            as_type: Box::new(Type::mixed()),
            defining_entity: "App\\Collection".into(),
        });
        assert_eq!(render_type(&tpl, &ImportCtx::short()), "T");
    }

    #[test]
    fn renders_mixed() {
        assert_eq!(render("mixed"), "mixed");
    }

    #[test]
    fn renders_self_keyword() {
        // mir Display would emit `self()`; we emit the bare keyword.
        assert_eq!(render("self"), "self");
        let s = Type::single(Atomic::TSelf {
            fqcn: "App\\Foo".into(),
        });
        assert_eq!(render_type(&s, &ImportCtx::short()), "self");
    }

    // --- Non-generic regression corpus (byte-identical to today's docblock
    //     `.to_string()` output) ---

    #[test]
    fn regression_corpus_matches_today() {
        // Today, docblock hints render through mir's `Display`. These must be
        // byte-identical, except for FQCN shortening / self-static-parent which
        // are handled by short names elsewhere.
        assert_eq!(render("User"), "User");
        assert_eq!(render("?User"), "User|null");
        assert_eq!(render("User|Order"), "User|Order");
        assert_eq!(render("array"), "array<mixed, mixed>");
        assert_eq!(render("int"), "int");
    }

    // --- Short-name / import resolution ---

    #[test]
    fn shortens_fqcn_by_default() {
        assert_eq!(render("App\\Models\\User"), "User");
        assert_eq!(render("\\App\\Models\\User"), "User");
        assert_eq!(
            render("App\\Models\\Collection<App\\Models\\User>"),
            "Collection<User>"
        );
    }

    #[test]
    fn uses_alias_from_use_map() {
        let mut uses = std::collections::HashMap::new();
        uses.insert("App\\Models\\User".to_string(), "UserModel".to_string());
        let ctx = ImportCtx::with_uses(&uses);
        let t = ty("App\\Models\\User");
        assert_eq!(render_type(&t, &ctx), "UserModel");
        // A FQCN not in the map still shortens.
        let t2 = ty("App\\Other\\Thing");
        assert_eq!(render_type(&t2, &ctx), "Thing");
    }

    // --- Additional atomic coverage ---

    #[test]
    fn renders_non_empty_list() {
        assert_eq!(render("non-empty-list<int>"), "non-empty-list<int>");
    }

    #[test]
    fn renders_class_string_bare() {
        assert_eq!(render("class-string"), "class-string");
    }

    #[test]
    fn renders_scalars_and_special() {
        assert_eq!(render("string"), "string");
        assert_eq!(render("bool"), "bool");
        assert_eq!(render("float"), "float");
        assert_eq!(render("void"), "void");
        assert_eq!(render("object"), "object");
        assert_eq!(render("non-empty-string"), "non-empty-string");
    }

    #[test]
    fn empty_union_renders_never() {
        assert_eq!(render_type(&Type::empty(), &ImportCtx::short()), "never");
    }

    #[test]
    fn renders_int_range() {
        let t = Type::single(Atomic::TIntRange {
            min: Some(1),
            max: Some(10),
        });
        assert_eq!(render_type(&t, &ImportCtx::short()), "int<1, 10>");
    }

    #[test]
    fn renders_literal_enum_case_short() {
        let t = Type::single(Atomic::TLiteralEnumCase {
            enum_fqcn: "App\\Status".into(),
            case_name: "Active".into(),
        });
        assert_eq!(render_type(&t, &ImportCtx::short()), "Status::Active");
    }

    #[test]
    fn renders_static_and_parent_keywords() {
        let st = Type::single(Atomic::TStaticObject {
            fqcn: "App\\Foo".into(),
        });
        assert_eq!(render_type(&st, &ImportCtx::short()), "static");
        let pa = Type::single(Atomic::TParent {
            fqcn: "App\\Base".into(),
        });
        assert_eq!(render_type(&pa, &ImportCtx::short()), "parent");
    }

    #[test]
    fn renders_callable_signature_short() {
        // `callable(App\\User): App\\Order` should short-name both sides.
        let t = ty("callable(App\\Models\\User): App\\Models\\Order");
        let out = render_type(&t, &ImportCtx::short());
        assert!(out.starts_with("callable("), "got: {out}");
        assert!(out.contains("User"), "got: {out}");
        assert!(out.contains("Order"), "got: {out}");
        assert!(!out.contains("App\\"), "FQCN leaked: {out}");
    }

    // --- Template declaration rendering (variance) ---

    #[test]
    fn renders_template_decl_invariant_no_bound() {
        assert_eq!(
            render_template_decl("T", None, Variance::Invariant, &ImportCtx::short()),
            "T"
        );
    }

    #[test]
    fn renders_template_decl_with_bound() {
        let bound = ty("App\\Models\\Base");
        assert_eq!(
            render_template_decl("T", Some(&bound), Variance::Invariant, &ImportCtx::short()),
            "T of Base"
        );
    }

    #[test]
    fn renders_template_decl_covariant() {
        assert_eq!(
            render_template_decl("T", None, Variance::Covariant, &ImportCtx::short()),
            "covariant T"
        );
    }

    #[test]
    fn renders_template_decl_contravariant_with_bound() {
        let bound = ty("Base");
        assert_eq!(
            render_template_decl(
                "TIn",
                Some(&bound),
                Variance::Contravariant,
                &ImportCtx::short()
            ),
            "contravariant TIn of Base"
        );
    }

    // --- is_generic_relevant (Carryover-1) ---

    #[test]
    fn generic_relevant_for_objects_and_templates() {
        // Named objects (the core VF2B case: `@return T` ⇒ `T = User`).
        assert!(is_generic_relevant(&ty("User")));
        assert!(is_generic_relevant(&ty("Collection<User>")));
        // self/static/parent and a real template param.
        assert!(is_generic_relevant(&Type::single(Atomic::TSelf {
            fqcn: "App\\Foo".into()
        })));
        assert!(is_generic_relevant(&Type::single(Atomic::TStaticObject {
            fqcn: "App\\Foo".into()
        })));
        assert!(is_generic_relevant(&Type::single(Atomic::TParent {
            fqcn: "App\\Foo".into()
        })));
        assert!(is_generic_relevant(&Type::single(Atomic::TTemplateParam {
            name: "T".into(),
            as_type: Box::new(Type::mixed()),
            defining_entity: "App\\Collection".into(),
        })));
    }

    #[test]
    fn generic_relevant_for_containers_carrying_objects() {
        assert!(is_generic_relevant(&ty("list<User>")));
        assert!(is_generic_relevant(&ty("array<string, User>")));
        assert!(is_generic_relevant(&ty("class-string<User>")));
        assert!(is_generic_relevant(&ty("User|null")));
        assert!(is_generic_relevant(&ty("Iterator&Countable")));
    }

    #[test]
    fn not_generic_relevant_for_bare_scalars() {
        // Bare scalars / literals / top-bottom types must NOT override the legacy
        // path — this is what kills the VF2B over-fire (`$x = 1; $x;`).
        assert!(!is_generic_relevant(&ty("int")));
        assert!(!is_generic_relevant(&Type::single(Atomic::TLiteralInt(1))));
        assert!(!is_generic_relevant(&ty("string")));
        assert!(!is_generic_relevant(&Type::single(Atomic::TLiteralString(
            "str".into()
        ))));
        assert!(!is_generic_relevant(&ty("bool")));
        assert!(!is_generic_relevant(&ty("float")));
        assert!(!is_generic_relevant(&ty("mixed")));
        assert!(!is_generic_relevant(&ty("void")));
        assert!(!is_generic_relevant(&ty("null")));
        assert!(!is_generic_relevant(&ty("object")));
        assert!(!is_generic_relevant(&ty("class-string")));
        // A scalar-only container is likewise not generic-relevant.
        assert!(!is_generic_relevant(&ty("list<int>")));
        assert!(!is_generic_relevant(&ty("array<string, int>")));
        assert!(!is_generic_relevant(&ty("int|string")));
    }
}
