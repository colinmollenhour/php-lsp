//! Comprehensive hover coverage.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn hover_backed_enum_case_in_match_arm() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
enum Priority: int { case Low = 1; case High = 2; }
match ($p) {
    Priority::H$0igh => echo 'urgent',
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        case Priority::High = 2
        ```"#]]
    .assert_eq(&v);
}

/// Confirm that static method hover in match arm still works (regression check).
#[tokio::test]
async fn hover_backed_enum_shows_backing_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
enum Stat$0us: string { case Active = 'active'; }
"#,
        )
        .await;
    expect![[r#"
        ```php
        enum Status: string
        ```"#]]
    .assert_eq(&v);
}

/// Backed int enum.
#[tokio::test]
async fn hover_class_constant() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Config {
    const VERSI$0ON = 42;
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        const int VERSION = 42
        ```"#]]
    .assert_eq(&v);
}

/// A function with a nullable param type `?T` must render the `?` in hover so
/// callers can see the type is optional. Cursor is on the function name.
#[tokio::test]
async fn hover_enum_case_declaration() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
enum Status { case Acti$0ve; case Inactive; }
"#,
        )
        .await;
    expect![[r#"
        ```php
        case Status::Active
        ```"#]]
    .assert_eq(&v);
}

/// Hovering on a class constant must show the constant with its inferred or
/// declared type. An unimplemented constant-hover returns `<no hover>`.
#[tokio::test]
async fn hover_function_with_signature() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(r#"<?php function gr$0eet(string $name, int $count = 1): string {}"#)
        .await;
    expect![[r#"
        ```php
        function greet(string $name, int $count = 1): string
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_nullable_param_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
function sho$0w(?string $label): void {}
"#,
        )
        .await;
    expect![[r#"
        ```php
        function show(?string $label): void
        ```"#]]
    .assert_eq(&v);
}

/// Hovering on a trait identifier must render as `trait Name`, not `class`.
#[tokio::test]
async fn hover_property_access() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class User {
    public string $name = '';
}
$u = new User();
echo $u->na$0me;
"#,
        )
        .await;
    expect![[r#"
        ```php
        (property) public User::$name: string
        ```"#]]
    .assert_eq(&v);
}

/// Hovering on an enum *case* (not the enum name) should return the qualified
/// case label. If the server only indexes enum names but not individual cases
/// this will produce `<no hover>` — that is the bug to fix.
#[tokio::test]
async fn hover_static_property() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Config {
    public static string $version = '1.0';
}
Config::$ver$0sion;
"#,
        )
        .await;
    expect![[r#"
        ```php
        (property) public static Config::$version: string
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_template_at_call_site_shows_literal_t() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
/** @template T @param T $x @return T */
function identity($x) { return $x; }
$myString = 'hello';
// Hovering on the return value assignment
$result = identi$0ty($myString);
"#,
        )
        .await;
    expect![[r#"
        ```php
        function identity($x)
        ```

        ---

        **@template** `T`"#]]
    .assert_eq(&v);
}
#[tokio::test]
async fn hover_template_param_type_in_signature() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
/** @template T @param T $v @return T */
function box($v) { }
$result = box$0('hello');
"#,
        )
        .await;
    expect![[r#"
        ```php
        function box($v)
        ```

        ---

        **@template** `T`"#]]
    .assert_eq(&v);
}

/// At a call site, template T is shown literally (not substituted to string).
/// NOTE: Full template substitution (T → string) requires call-site argument
/// inference in type_map.rs, which is a larger architectural change deferred
/// to a future iteration. This test documents the current limitation.
#[tokio::test]
async fn hover_union_type_property() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Config {
    public string|int $setting = '';
}
$c = new Config();
echo $c->se$0tting;
"#,
        )
        .await;
    expect![[r#"
        ```php
        (property) public Config::$setting: string|int
        ```"#]]
    .assert_eq(&v);
}
