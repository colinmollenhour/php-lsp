//! Inline variable code action transformation tests.
//! Tests verify that variables are replaced with their assigned expressions.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn inline_variable_simple_expression() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$greeting = "Hello";
echo $0$greeting$0;
"#,
            "Inline variable '$greeting'",
        )
        .await;
    expect![[r#"
        <?php
        echo "Hello";
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inline_variable_numeric_expression() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$count = 42;
$total = $0$count$0 * 2;
"#,
            "Inline variable '$count'",
        )
        .await;
    expect![[r#"
        <?php
        $total = 42 * 2;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inline_variable_removes_assignment() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$x = 5;
echo $0$x$0;
echo $x;
"#,
            "Inline variable '$x'",
        )
        .await;
    expect![[r#"
        <?php
        echo 5;
        echo 5;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inline_variable_function_call() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$name = getName();
echo "Hello " . $0$name$0;
"#,
            "Inline variable '$name'",
        )
        .await;
    expect![[r#"
        <?php
        echo "Hello " . getName();
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inline_variable_array_expression() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$items = [1, 2, 3];
foreach ($0$items$0 as $item) {
    echo $item;
}
"#,
            "Inline variable '$items'",
        )
        .await;
    expect![[r#"
        <?php
        foreach ([1, 2, 3] as $item) {
            echo $item;
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inline_variable_with_complex_expression() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$result = $this->getValue() + 10 * 2;
echo $0$result$0;
"#,
            "Inline variable '$result'",
        )
        .await;
    expect![[r#"
        <?php
        echo $this->getValue() + 10 * 2;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inline_variable_multiple_uses_same_line() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$x = 5;
echo $0$x$0 + $x;
"#,
            "Inline variable '$x'",
        )
        .await;
    // Both uses should be replaced
    expect![[r#"
        <?php
        echo 5 + 5;
    "#]]
    .assert_eq(&out);
}
