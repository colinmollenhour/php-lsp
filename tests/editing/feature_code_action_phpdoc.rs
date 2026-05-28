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

#[tokio::test]
async fn phpdoc_method_without_params() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Foo {
    public function $0bar$0(): void {}
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect![[r#"
        <?php
        class Foo {
            /**
             * @return void
             */
            public function bar(): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn phpdoc_trait_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
trait Logger {
    public function $0log$0(string $msg): void {}
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect![[r#"
        <?php
        trait Logger {
            /**
             * @param string $msg
             * @return void
             */
            public function log(string $msg): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn phpdoc_enum_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
enum Suit {
    case Hearts;
    public function $0label$0(int $pad): string { return ''; }
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect![[r#"
        <?php
        enum Suit {
            case Hearts;
            /**
             * @param int $pad
             * @return string
             */
            public function label(int $pad): string { return ''; }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn phpdoc_no_action_when_docblock_exists() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
/** Greets someone. */
function $0greet$0(string $name): string {}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect!["<action not found: Generate PHPDoc>"].assert_eq(&out);
}

#[tokio::test]
async fn phpdoc_no_action_when_cursor_not_on_function() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
$0class$0 Foo {
    public function bar(): void {}
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect!["<action not found: Generate PHPDoc>"].assert_eq(&out);
}

#[tokio::test]
async fn phpdoc_no_action_when_trait_method_has_docblock() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
trait Logger {
    /** Already documented. */
    public function $0log$0(string $msg): void {}
}
"#,
            "Generate PHPDoc",
        )
        .await;
    expect!["<action not found: Generate PHPDoc>"].assert_eq(&out);
}
