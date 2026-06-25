//! Code action: "Convert to arrow function" — converts a closure whose body is
//! a single `return` statement into a PHP 7.4+ arrow function expression.

use super::*;
use expect_test::expect;

// --- Offered ---

#[tokio::test]
async fn arrow_fn_offered_for_simple_closure() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
$fn = $0function() { return 42; }$0;
"#,
        )
        .await;
    assert!(
        out.contains("Convert to arrow function"),
        "expected action in: {out}"
    );
}

#[tokio::test]
async fn arrow_fn_offered_when_cursor_anywhere_in_closure() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    // Cursor in the middle of the body, not on `function` keyword.
    let out = s
        .check_code_actions(
            r#"<?php
$fn = function() { return $042; };
"#,
        )
        .await;
    assert!(
        out.contains("Convert to arrow function"),
        "expected action in: {out}"
    );
}

// --- Not offered ---

#[tokio::test]
async fn arrow_fn_not_offered_for_multi_statement_body() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
$fn = $0function($x) {
    $y = $x * 2;
    return $y;
}$0;
"#,
        )
        .await;
    assert!(
        !out.contains("Convert to arrow function"),
        "should not offer for multi-statement body, got: {out}"
    );
}

#[tokio::test]
async fn arrow_fn_not_offered_when_body_is_not_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
$fn = $0function($x) { echo $x; }$0;
"#,
        )
        .await;
    assert!(
        !out.contains("Convert to arrow function"),
        "should not offer when body is not return, got: {out}"
    );
}

#[tokio::test]
async fn arrow_fn_not_offered_for_by_ref_use_capture() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
$counter = 0;
$fn = $0function() use (&$counter) { return $counter; }$0;
"#,
        )
        .await;
    assert!(
        !out.contains("Convert to arrow function"),
        "should not offer for by-ref use capture, got: {out}"
    );
}

// --- Applied edits ---

#[tokio::test]
async fn arrow_fn_converts_no_param_closure() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$fn = $0function() { return 42; }$0;
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $fn = fn() => 42;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn arrow_fn_converts_closure_with_params() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$fn = $0function(int $x, int $y) { return $x + $y; }$0;
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $fn = fn(int $x, int $y) => $x + $y;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn arrow_fn_converts_closure_with_return_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$fn = $0function(string $s): string { return strtoupper($s); }$0;
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $fn = fn(string $s): string => strtoupper($s);
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn arrow_fn_converts_static_closure() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$fn = $0static function(int $n): int { return $n * 2; }$0;
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $fn = static fn(int $n): int => $n * 2;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn arrow_fn_drops_value_use_clause() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$base = 10;
$fn = $0function(int $x) use ($base) { return $x + $base; }$0;
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $base = 10;
        $fn = fn(int $x) => $x + $base;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn arrow_fn_converts_closure_inside_array_map() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$result = array_map($0function(int $n) { return $n * $n; }$0, $items);
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $result = array_map(fn(int $n) => $n * $n, $items);
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn arrow_fn_converts_innermost_nested_closure() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$outer = function(int $a) {
    return $0function(int $b) { return $a + $b; }$0;
};
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $outer = function(int $a) {
            return fn(int $b) => $a + $b;
        };
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn arrow_fn_with_nullable_return_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$fn = $0function(?string $s): ?string { return $s; }$0;
"#,
            "Convert to arrow function",
        )
        .await;
    expect![[r#"
        <?php
        $fn = fn(?string $s): ?string => $s;
    "#]]
    .assert_eq(&out);
}
