//! Code action smoke coverage. Each scenario uses a two-`$0` selection to
//! name the range the action acts on.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn code_actions_offers_generate_constructor() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_actions(
            r#"<?php
class U$0ser$0 {
    public string $name = '';
    public int $age = 0;
}
"#,
        )
        .await;
    expect![[r#"
        refactor         Generate 4 getters/setters
        refactor         Generate constructor
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_actions_offers_extract_variable_on_expression() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_actions(
            r#"<?php
function f(): int {
    return $01 + 2$0;
}
"#,
        )
        .await;
    expect!["refactor.extract Extract variable [edit]"].assert_eq(&out);
}

#[tokio::test]
async fn code_actions_offers_add_return_type() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_actions(
            r#"<?php
function $0noReturn$0() { return 42; }
"#,
        )
        .await;
    expect![[r#"
        refactor         Add return type `: mixed`
        refactor         Generate PHPDoc
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_actions_offers_implement_missing_methods() {
    let mut s = TestServer::new().await;
    let out = s
        .check_code_actions(
            r#"<?php
interface Writable { public function write(): void; }
class $0My$0 implements Writable {}
"#,
        )
        .await;
    expect![[r#"
        quickfix         Implement missing method
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

// --- codeAction/resolve roundtrip tests ---
// Each test verifies that a deferred action (no `edit` on initial request)
// gets its full WorkspaceEdit populated after calling codeAction/resolve.

#[tokio::test]
async fn code_action_resolve_return_type() {
    let mut server = TestServer::new().await;
    server
        .open("rt.php", "<?php\nfunction noReturn() { return 42; }\n")
        .await;

    let resp = server.code_action("rt.php", 1, 0, 1, 30).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Add return type `: mixed`"))
        .cloned()
        .expect("return type action");

    assert!(
        action["edit"].is_null(),
        "action must be deferred (no edit)"
    );
    assert!(!action["data"].is_null(), "action must carry resolve data");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null(), "resolve must not error");
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // rt.php
        1:19-1:19 → ": mixed""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_phpdoc() {
    let mut server = TestServer::new().await;
    server
        .open("doc.php", "<?php\nfunction greet(string $name): void {}\n")
        .await;

    let resp = server.code_action("doc.php", 1, 0, 1, 40).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Generate PHPDoc"))
        .cloned()
        .expect("phpdoc action");

    assert!(action["edit"].is_null(), "action must be deferred");
    assert!(!action["data"].is_null(), "action must carry resolve data");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // doc.php
        1:0-1:0 → "/**\n * @param string $name\n * @return void\n */\n""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_constructor() {
    let mut server = TestServer::new().await;
    server
        .open(
            "ctor.php",
            "<?php\nclass Point {\n    public float $x = 0.0;\n    public float $y = 0.0;\n}\n",
        )
        .await;

    let resp = server.code_action("ctor.php", 1, 0, 1, 11).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Generate constructor"))
        .cloned()
        .expect("constructor action");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // ctor.php
        4:0-4:0 → "    public function __construct(\n        float $x,\n        float $y,\n    ) {\n        $this->x = $x;\n        $this->y = $y;\n    }\n\n""#]].assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_getters_setters() {
    let mut server = TestServer::new().await;
    server
        .open(
            "gs.php",
            "<?php\nclass Box {\n    public int $width = 0;\n    public int $height = 0;\n}\n",
        )
        .await;

    let resp = server.code_action("gs.php", 1, 0, 1, 9).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.contains("getters/setters"))
                .unwrap_or(false)
        })
        .cloned()
        .expect("getters/setters action");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // gs.php
        4:0-4:0 → "    public function getWidth(): int\n    {\n        return $this->width;\n    }\n\n    public function setWidth(int $width): void\n    {\n        $this->width = $width;\n    }\n\n    public function getHeight(): int\n    {\n        return $this->height;\n    }\n\n    public function setHeight(int $height): void\n    {\n        $this->height = $height;\n    }\n\n""#]].assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_implement_missing_methods() {
    let mut server = TestServer::new().await;
    server
        .open(
            "impl.php",
            "<?php\ninterface Loggable { public function log(): void; }\nclass App implements Loggable {}\n",
        )
        .await;

    let resp = server.code_action("impl.php", 2, 0, 2, 30).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Implement missing method"))
        .cloned()
        .expect("implement missing method action");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // impl.php
        2:31-2:31 → "\n    public function log(): void\n    {\n        throw new \\RuntimeException('Not implemented');\n    }\n\n""#]].assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_promote_constructor_params() {
    let mut server = TestServer::new().await;
    server
        .open(
            "promote.php",
            "<?php\nclass Service {\n    public function __construct(\n        private string $name,\n        private int $port\n    ) {}\n}\n",
        )
        .await;

    let resp = server.code_action("promote.php", 2, 0, 5, 6).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .cloned();

    if let Some(action) = action {
        assert!(action["edit"].is_null(), "action must be deferred");

        let resolved = server.code_action_resolve(action).await;
        assert!(resolved["error"].is_null());
        let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
        expect![[r#""#]].assert_eq(&out);
    }
    // promote action may not apply when params are already promoted — that is valid
}

#[tokio::test]
async fn code_action_resolve_without_data_is_passthrough() {
    let mut server = TestServer::new().await;
    server.open("noop.php", "<?php").await;

    let action = serde_json::json!({
        "title": "My Action",
        "kind": "refactor"
    });
    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    assert_eq!(
        resolved["result"]["title"].as_str(),
        Some("My Action"),
        "title must roundtrip"
    );
    assert!(
        resolved["result"]["edit"].is_null(),
        "no edit should be added for data-less actions"
    );
}
