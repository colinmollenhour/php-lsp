//! Extract variable code action transformation tests.
//! Tests verify that expressions are extracted into new variables.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn extract_variable_from_expression() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_edit(
            r#"<?php
function calc() {
    echo $01 + 2$0;
}
"#,
            "Extract variable",
        )
        .await;
    expect![[r#"
        // main.php
        2:0-2:0 → "    $extracted = 1 + 2;\n"
        2:9-2:14 → "$extracted""#]]
    .assert_eq(&out);
}
