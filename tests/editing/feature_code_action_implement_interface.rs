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
