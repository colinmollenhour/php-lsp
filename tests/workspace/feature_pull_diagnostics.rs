use super::*;
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

    // Extract the result from the JSON-RPC response envelope
    let result = &resp["result"];
    let items = &result["items"];
    assert!(
        !items.as_array().unwrap_or(&vec![]).is_empty(),
        "should have parse error"
    );
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

    let result = &resp["result"];
    let items = &result["items"];
    let array = items.as_array().expect("items should be an array");

    // Should have at least one diagnostic for undefined_function
    assert!(
        !array.is_empty(),
        "should have semantic error for undefined function"
    );
    let has_undefined = array.iter().any(|d| {
        d.get("message")
            .and_then(|m| m.as_str())
            .map(|s| s.contains("undefined") || s.contains("not defined"))
            .unwrap_or(false)
    });
    assert!(has_undefined, "should contain undefined function error");
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

    let result = &resp["result"];
    let items = &result["items"];
    assert!(
        items.as_array().map(|a| a.is_empty()).unwrap_or(true),
        "should have no diagnostics for valid code"
    );
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
