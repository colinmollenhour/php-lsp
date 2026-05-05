//! Document + workspace symbol coverage.

use super::*;

use expect_test::expect;
use serde_json::json;

#[tokio::test]
async fn document_symbols_outline() {
    let mut s = TestServer::new().await;
    let out = s
        .check_document_symbols(
            r#"<?php
class Greeter {
    public function hello(): string { return 'hi'; }
    public function bye(): void {}
}
function top_level(): void {}
"#,
        )
        .await;
    expect![[r#"
        Class Greeter @L1
          Method hello @L2
          Method bye @L3
        Function top_level @L5"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn document_symbols_nested_enum() {
    let mut s = TestServer::new().await;
    let out = s
        .check_document_symbols(
            r#"<?php
enum Status {
    case Active;
    case Inactive;
}
"#,
        )
        .await;
    expect![[r#"
        Enum Status @L1
          EnumMember Active @L2
          EnumMember Inactive @L3"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn document_symbols_interface() {
    let mut s = TestServer::new().await;
    let out = s
        .check_document_symbols(
            r#"<?php
interface Writable {
    public function write(): void;
}
"#,
        )
        .await;
    expect![[r#"
        Interface Writable @L1
          Method write @L2"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn workspace_symbols_finds_class_by_query() {
    let mut s = TestServer::new().await;
    let out = s
        .check_workspace_symbols(
            r#"<?php
class MagicRegistry {}
function abracadabra(): void {}
"#,
            "MagicReg",
        )
        .await;
    expect!["Class       MagicRegistry @ main.php:1"].assert_eq(&out);
}

/// Workspace symbol search must find `User` by short name even though the FQN
/// is `App\Model\User`.
#[tokio::test]
async fn workspace_symbol_finds_class_by_short_name() {
    let mut server = TestServer::with_fixture("psr4-mini").await;
    server.wait_for_index_ready().await;
    let out = server
        .check_workspace_symbols(
            r#"<?php
        // This file won't be used; we're searching the fixture
        "#,
            "User",
        )
        .await;
    expect!["Class       User @ src/Model/User.php:4"].assert_eq(&out);
}

// --- workspaceSymbol/resolve ---

#[tokio::test]
async fn symbol_resolve_fills_range_for_open_class() {
    let mut server = TestServer::new().await;
    server
        .open("resolve.php", "<?php\nclass Resolvable {}\n")
        .await;
    let uri = server.uri("resolve.php");

    let symbol = json!({
        "name": "Resolvable",
        "kind": 5,
        "location": { "uri": uri },
    });
    let resp = server.workspace_symbol_resolve(symbol).await;
    let out = render_resolved_workspace_symbol(&resp, &server.uri(""));
    expect!["Resolvable (Class) @ resolve.php:1:6-1:16"].assert_eq(&out);
}

#[tokio::test]
async fn symbol_resolve_fills_range_for_open_function() {
    let mut server = TestServer::new().await;
    server
        .open("resolve.php", "<?php\nfunction myFunc() {}\n")
        .await;
    let uri = server.uri("resolve.php");

    let symbol = json!({
        "name": "myFunc",
        "kind": 12,
        "location": { "uri": uri },
    });
    let resp = server.workspace_symbol_resolve(symbol).await;
    let out = render_resolved_workspace_symbol(&resp, &server.uri(""));
    expect!["myFunc (Function) @ resolve.php:1:9-1:15"].assert_eq(&out);
}

#[tokio::test]
async fn symbol_resolve_unchanged_for_closed_file() {
    let mut server = TestServer::new().await;

    let symbol = json!({
        "name": "ClosedClass",
        "kind": 5,
        "location": { "uri": "file:///nonexistent_closed.php" },
    });
    let resp = server.workspace_symbol_resolve(symbol).await;
    let out = render_resolved_workspace_symbol(&resp, "file:///");
    expect!["ClosedClass (Class) @ nonexistent_closed.php [uri-only]"].assert_eq(&out);
}

#[tokio::test]
async fn symbol_resolve_passthrough_for_already_resolved_location() {
    let mut server = TestServer::new().await;
    server
        .open("passthrough.php", "<?php\nfunction alreadyResolved() {}\n")
        .await;
    let uri = server.uri("passthrough.php");

    let symbol = json!({
        "name": "alreadyResolved",
        "kind": 12,
        "location": {
            "uri": uri,
            "range": {
                "start": { "line": 1, "character": 9 },
                "end":   { "line": 1, "character": 24 },
            },
        },
    });
    let resp = server.workspace_symbol_resolve(symbol).await;
    let out = render_resolved_workspace_symbol(&resp, &server.uri(""));
    expect!["alreadyResolved (Function) @ passthrough.php:1:9-1:24"].assert_eq(&out);
}

#[tokio::test]
async fn symbol_resolve_finds_first_occurrence_when_name_appears_multiple_times() {
    let mut server = TestServer::new().await;
    server
        .open(
            "multi.php",
            "<?php\nclass Duplicate {}\nfunction test() { $x = new Duplicate(); }\n",
        )
        .await;
    let uri = server.uri("multi.php");

    let symbol = json!({
        "name": "Duplicate",
        "kind": 5,
        "location": { "uri": uri },
    });
    let resp = server.workspace_symbol_resolve(symbol).await;
    let out = render_resolved_workspace_symbol(&resp, &server.uri(""));
    expect!["Duplicate (Class) @ multi.php:1:6-1:15"].assert_eq(&out);
}

#[tokio::test]
async fn symbol_resolve_symbol_at_line_zero() {
    let mut server = TestServer::new().await;
    server.open("line0.php", "<?php class AtStart {}\n").await;
    let uri = server.uri("line0.php");

    let symbol = json!({
        "name": "AtStart",
        "kind": 5,
        "location": { "uri": uri },
    });
    let resp = server.workspace_symbol_resolve(symbol).await;
    let out = render_resolved_workspace_symbol(&resp, &server.uri(""));
    expect!["AtStart (Class) @ line0.php:0:12-0:19"].assert_eq(&out);
}

#[tokio::test]
async fn symbol_resolve_nonexistent_symbol_in_source() {
    let mut server = TestServer::new().await;
    server
        .open("noexist.php", "<?php\nclass RealClass {}\n")
        .await;
    let uri = server.uri("noexist.php");

    let symbol = json!({
        "name": "NonExistentClass",
        "kind": 5,
        "location": { "uri": uri },
    });
    let resp = server.workspace_symbol_resolve(symbol).await;
    let out = render_resolved_workspace_symbol(&resp, &server.uri(""));
    expect!["NonExistentClass (Class) @ noexist.php [uri-only]"].assert_eq(&out);
}

#[tokio::test]
async fn symbol_resolve_is_idempotent() {
    let mut server = TestServer::new().await;
    server
        .open("idempotent.php", "<?php\nclass TestClass {}\n")
        .await;
    let uri = server.uri("idempotent.php");

    let symbol = json!({
        "name": "TestClass",
        "kind": 5,
        "location": { "uri": uri },
    });

    let resolved_once = server.workspace_symbol_resolve(symbol.clone()).await;
    let resolved_twice = server
        .workspace_symbol_resolve(resolved_once["result"].clone())
        .await;

    assert_eq!(
        resolved_once["result"], resolved_twice["result"],
        "calling resolve twice must return identical results (idempotent)"
    );
}
