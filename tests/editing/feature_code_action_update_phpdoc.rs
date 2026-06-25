//! "Update PHPDoc to match signature" code action.
//!
//! Triggered when a function/method already has a docblock whose @param/@return
//! section is out of sync with the actual signature.

use super::*;
use expect_test::expect;

// ── Availability ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_phpdoc_offered_when_param_added() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Service {
    /**
     * @param string $name
     */
    public function $0greet$0(string $name, int $times): void {}
}
"#,
        )
        .await;
    assert!(
        out.contains("Update PHPDoc to match signature"),
        "expected action in: {out}"
    );
}

#[tokio::test]
async fn update_phpdoc_offered_when_param_removed() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Service {
    /**
     * @param string $name
     * @param int $age
     */
    public function $0greet$0(string $name): void {}
}
"#,
        )
        .await;
    assert!(
        out.contains("Update PHPDoc to match signature"),
        "expected action in: {out}"
    );
}

#[tokio::test]
async fn update_phpdoc_offered_when_param_renamed() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Service {
    /**
     * @param string $oldName
     */
    public function $0greet$0(string $newName): void {}
}
"#,
        )
        .await;
    assert!(
        out.contains("Update PHPDoc to match signature"),
        "expected action in: {out}"
    );
}

#[tokio::test]
async fn update_phpdoc_offered_when_return_type_missing() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Service {
    /**
     * @param string $name
     */
    public function $0greet$0(string $name): string {}
}
"#,
        )
        .await;
    assert!(
        out.contains("Update PHPDoc to match signature"),
        "expected action in: {out}"
    );
}

#[tokio::test]
async fn update_phpdoc_not_offered_when_in_sync() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Service {
    /**
     * @param string $name
     * @return string
     */
    public function $0greet$0(string $name): string { return $name; }
}
"#,
        )
        .await;
    assert!(
        !out.contains("Update PHPDoc to match signature"),
        "should not offer when already in sync, got: {out}"
    );
}

#[tokio::test]
async fn update_phpdoc_not_offered_when_no_docblock() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Service {
    public function $0greet$0(string $name): void {}
}
"#,
        )
        .await;
    assert!(
        !out.contains("Update PHPDoc to match signature"),
        "should not offer when there is no docblock, got: {out}"
    );
}

#[tokio::test]
async fn update_phpdoc_not_offered_for_inherit_doc() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class Child extends Base {
    /** {@inheritDoc} */
    public function $0greet$0(string $name, int $extra): void {}
}
"#,
        )
        .await;
    assert!(
        !out.contains("Update PHPDoc to match signature"),
        "should not offer for @inheritDoc, got: {out}"
    );
}

// ── Applied edits ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_phpdoc_adds_missing_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Service {
    /**
     * @param string $name
     */
    public function $0greet$0(string $name, int $times): void {}
}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        class Service {
            /**
             * @param string $name
             * @param int $times
             */
            public function greet(string $name, int $times): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn update_phpdoc_removes_stale_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Service {
    /**
     * @param string $name
     * @param int $age
     */
    public function $0greet$0(string $name): void {}
}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        class Service {
            /**
             * @param string $name
             */
            public function greet(string $name): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn update_phpdoc_renames_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Service {
    /**
     * @param string $oldName
     */
    public function $0greet$0(string $newName): void {}
}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        class Service {
            /**
             * @param string $newName
             */
            public function greet(string $newName): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn update_phpdoc_adds_return_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Service {
    /**
     * @param string $name
     */
    public function $0greet$0(string $name): string { return $name; }
}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        class Service {
            /**
             * @param string $name
             * @return string
             */
            public function greet(string $name): string { return $name; }
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn update_phpdoc_preserves_description() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Greeter {
    /**
     * Say hello to someone.
     *
     * @param string $name
     */
    public function $0greet$0(string $name, int $times): void {}
}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        class Greeter {
            /**
             * Say hello to someone.
             *
             * @param string $name
             * @param int $times
             */
            public function greet(string $name, int $times): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn update_phpdoc_preserves_param_description() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Greeter {
    /**
     * @param string $name The user's name
     */
    public function $0greet$0(string $name, int $times): void {}
}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        class Greeter {
            /**
             * @param string $name The user's name
             * @param int $times
             */
            public function greet(string $name, int $times): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn update_phpdoc_preserves_throws() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class Service {
    /**
     * @param string $name
     * @throws RuntimeException
     */
    public function $0greet$0(string $name, int $times): void {}
}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        class Service {
            /**
             * @param string $name
             * @param int $times
             * @throws RuntimeException
             */
            public function greet(string $name, int $times): void {}
        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn update_phpdoc_free_function() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
/**
 * @param string $name
 */
function $0greet$0(string $name, int $times): void {}
"#,
            "Update PHPDoc to match signature",
        )
        .await;
    expect![[r#"
        <?php
        /**
         * @param string $name
         * @param int $times
         */
        function greet(string $name, int $times): void {}
    "#]]
    .assert_eq(&out);
}
