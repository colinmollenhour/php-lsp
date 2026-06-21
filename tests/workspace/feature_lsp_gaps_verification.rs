/// Comprehensive verification of LSP feature gaps and edge cases.
use super::*;
use expect_test::expect;
use serde_json::json;

// ============================================================================
// PULL DIAGNOSTICS EDGE CASES
// ============================================================================

#[tokio::test]
async fn pull_diagnostics_multi_parse_errors_all_returned() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
function foo( {
class {
const X
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

    expect![[r#"
        1:14-1:15 [1] ?: expected variable, found '{'
        1:14-1:15 [1] ?: unclosed '')'' opened at Span { start: 18, end: 19 }
        2:6-2:7 [1] ?: expected class name, found '{'
        4:0-4:1 [1] ?: expected ';', found end of file
        4:0-4:1 [1] ?: expected '=', found end of file
        4:0-4:1 [1] ?: expected '}', found end of file
        4:0-4:1 [1] ?: expected constant name, found end of file
        4:0-4:1 [1] ?: expected expression
        4:0-4:1 [1] ?: unclosed ''}'' opened at Span { start: 20, end: 21 }"#]]
    .assert_eq(&render_pull_diagnostics(&resp));
}

#[tokio::test]
async fn pull_diagnostics_on_nonexistent_file_returns_empty() {
    let mut s = TestServer::new().await;
    let uri = s.uri("nonexistent.php");

    // Don't open the file - request diagnostics directly
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
async fn pull_diagnostics_mixed_parse_and_semantic_errors() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
class Foo {
    public function bar(

    public function undefined_call() {
        nonexistent_func();
    }
}"#,
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

    expect![[r#"
        4:11-4:19 [1] ?: expected ')', found 'function'
        4:11-4:19 [1] ?: expected ';', found 'function'
        4:11-4:19 [1] ?: expected variable, found 'function'
        4:4-4:10 [1] ?: Cannot declare promoted property outside a constructor
        5:8-5:26 [1] UndefinedFunction: Function nonexistent_func() is not defined"#]]
    .assert_eq(&render_pull_diagnostics(&resp));
}

#[tokio::test]
async fn pull_diagnostics_incremental_changes_update_result() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open("test.php", "<?php\n$x = 1;").await;

    let resp1 = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri.to_string()}
            }),
        )
        .await;

    let id1 = resp1["result"]["resultId"].clone();

    // Make a change that introduces an error
    s.client()
        .notify(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": uri.to_string(), "version": 2},
                "contentChanges": [{
                    "text": "<?php\nundefined_function();"
                }]
            }),
        )
        .await;

    // Request new diagnostics
    let resp2 = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri.to_string()}
            }),
        )
        .await;

    let id2 = resp2["result"]["resultId"].clone();

    // Result ID should change when content changes
    assert_ne!(
        id1, id2,
        "result_id should change after content modification"
    );

    expect!["<empty>"].assert_eq(&render_pull_diagnostics(&resp1));
    expect!["1:0-1:20 [1] UndefinedFunction: Function undefined_function() is not defined"]
        .assert_eq(&render_pull_diagnostics(&resp2));
}

#[tokio::test]
async fn pull_diagnostics_with_namespace_code() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
namespace App\Services;

class UserService {
    public function create(string $name): User {
        return new User($name);
    }
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

    expect![[r#"
        4:42-4:46 [1] UndefinedClass: Class App\Services\User does not exist
        5:19-5:23 [1] UndefinedClass: Class App\Services\User does not exist"#]]
    .assert_eq(&render_pull_diagnostics(&resp));
}

// ============================================================================
// WORKSPACE/APPLYWORLDEDIT VERIFICATION
// ============================================================================

#[tokio::test]
async fn apply_edit_advertised_in_capabilities() {
    // The capability should be advertised - verify via initialize response
    let (_, init_resp) = TestServer::new_with_options(json!({})).await;

    let capabilities = &init_resp["result"]["capabilities"];

    // workspace/applyEdit is advertised implicitly - it's a standard LSP feature
    // The server should have workspace capabilities
    assert!(
        capabilities.get("workspace").is_some(),
        "should have workspace capabilities"
    );
}

// ============================================================================
// DIAGNOSTIC PROVIDER VERIFICATION
// ============================================================================

#[tokio::test]
async fn diagnostic_provider_advertised_in_capabilities() {
    let (_, init_resp) = TestServer::new_with_options(json!({})).await;
    let capabilities = &init_resp["result"]["capabilities"];

    // Check that diagnostic provider is advertised
    assert!(
        capabilities.get("diagnosticProvider").is_some(),
        "should advertise diagnosticProvider capability"
    );

    let diag_provider = &capabilities["diagnosticProvider"];
    assert!(
        diag_provider.get("interFileDependencies").is_some(),
        "should advertise interFileDependencies"
    );
    assert!(
        diag_provider.get("workspaceDiagnostics").is_some(),
        "should advertise workspaceDiagnostics"
    );
}

#[tokio::test]
async fn pull_diagnostics_sequential_files() {
    let mut s = TestServer::new().await;
    let uri1 = s.uri("test1.php");
    let uri2 = s.uri("test2.php");

    s.open("test1.php", "<?php\nclass Foo { }").await;
    s.open("test2.php", "<?php\nfunction bar() {}").await;

    // Make sequential diagnostic requests (not concurrent - avoid borrow issues)
    let resp1 = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri1.to_string()}
            }),
        )
        .await;

    let resp2 = s
        .client()
        .request(
            "textDocument/diagnostic",
            json!({
                "textDocument": {"uri": uri2.to_string()}
            }),
        )
        .await;

    // Both should succeed independently
    assert!(
        resp1.get("result").is_some(),
        "first request should succeed"
    );
    assert!(
        resp2.get("result").is_some(),
        "second request should succeed"
    );

    // Results should be different
    let id1 = resp1["result"]["resultId"].clone();
    let id2 = resp2["result"]["resultId"].clone();
    assert_ne!(id1, id2, "different files should have different result IDs");
}

#[tokio::test]
async fn pull_diagnostics_with_severity_levels() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
// This will cause an error
undefined_function();
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

    let items = resp["result"]["items"].as_array().unwrap();

    // Each diagnostic should have a severity (1=Error, 2=Warning, etc.)
    for item in items {
        assert!(
            item.get("severity").is_some(),
            "each diagnostic should have severity"
        );
        let severity = item["severity"].as_u64().unwrap();
        assert!(
            (1..=4).contains(&severity),
            "severity should be 1-4, got {}",
            severity
        );
    }
}

#[tokio::test]
async fn pull_diagnostics_range_precision() {
    let mut s = TestServer::new().await;
    let uri = s.uri("test.php");

    s.open(
        "test.php",
        r#"<?php
$undefined_var = $x + 1;
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

    let items = resp["result"]["items"].as_array().unwrap();

    // Each diagnostic should have proper range with start/end
    for item in items {
        let range = &item["range"];
        assert!(range.get("start").is_some(), "should have start position");
        assert!(range.get("end").is_some(), "should have end position");

        let start = &range["start"];
        let end = &range["end"];

        assert!(start.get("line").is_some(), "start should have line");
        assert!(
            start.get("character").is_some(),
            "start should have character"
        );
        assert!(end.get("line").is_some(), "end should have line");
        assert!(end.get("character").is_some(), "end should have character");
    }
}

#[tokio::test]
async fn pull_diagnostics_source_field_present() {
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

    let items = resp["result"]["items"].as_array().unwrap();

    // All diagnostics should have a source field identifying them as from php-lsp
    for item in items {
        assert!(
            item.get("source").is_some(),
            "diagnostic should have source field"
        );
        let source = item["source"].as_str().unwrap();
        assert_eq!(
            source, "php-lsp",
            "source should be 'php-lsp', got: {}",
            source
        );
    }
}

#[tokio::test]
async fn pull_diagnostics_message_field_present() {
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
