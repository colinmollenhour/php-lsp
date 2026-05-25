//! Implement interface code action transformation tests.
//! Tests verify that method stubs are correctly generated for unimplemented interfaces.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn implement_single_interface_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
interface Logger { public function log(string $msg): void; }
class $0App$0 implements Logger {}
"#,
            "Implement missing method",
        )
        .await;
    expect![[r#"
        <?php
        interface Logger { public function log(string $msg): void; }
        class App implements Logger {
            public function log(string $msg): void
            {
                throw new \RuntimeException('Not implemented');
            }

        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn implement_multiple_interface_methods() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
interface Handler {
    public function process(string $input): string;
    public function validate(): bool;
}
class $0Processor$0 implements Handler {}
"#,
            "Implement 2 missing methods",
        )
        .await;
    expect![[r#"
        <?php
        interface Handler {
            public function process(string $input): string;
            public function validate(): bool;
        }
        class Processor implements Handler {
            public function process(string $input): string
            {
                throw new \RuntimeException('Not implemented');
            }

            public function validate(): bool
            {
                throw new \RuntimeException('Not implemented');
            }

        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn implement_interface_with_return_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
interface Repository {
    public function find(int $id): ?object;
}
class $0UserRepository$0 implements Repository {}
"#,
            "Implement missing method",
        )
        .await;
    expect![[r#"
        <?php
        interface Repository {
            public function find(int $id): ?object;
        }
        class UserRepository implements Repository {
            public function find(int $id): ?object
            {
                throw new \RuntimeException('Not implemented');
            }

        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn implement_interface_with_multiple_parameters() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
interface Factory {
    public function create(string $name, array $config, int $version = 1): object;
}
class $0DefaultFactory$0 implements Factory {}
"#,
            "Implement missing method",
        )
        .await;
    expect![[r#"
        <?php
        interface Factory {
            public function create(string $name, array $config, int $version = 1): object;
        }
        class DefaultFactory implements Factory {
            public function create(string $name, array $config, int $version = 1): object
            {
                throw new \RuntimeException('Not implemented');
            }

        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn implement_no_action_when_already_implemented() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
interface Logger { public function log(): void; }
class $0ConsoleLogger$0 implements Logger {
    public function log(): void { }
}
"#,
            "Implement missing method",
        )
        .await;
    expect!["<action not found: Implement missing method>"].assert_eq(&out);
}
