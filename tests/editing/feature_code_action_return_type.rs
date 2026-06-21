//! Add return type code action transformation tests.
//! Tests verify that inferred return types are correctly added to functions and methods.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn void_for_function_with_no_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function $0greet$0() {}
"#,
            "Add return type `: void`",
        )
        .await;
    expect![[r#"
        <?php
        function greet(): void {}
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn mixed_for_function_with_value_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function $0getId$0() { return 42; }
"#,
            "Add return type `: mixed`",
        )
        .await;
    expect![[r#"
        <?php
        function getId(): mixed { return 42; }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn no_action_when_return_type_exists() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
function $0getId$0(): int { return 42; }
"#,
        )
        .await;
    expect![[r#"
        refactor         Generate PHPDoc
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn no_action_when_cursor_not_on_function() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
function greet() {}
$x = $0 "hello";
"#,
        )
        .await;
    expect!["<no actions>"].assert_eq(&out);
}

#[tokio::test]
async fn void_for_method_with_no_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Foo {
    public function $0bar$0() {}
}
"#,
            "Add return type `: void`",
        )
        .await;
    expect![[r#"
        <?php
        class Foo {
            public function bar(): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn mixed_for_method_with_value_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Foo {
    public function $0getId$0() { return $this->id; }
}
"#,
            "Add return type `: mixed`",
        )
        .await;
    expect![[r#"
        <?php
        class Foo {
            public function getId(): mixed { return $this->id; }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn skips_constructor() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Foo {
    public function $0__construct$0() {}
}
"#,
        )
        .await;
    expect![[r#"
        refactor         Generate PHPDoc
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn void_for_function_returning_void_explicitly() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function $0run$0() { return; }
"#,
            "Add return type `: void`",
        )
        .await;
    expect![[r#"
        <?php
        function run(): void { return; }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn mixed_for_if_return_in_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Foo {
    public function $0get$0() { if (true) { return 1; } }
}
"#,
            "Add return type `: mixed`",
        )
        .await;
    expect![[r#"
        <?php
        class Foo {
            public function get(): mixed { if (true) { return 1; } }
        }
    "#]]
    .assert_eq(&out);
}
