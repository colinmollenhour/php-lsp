//! Generate PHPDoc code action transformation tests.
//! Tests verify that PHPDoc blocks are correctly generated with @param and @return tags.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn phpdoc_function_with_params_and_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
function $0greet$0(string $name, int $age): string {
    return "Hello $name";
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect![[r#"
        <?php
        /**
         * @param string $name
         * @param int $age
         * @return string
         */
        function greet(string $name, int $age): string {
            return "Hello $name";
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn phpdoc_method_with_single_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
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
        <?php
        class Logger {
            /**
             * @param string $message
             */
            public function log(string $message) {
                echo $message;
            }
        }
    "#]]
    .assert_eq(&out);
}
