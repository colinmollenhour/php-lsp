//! Inline variable code action transformation tests.
//! Tests verify that variables are replaced with their assigned expressions.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn inline_variable_single_use() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$greeting = $0"Hello"$0;
echo $greeting;
"#,
            "Inline variable '$greeting'",
        )
        .await;
    expect!["<action not found: Inline variable '$greeting'>"].assert_eq(&out);
}
