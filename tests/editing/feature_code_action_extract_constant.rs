//! Extract constant code action transformation tests.
//! Tests verify that selected literals are extracted into named constants.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn extract_constant_string_in_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
class Greeter {
    public function greet(): string {
        return $0"Hello, World!"$0;
    }
}
"#,
            "Extract constant",
        )
        .await;
    expect![[r#"
        <?php
        class Greeter {
            private const HELLO_WORLD = "Hello, World!";
            public function greet(): string {
                return self::HELLO_WORLD;
            }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_integer_in_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
class Timer {
    public function delay(): void {
        sleep($042$0);
    }
}
"#,
            "Extract constant",
        )
        .await;
    expect![[r#"
        <?php
        class Timer {
            private const CONSTANT_42 = 42;
            public function delay(): void {
                sleep(self::CONSTANT_42);
            }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_float_in_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
class Calculator {
    public function ratio(): float {
        return $03.14$0;
    }
}
"#,
            "Extract constant",
        )
        .await;
    expect![[r#"
        <?php
        class Calculator {
            private const CONSTANT_3_14 = 3.14;
            public function ratio(): float {
                return self::CONSTANT_3_14;
            }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_at_file_scope() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
function getName() {
    return $0"app"$0;
}
"#,
            "Extract constant",
        )
        .await;
    expect![[r#"
        <?php
        const APP = "app";
        function getName() {
            return APP;
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_in_interface() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
interface Status {
    const ACTIVE = $0true$0;
}
"#,
            "Extract constant",
        )
        .await;
    expect!["<action not found: Extract constant>"].assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_in_trait() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
trait Logging {
    public function log(): void {
        $level = $0"info"$0;
    }
}
"#,
            "Extract constant",
        )
        .await;
    expect![[r#"
        <?php
        trait Logging {
            private const INFO = "info";
            public function log(): void {
                $level = self::INFO;
            }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_after_interface_at_file_scope() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
interface PaymentGateway {
    public function charge(): void;
}
$fee = $0250$0;
"#,
            "Extract constant",
        )
        .await;
    expect![[r#"
        <?php
        const CONSTANT_250 = 250;
        interface PaymentGateway {
            public function charge(): void;
        }
        $fee = CONSTANT_250;
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_non_literal_returns_no_action() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
$x = $0foo()$0;
"#,
            "Extract constant",
        )
        .await;
    expect!["<action not found: Extract constant>"].assert_eq(&out);
}

#[tokio::test]
async fn extract_constant_file_scope_inserts_before_use_statement() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_action_apply(
            r#"<?php
$x = $0"hello"$0;
use Foo\Bar;
"#,
            "Extract constant",
        )
        .await;
    expect![[r#"
        <?php
        const HELLO = "hello";
        $x = HELLO;
        use Foo\Bar;
    "#]]
    .assert_eq(&out);
}
