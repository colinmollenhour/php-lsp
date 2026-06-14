//! Extract variable code action transformation tests.
//! Tests verify that expressions are extracted into new variables.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn extract_variable_from_expression() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function calc() {
    echo $01 + 2$0;
}
"#,
            "Extract variable",
        )
        .await;
    expect![[r#"
        <?php
        function calc() {
            $extracted = 1 + 2;
            echo $extracted;
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_variable_function_call() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Service {
    public function run() {
        return $0strlen("test")$0;
    }
}
"#,
            "Extract variable",
        )
        .await;
    expect![[r#"
        <?php
        class Service {
            public function run() {
                $extracted = strlen("test");
                return $extracted;
            }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_variable_array_access() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$data = ["key" => "value"];
echo $0$data["key"]$0;
"#,
            "Extract variable",
        )
        .await;
    expect![[r#"
        <?php
        $data = ["key" => "value"];
        $extracted = $data["key"];
        echo $extracted;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_variable_method_call() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Handler {
    public function process() {
        $result = $0$this->getValue()$0 + 10;
    }
}
"#,
            "Extract variable",
        )
        .await;
    expect![[r#"
        <?php
        class Handler {
            public function process() {
                $extracted = $this->getValue();
                $result = $extracted + 10;
            }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_variable_preserves_indentation_nested() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Service {
    public function process() {
        if (true) {
            return $01 + 2$0;
        }
    }
}
"#,
            "Extract variable",
        )
        .await;
    // Should create $extracted at the correct indentation level
    expect![[r#"
        <?php
        class Service {
            public function process() {
                if (true) {
                    $extracted = 1 + 2;
                    return $extracted;
                }
            }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_variable_no_action_for_empty_selection() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$x = foo();$0
"#,
            "Extract variable",
        )
        .await;
    expect!["<action not found: Extract variable>"].assert_eq(&out);
}

#[tokio::test]
async fn extract_variable_no_action_for_plain_variable() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$x = $0$foo$0;
"#,
            "Extract variable",
        )
        .await;
    expect!["<action not found: Extract variable>"].assert_eq(&out);
}

#[tokio::test]
async fn extract_variable_multibyte_unicode_string() {
    // "é" is 2 UTF-8 bytes but 1 UTF-16 code unit; LSP positions are UTF-16.
    // Slicing by byte offset would either panic or return garbage.
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
function greet(): string {
    return $0"café"$0;
}
"#,
            "Extract variable",
        )
        .await;
    expect![[r#"
        <?php
        function greet(): string {
            $extracted = "café";
            return $extracted;
        }
    "#]]
    .assert_eq(&out);
}
