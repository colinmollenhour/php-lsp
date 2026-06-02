//! WP3 generic-aware completion coverage: member completion through PHPStan-style
//! generics (`@template` / `@return T` / `@extends Base<User>`), plus a
//! non-generic regression golden.
//!
//! All generic behaviour is gated behind mir's resolved-symbol cache
//! (`resolved_type_at`) + the substitution queries. When the resolved type is
//! absent the legacy short-name path runs unchanged, which the regression test
//! pins. `open_fixture` waits for the diagnostics pass, so the resolved-symbol
//! cache is populated by the time completion runs.

use super::*;

use expect_test::expect;

/// A method whose docblock `@return T` resolves, in completion, to the receiver's
/// element type. `$c` is `Collection<User>`, so `$c->current()` shows `User` in
/// its detail and `$c->get()` likewise.
#[tokio::test]
async fn generic_member_return_shows_element_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion_ordered(
            r#"<?php
/**
 * @template T
 */
class Collection {
    /** @return T */
    public function current() {}
    /** @return T */
    public function get(int $i) {}
    /** @return T */
    public function first() {}
}
class User {
    public function name(): string {}
}

/** @var Collection<User> $c */
$c = new Collection();
$c->$0
"#,
        )
        .await;
    expect![[r#"
        Method      current | User
        Method      first | User
        Method      get | User"#]]
    .assert_eq(&out);
}

/// Chained completion: `$c->first()->|` lists the element type `User`'s members
/// because mir already substitutes the complete chain to `User`.
#[tokio::test]
async fn chained_generic_call_lists_element_members() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion(
            r#"<?php
/**
 * @template T
 */
class Collection {
    /** @return T */
    public function first() {}
}
class User {
    public function greet(): string {}
    public string $email;
}

/** @var Collection<User> $c */
$c = new Collection();
$c->first()->$0
"#,
        )
        .await;
    expect![[r#"
        Property    $email
        Method      greet"#]]
    .assert_eq(&out);
}

/// Inheritance: `class UserRepo extends BaseRepo` with `@extends BaseRepo<User>`
/// where `BaseRepo` has `@template T` and a method `@return T` → that inherited
/// method resolves to `User` in completion.
#[tokio::test]
async fn inherited_extends_binding_resolves_template() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion_ordered(
            r#"<?php
/**
 * @template T
 */
class BaseRepo {
    /** @return T */
    public function find() {}
}
/**
 * @extends BaseRepo<User>
 */
class UserRepo extends BaseRepo {
}
class User {}

$repo = new UserRepo();
$repo->$0
"#,
        )
        .await;
    expect![[r#"Method      find | User"#]].assert_eq(&out);
}

/// Regression: a non-generic receiver's member completion is byte-identical to
/// today (no generics in play → legacy short-name path).
#[tokio::test]
async fn non_generic_member_completion_unchanged() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion(
            r#"<?php
class Service {
    public function run(): void {}
    public string $name;
}
$s = new Service();
$s->$0
"#,
        )
        .await;
    expect![[r#"
        Property    $name
        Method      run"#]]
    .assert_eq(&out);
}

/// VF4(d): a GENUINE two-file fixture — `BaseRepo` (with `@template T` and a
/// `@return T` method) lives in one file, `UserRepo extends BaseRepo` with
/// `@extends BaseRepo<User>` in another. The inherited method resolves to `User`
/// across files (the single-file `inherited_extends_binding_resolves_template`
/// test could not exercise the cross-file `inherited_template_bindings` walk).
#[tokio::test]
async fn cross_file_inherited_extends_binding_resolves_template() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion_ordered(
            r#"//- /base.php
<?php
/**
 * @template T
 */
class BaseRepo {
    /** @return T */
    public function find() {}
}
//- /user.php
<?php
class User {}
//- /repo.php
<?php
/**
 * @extends BaseRepo<User>
 */
class UserRepo extends BaseRepo {
}

$repo = new UserRepo();
$repo->$0
"#,
        )
        .await;
    expect![[r#"Method      find | User"#]].assert_eq(&out);
}

/// VF4(a): an arity mismatch (`@var Collection $c` annotated as
/// `Collection<User, Extra>` — two args for a single `@template T`) must fall
/// back gracefully: `class_template_params.zip(type_params)` silently truncates,
/// binding only `T = User`, and there is no panic.
#[tokio::test]
async fn arity_mismatch_falls_back_without_panic() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let opened = s
        .open_fixture(
            r#"<?php
/** @template T */
class Collection {
    /** @return T */
    public function current() {}
}
class User { public function name(): string {} }
class Extra {}

/** @var Collection<User, Extra> $c */
$c = new Collection();
$c->$0
"#,
        )
        .await;
    let c = opened.cursor().clone();
    let resp = s.completion(&c.path, c.line, c.character).await;
    assert!(
        resp.get("error").filter(|e| !e.is_null()).is_none(),
        "completion on an arity-mismatched receiver returned an error: {resp:?}"
    );
    let out = render_completion(&resp);
    // The excess type arg is truncated; `T` still binds to `User`.
    assert!(
        out.contains("Method      current"),
        "expected `current` to be offered, got:\n{out}"
    );
}

/// VF4(b): a union generic receiver (`Collection<User>|Collection<Order>`) merges
/// the member set and surfaces both substituted return types as a union detail
/// (VF6), rather than dropping all but the first constituent.
#[tokio::test]
async fn union_generic_receiver_merges_substituted_details() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion_ordered(
            r#"<?php
/** @template T */
class Collection {
    /** @return T */
    public function current() {}
}
class User {}
class Order {}

/** @var Collection<User>|Collection<Order> $c */
$c = something();
$c->$0
"#,
        )
        .await;
    // Both constituents contribute `current(): T`; the details merge into a union.
    assert!(
        out.contains("current | User|Order") || out.contains("current | Order|User"),
        "expected union-merged detail `User|Order`, got:\n{out}"
    );
}

/// VF4(b): a nullable generic receiver (`?Collection<User>`) drops the `null`
/// constituent and resolves members against `Collection<User>`.
#[tokio::test]
async fn nullable_generic_receiver_resolves_core_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion_ordered(
            r#"<?php
/** @template T */
class Collection {
    /** @return T */
    public function current() {}
}
class User {}

/** @var ?Collection<User> $c */
$c = maybe();
$c->$0
"#,
        )
        .await;
    expect![[r#"Method      current | User"#]].assert_eq(&out);
}

/// VF4(c): a `$this` receiver inside a generic class. `$this->current()` resolves
/// `@return T` against the class's own template binding when one is in scope.
/// Asserts no panic and that the self member is offered.
#[tokio::test]
async fn this_receiver_in_generic_class() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let opened = s
        .open_fixture(
            r#"<?php
/** @template T */
class Collection {
    /** @return T */
    public function current() {}
    public function run(): void {
        $this->$0
    }
}
"#,
        )
        .await;
    let c = opened.cursor().clone();
    let resp = s.completion(&c.path, c.line, c.character).await;
    assert!(
        resp.get("error").filter(|e| !e.is_null()).is_none(),
        "completion on a `$this` receiver returned an error: {resp:?}"
    );
    let out = render_completion(&resp);
    assert!(
        out.contains("Method      current"),
        "expected `$this->current` to be offered, got:\n{out}"
    );
    assert!(
        out.contains("Method      run"),
        "expected `$this->run` to be offered, got:\n{out}"
    );
}

/// VF5: a method declaring its OWN `@template T` that shadows the class's
/// `@template T` must NOT have its method-local `T` replaced by the class
/// binding. `transform()` returns its own `T`, so its detail must not be
/// substituted to the class's element type (`User`); only `current()` (which
/// returns the class `T`) resolves to `User`.
#[tokio::test]
async fn method_local_template_shadows_class_binding() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion_ordered(
            r#"<?php
/** @template T */
class Collection {
    /** @return T */
    public function current() {}
    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function transform($value) {}
}
class User {}

/** @var Collection<User> $c */
$c = new Collection();
$c->$0
"#,
        )
        .await;
    // `current` resolves the class `T` to `User`; `transform`'s method-local `T`
    // is shadowed, so it carries no `| User` detail.
    expect![[r#"
        Method      current | User
        Method      transform"#]]
    .assert_eq(&out);
}

/// VF14: a non-generic call-chain completion golden (`$svc->factory()->`). The
/// legacy path could never resolve a chain receiver; routing it through mir adds
/// capability. This pins the contents/order so future drift is caught.
#[tokio::test]
async fn non_generic_call_chain_completion() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion_ordered(
            r#"<?php
class Widget {
    public function render(): string {}
    public string $id;
}
class Factory {
    public function factory(): Widget {}
}
$svc = new Factory();
$svc->factory()->$0
"#,
        )
        .await;
    expect![[r#"
        Property    $id
        Method      render"#]]
    .assert_eq(&out);
}

/// A `mixed` receiver must fall back cleanly (no panic): the generic path maps
/// `mixed` to no base class and returns `None`, so completion degrades to the
/// legacy path rather than panicking. The key guarantee is "no panic + a valid
/// response"; we assert membership rather than a brittle full snapshot.
#[tokio::test]
async fn mixed_receiver_falls_back_without_panic() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let opened = s
        .open_fixture(
            r#"<?php
class Box {
    public function consume(mixed $x): void {
        $x->$0
    }
}
"#,
        )
        .await;
    let c = opened.cursor().clone();
    let resp = s.completion(&c.path, c.line, c.character).await;
    // No error in the response and the request did not panic.
    assert!(
        resp.get("error").filter(|e| !e.is_null()).is_none(),
        "completion on a mixed receiver returned an error: {resp:?}"
    );
    let out = render_completion(&resp);
    // The current-doc method `consume` is offered via the legacy fallback; the
    // generic path correctly declined (it never sets a `| <type>` detail here).
    assert!(
        out.contains("Method      consume"),
        "expected legacy fallback to list `consume`, got:\n{out}"
    );
    assert!(
        !out.contains(" | "),
        "mixed receiver must not produce generic-substituted detail, got:\n{out}"
    );
}

/// E3 (the headline engine enhancement) — CROSS-FILE UNANNOTATED generic
/// member completion. A generic `Box<T>` with a promoted
/// `__construct(public T $value)` and an *unannotated* `get()` (body
/// `return $this->value;`, NO `@return`) is defined in `box.php`; the element
/// type `User` lives in `user.php`; the usage is in `main.php`. mir 0.31
/// infers `new Box(new User())` as `Box<User>` (constructor-arg class-template
/// inference) and resolves the unannotated cross-file `get()` to the element
/// type `User`. Member completion on the stored result therefore lists
/// `User`'s members (`$email`, `name()`).
#[tokio::test]
async fn cross_file_unannotated_generic_member_completion_lists_element_members() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_completion(
            r#"//- /box.php
<?php
/** @template T */
class Box {
    public function __construct(public T $value) {}
    public function get() { return $this->value; }
}
//- /user.php
<?php
class User {
    public function name(): string {}
    public string $email;
}
//- /main.php
<?php
$u = (new Box(new User()))->get();
$u->$0
"#,
        )
        .await;
    assert!(
        out.contains("Property    $email"),
        "expected element type `User`'s property `$email` in member completion, got:\n{out}"
    );
    assert!(
        out.contains("Method      name"),
        "expected element type `User`'s method `name` in member completion, got:\n{out}"
    );
}

/// E3 (cross-file, hover): the stored result of the unannotated cross-file
/// `get()` hovers as the element type. With an object element type (`User`)
/// the resolved type is generic-relevant, so the resolved-type override fires
/// and hover renders `User` (the legacy path could never resolve a
/// `(new Box(new User()))->get()` chain across files).
#[tokio::test]
async fn cross_file_unannotated_generic_return_hover_shows_element_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"//- /box.php
<?php
/** @template T */
class Box {
    public function __construct(public T $value) {}
    public function get() { return $this->value; }
}
//- /user.php
<?php
class User {
    public function name(): string {}
    public string $email;
}
//- /main.php
<?php
$u = (new Box(new User()))->get();
$u$0;
"#,
        )
        .await;
    expect![[r#"`$u` `User`"#]].assert_eq(&out);
}

/// E3 (constructor-arg class-template inference + scalar-literal widening):
/// `new Box(5)` infers `Box<int>` (NOT `Box<5>`). The element type surfaces in
/// hover on the `new` result. This is the cross-file companion to the
/// resolved-type-layer `int` assertion in
/// `document_store::tests::cross_file_unannotated_generic_return_resolves_int`
/// (where a bare scalar element type is intentionally gated out of the hover
/// override by `is_generic_relevant`, but `Box<int>` itself is relevant).
#[tokio::test]
async fn cross_file_new_infers_generic_type_param_from_constructor_arg() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"//- /box.php
<?php
/** @template T */
class Box {
    public function __construct(public T $value) {}
    public function get() { return $this->value; }
}
//- /main.php
<?php
$box = new Box(5);
$box$0;
"#,
        )
        .await;
    expect![[r#"`$box` `Box<int>`"#]].assert_eq(&out);
}
