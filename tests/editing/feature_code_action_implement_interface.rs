//! Implement interface code action transformation tests.
//! Tests verify that method stubs are correctly generated for unimplemented interfaces.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn implement_single_interface_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_edit(
            r#"<?php
interface Logger { public function log(string $msg): void; }
class $0App$0 implements Logger {}
"#,
            "Implement missing method",
        )
        .await;
    expect![[r#"
        // main.php
        2:29-2:29 → "\n    public function log(string $msg): void\n    {\n        throw new \\RuntimeException('Not implemented');\n    }\n\n""#]]
        .assert_eq(&out);
}
