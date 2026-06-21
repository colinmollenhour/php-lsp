use super::*;
use expect_test::expect;
use serde_json::json;

#[tokio::test]
async fn pull_diagnostics_returns_parse_errors() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
class {
"#,
    )
    .await;

    let resp = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri.to_string()}
            }),
        )
        .await;

    let out = render_pull_diagnostics(&resp);
    expect![[r#"
        1:6-1:7 [1] ?: expected class name, found '{'
        2:0-2:1 [1] ?: expected '}', found end of file"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn pull_diagnostics_includes_semantic_errors() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
function foo() {
    echo undefined_function();
}
"#,
    )
    .await;

    let resp = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri.to_string()}
            }),
        )
        .await;

    expect!["2:9-2:29 [1] UndefinedFunction: Function undefined_function() is not defined"]
        .assert_eq(&render_pull_diagnostics(&resp));
}

#[tokio::test]
async fn pull_diagnostics_empty_on_valid_code() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
function foo(): int {
    return 42;
}
"#,
    )
    .await;

    let resp = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri.to_string()}
            }),
        )
        .await;

    expect!["<empty>"].assert_eq(&render_pull_diagnostics(&resp));
}

#[tokio::test]
async fn pull_diagnostics_result_id_stable() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
$x = 1;
"#,
    )
    .await;

    // Request diagnostics twice
    let resp1 = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri.to_string()}
            }),
        )
        .await;

    let resp2 = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri.to_string()}
            }),
        )
        .await;

    let id1 = resp1["result"]["resultId"].clone();
    let id2 = resp2["result"]["resultId"].clone();

    // Same content should produce same result_id for caching
    assert_eq!(id1, id2, "result_id should be stable for same content");
}
