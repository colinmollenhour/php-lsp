//! Generate PHPDoc code action transformation tests.
//! Tests verify that PHPDoc blocks are correctly generated with @param and @return tags.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn phpdoc_function_with_params_and_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_edit(
            r#"<?php
function $0greet$0(string $name, int $age): string {
    return "Hello $name";
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect![[r#"
        // main.php
        1:0-1:0 → "/**\n * @param string $name\n * @param int $age\n * @return string\n */\n""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn phpdoc_method_with_single_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_edit(
            r#"<?php
class Logger {
    public function $0log$0(string $message) {
        echo $message;
    }
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect![[r#"
        // main.php
        2:0-2:0 → "    /**\n     * @param string $message\n     */\n""#]]
    .assert_eq(&out);
}
