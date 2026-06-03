//! WP2 generic-aware feature coverage: hover / inlay / type-definition over
//! PHPStan-style generics, plus a non-generic regression golden.
//!
//! All generic behavior is gated behind mir's resolved-symbol cache
//! (`resolved_type_at`); when it returns `None` the legacy path runs unchanged,
//! which the regression test pins.

use super::*;

use expect_test::expect;

/// Hover on a variable typed `Collection<User>` renders the full generic type
/// (the legacy path would show only `Collection`).
#[tokio::test]
async fn hover_var_shows_generic_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
/**
 * @template T
 */
class Collection {
    /** @return T */
    public function first() {}
}
class User {}

/** @var Collection<User> $c */
$c = new Collection();
$c$0;
"#,
        )
        .await;
    expect![[r#"`$c` `Collection<User>`"#]].assert_eq(&out);
}

/// Hover on a `@template T of Base` declaration renders the bound.
#[tokio::test]
async fn hover_template_decl_shows_bound() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
class Base {}
/**
 * @template T of Ba$0se
 */
class Collection {}
"#,
        )
        .await;
    expect![[r#"`@template T of Base`"#]].assert_eq(&out);
}

/// Hover on a `@template-covariant T of Base` declaration renders the variance
/// keyword and the bound.
#[tokio::test]
async fn hover_template_decl_shows_variance() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
class Base {}
/**
 * @template-covariant T$0 of Base
 */
class Collection {}
"#,
        )
        .await;
    expect![[r#"`@template-covariant T of Base`"#]].assert_eq(&out);
}

/// Inlay return-type hint prefers mir's generic-aware return type.
#[tokio::test]
async fn inlay_shows_generic_return_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_inlay_hints(
            r#"<?php
/** @template T */
class Collection {
    /** @return T */
    public function first() {}
}
class User {}

/**
 * @return Collection<User>
 */
function make(): Collection { return new Collection(); }

$c = make();
"#,
        )
        .await;
    assert!(
        out.contains("Collection<User>"),
        "expected inlay hint to show Collection<User>, got: {out}"
    );
}

/// Go-to-type-definition on a `Collection<User>` variable navigates to the
/// `Collection` class declaration (the base, not the type argument).
#[tokio::test]
async fn type_definition_navigates_to_base_class() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_type_definition(
            r#"<?php
/** @template T */
class Collection {}
class User {}

/** @var Collection<User> $c */
$c = new Collection();
$c$0;
"#,
        )
        .await;
    // Line 2 (0-based) is `class Collection {}` — the base, not `User`.
    expect![[r#"main.php:2:6-2:16"#]].assert_eq(&out);
}

/// VF2B: a plain template substitution (`@return T` with `T = User`) renders as
/// the bracket-less `User`. The old `<`-in-rendered-text gate dropped this; the
/// fix surfaces the resolved type whenever it differs from the legacy value.
#[tokio::test]
async fn hover_var_shows_plain_substituted_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
/** @template T */
class BaseRepo {
    /** @return T */
    public function find() {}
}
/** @extends BaseRepo<User> */
class UserRepo extends BaseRepo {}
class User {}

$repo = new UserRepo();
$u = $repo->find();
$u$0;
"#,
        )
        .await;
    // `$u` is the substituted element type `User` (plain, no `<...>`). The legacy
    // path would not resolve `$repo->find()` to `User` at all.
    expect![[r#"`$u` `User`"#]].assert_eq(&out);
}

/// VF2B (inlay): the return-type hint for a method whose declared return type is
/// refined by `@return T` (with `T = User`) shows the plain substituted `User`,
/// not the declared `object` and not the raw `T`. The native return type makes
/// the return-type hint fire; the resolved type then refines it. Exercises the
/// VF2B gate for a bracket-less substitution (the old `<`-gate would drop it).
#[tokio::test]
async fn inlay_shows_plain_substituted_return_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_inlay_hints(
            r#"<?php
/** @template T */
class BaseRepo {
    /** @return T */
    public function find(): object {}
}
/** @extends BaseRepo<User> */
class UserRepo extends BaseRepo {}
class User {}

$repo = new UserRepo();
$x = $repo->find();
"#,
        )
        .await;
    assert!(
        out.contains(": User"),
        "expected inlay hint `: User` from plain template substitution, got: {out}"
    );
    assert!(
        !out.contains(": T"),
        "must not surface the raw template name `T`, got: {out}"
    );
}

/// VF16: a `@phpstan-template T of Base` alias (which mir 0.30 does not parse
/// into a structured `bound_ty`) still renders its recovered string bound in the
/// hover, instead of dropping it.
#[tokio::test]
async fn hover_alias_template_decl_shows_bound() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
class Base {}
/**
 * @phpstan-template T of Ba$0se
 */
class Collection {}
"#,
        )
        .await;
    expect![[r#"`@template T of Base`"#]].assert_eq(&out);
}

/// VF16b: a `@phpstan-template`/`@psalm-template` alias with a fully-qualified
/// bound is now parsed into a structured `bound_ty` and rendered through the
/// import-aware short-name path, so `\App\Base` shortens to `Base` — consistent
/// with mir-parsed `@template` bounds rather than leaking the raw FQCN.
#[tokio::test]
async fn hover_alias_template_decl_shortens_fq_bound() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
namespace App;
class Base {}
/**
 * @phpstan-template T of \App\Ba$0se
 */
class Collection {}
"#,
        )
        .await;
    expect![[r#"`@template T of Base`"#]].assert_eq(&out);
}

/// Regression: hover on a plain (non-generic) variable is byte-identical to the
/// legacy path — the resolved type equals the legacy `TypeMap` value, so the
/// override does not fire and the existing rendering is used.
#[tokio::test]
async fn hover_plain_variable_unchanged() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
class User {}
$u = new User();
$u$0;
"#,
        )
        .await;
    expect![[r#"`$u` `User`"#]].assert_eq(&out);
}

/// Carryover-1 regression (hover): a bare scalar literal assignment (`$x = 1;`)
/// must NOT get a resolved-type override. mir may resolve `$x` to the literal
/// type `1` (`TLiteralInt`), which is NOT generic-relevant, so the override gate
/// (`is_generic_relevant`) keeps it off and hover stays on the legacy path
/// (which produces no `$x`-type hover for an untyped scalar local).
#[tokio::test]
async fn hover_scalar_int_literal_not_overridden() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
$x = 1;
$x$0;
"#,
        )
        .await;
    // Must not surface the literal/scalar resolved type (`1` / `int`); the legacy
    // path tracks no type for a bare scalar local, so there is no hover.
    assert!(
        !out.contains('1') && !out.contains("int"),
        "bare scalar `$x = 1` must not get a resolved-type override, got: {out}"
    );
    expect![["<no hover>"]].assert_eq(&out);
}

/// Carryover-1 regression (hover): a bare string literal (`$s = "str";`) is
/// likewise NOT generic-relevant and must not get a resolved-type override.
#[tokio::test]
async fn hover_scalar_string_literal_not_overridden() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
$s = "str";
$s$0;
"#,
        )
        .await;
    assert!(
        !out.contains("str") && !out.contains("string"),
        "bare scalar `$s = \"str\"` must not get a resolved-type override, got: {out}"
    );
    expect![["<no hover>"]].assert_eq(&out);
}

/// Carryover-1 regression (inlay): a bare scalar local (`$x = 1;`) must NOT get a
/// resolved-type inlay hint. The resolved literal/scalar type is not
/// generic-relevant, so `generic_hint_at` returns `None` and the legacy path (no
/// inlay for an untyped scalar local) is preserved.
#[tokio::test]
async fn inlay_scalar_literal_not_overridden() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_inlay_hints(
            r#"<?php
$x = 1;
$s = "str";
"#,
        )
        .await;
    assert!(
        !out.contains(": int") && !out.contains(": 1") && !out.contains(": string"),
        "bare scalar locals must not get a resolved-type inlay, got: {out}"
    );
}

/// VF10: hover on a `@template-extends Base<User>` line renders the EXTENDS
/// declaration, not a template decl. `@template-extends` starts with `@template`
/// but is an inheritance tag; classification must route it to the extends branch.
#[tokio::test]
async fn hover_template_extends_shows_extends_decl() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
/** @template T */
class Base {}
class User {}
/**
 * @template-extends Ba$0se<User>
 */
class Repo extends Base {}
"#,
        )
        .await;
    expect![[r#"`@extends Base<User>`"#]].assert_eq(&out);
}

/// VF10: hover on a `@template-implements Iter<User>` line renders the IMPLEMENTS
/// declaration, not a template decl.
#[tokio::test]
async fn hover_template_implements_shows_implements_decl() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
/** @template T */
interface Iter {}
class User {}
/**
 * @template-implements It$0er<User>
 */
class Repo implements Iter {}
"#,
        )
        .await;
    expect![[r#"`@implements Iter<User>`"#]].assert_eq(&out);
}
