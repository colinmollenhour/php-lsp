//! Add return type code action transformation tests.
//! Tests verify that inferred return types are correctly added to functions.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn add_return_type_infers_int() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function $0getCount$0() {
    return 42;
}
"#,
            "Add return type `: int`",
        )
        .await;
    expect!["<action not found: Add return type `: int`>"].assert_eq(&out);
}

#[tokio::test]
async fn add_return_type_void_when_no_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function $0doSomething$0() {
    echo "hello";
}
"#,
            "Add return type `: void`",
        )
        .await;
    expect![[r#"
        <?php
        function doSomething(): void {
            echo "hello";
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn add_return_type_infers_string() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function $0getName$0() {
    return "Alice";
}
"#,
            "Add return type `: string`",
        )
        .await;
    expect!["<action not found: Add return type `: string`>"].assert_eq(&out);
}
