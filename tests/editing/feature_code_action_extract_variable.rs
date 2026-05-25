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
