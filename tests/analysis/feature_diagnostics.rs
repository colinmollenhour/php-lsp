//! Diagnostic coverage matrix using the caret annotation DSL.
//! Each test names the expectation inline with `// ^^^ severity: message`.

use super::*;

use expect_test::expect;
use serde_json::json;

#[tokio::test]
async fn undefined_function_top_level() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function _wrap(): void {
    nonexistent_fn();
//  ^^^^^^^^^^^^^^^^ error: nonexistent_fn
}
"#,
    )
    .await;
}

#[tokio::test]
async fn undefined_function_inside_function() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function wrapper(): void {
    nonexistent_fn();
//  ^^^^^^^^^^^^^^^^ error: nonexistent_fn
}
"#,
    )
    .await;
}

#[tokio::test]
async fn undefined_function_inside_method() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
class C {
    public function run(): void {
        nonexistent_fn();
//      ^^^^^^^^^^^^^^^^ error: nonexistent_fn
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn undefined_function_inside_namespaced_method() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
namespace LspTest;
class Broken {
    public function f(): void {
        nonexistent_fn();
//      ^^^^^^^^^^^^^^^^ error: nonexistent_fn
    }
}
"#,
    )
    .await;
}

/// Regression for issue #170: mir-analyzer must detect errors inside
/// namespaced class method bodies, not just top-level / non-namespaced code.
#[tokio::test]
async fn issue_170_errors_inside_namespaced_method_detected() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
namespace LspTest;

class Broken
{
    public int $count = 0;

    public function bump(): int
    {
        $this->count++;
        return $this->count;
    }

    public function obviouslyBroken(): int
    {
        nonexistent_function();
//      ^^^^^^^^^^^^^^^^^^^^^^ error: nonexistent_function
        $x = new UnknownClass();
//               ^^^^^^^^^^^^ error: UnknownClass
        return 0;
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn undefined_class_in_new() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function _wrap(): void {
    $x = new UnknownClass();
//           ^^^^^^^^^^^^ error: UnknownClass
}
"#,
    )
    .await;
}

#[tokio::test]
async fn clean_file_has_no_diagnostics() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function f(string $x): string { return $x; }
f('ok');
"#,
    )
    .await;
}

#[tokio::test]
async fn diagnostics_clear_after_fix() {
    let mut s = TestServer::new().await;
    let notif = s.open("fix.php", "<?php\nundefined_fn();\n").await;
    assert!(
        !notif["params"]["diagnostics"]
            .as_array()
            .unwrap_or(&vec![])
            .is_empty()
    );
    let after = s.change("fix.php", 2, "<?php\n").await;
    assert!(
        after["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn parse_error_emits_diagnostic() {
    let mut s = TestServer::new().await;
    let notif = s.open("bad.php", "<?php\nfunction f( {\n").await;
    assert!(
        !notif["params"]["diagnostics"]
            .as_array()
            .unwrap_or(&vec![])
            .is_empty(),
        "expected parse diagnostic for malformed PHP"
    );
}

#[tokio::test]
async fn multiple_diagnostics_same_file() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function _wrap(): void {
    one_undefined();
//  ^^^^^^^^^^^^^^^ error: one_undefined
    two_undefined();
//  ^^^^^^^^^^^^^^^ error: two_undefined
}
"#,
    )
    .await;
}

#[tokio::test]
async fn pull_diagnostics_returns_report() {
    let mut server = TestServer::new().await;
    server.open("pull_diag.php", "<?php\n$x = 1;\n").await;

    let resp = server.pull_diagnostics("pull_diag.php").await;

    assert!(
        resp["error"].is_null(),
        "textDocument/diagnostic error: {:?}",
        resp
    );
    let result = &resp["result"];
    assert!(!result.is_null(), "expected non-null diagnostic report");
    // First pull on a freshly-opened file must be a full report, not unchanged.
    assert_eq!(
        result["kind"].as_str(),
        Some("full"),
        "first pull must return kind='full', got: {:?}",
        result["kind"]
    );
    // Clean file has no diagnostics.
    let items = result["items"]
        .as_array()
        .expect("'items' array in full report");
    assert!(
        items.is_empty(),
        "clean file should have zero diagnostics, got: {items:?}"
    );
}

#[tokio::test]
async fn workspace_diagnostic_clean_file() {
    let mut server = TestServer::new().await;
    server.open("ws_clean.php", "<?php\n$x = 1;\n").await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    expect![[r#"
        ws_clean.php
          <clean>"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn workspace_diagnostic_single_file_with_errors() {
    let mut server = TestServer::new().await;
    server
        .open(
            "ws_error.php",
            "<?php\nnonexistent_function();\n$x = new UnknownClass();\n",
        )
        .await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    expect![[r#"
        ws_error.php
          1:0 Function nonexistent_function() is not defined [UndefinedFunction] (error)
          2:9 Class UnknownClass does not exist [UndefinedClass] (error)"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn workspace_diagnostic_multiple_files_mixed() {
    let mut server = TestServer::new().await;
    server
        .open("ws_clean.php", "<?php\nfunction foo(): void {}\n")
        .await;
    server.open("ws_error.php", "<?php\nbar();\n").await;
    server
        .open("ws_another.php", "<?php\n$x = new Missing();\n")
        .await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    expect![[r#"
        ws_another.php
          1:9 Class Missing does not exist [UndefinedClass] (error)
        ws_clean.php
          <clean>
        ws_error.php
          1:0 Function bar() is not defined [UndefinedFunction] (error)"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn workspace_diagnostic_multiple_errors_same_file() {
    let mut server = TestServer::new().await;
    server
        .open(
            "multi_err.php",
            "<?php\none_undefined();\ntwo_undefined();\nthree_undefined();\n",
        )
        .await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    expect![[r#"
        multi_err.php
          1:0 Function one_undefined() is not defined [UndefinedFunction] (error)
          2:0 Function two_undefined() is not defined [UndefinedFunction] (error)
          3:0 Function three_undefined() is not defined [UndefinedFunction] (error)"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn workspace_diagnostic_after_edit() {
    let mut server = TestServer::new().await;
    server.open("ws_fix.php", "<?php\nundefined_fn();\n").await;

    // Verify initial error
    let resp1 = server.workspace_diagnostic().await;
    let out1 = render_workspace_diagnostic(&resp1, &server.uri(""));
    expect![[r#"
        ws_fix.php
          1:0 Function undefined_fn() is not defined [UndefinedFunction] (error)"#]]
    .assert_eq(&out1);

    // Fix the error by changing the code
    server.change("ws_fix.php", 2, "<?php\n").await;

    // Verify error is gone
    let resp2 = server.workspace_diagnostic().await;
    let out2 = render_workspace_diagnostic(&resp2, &server.uri(""));

    expect![[r#"
        ws_fix.php
          <clean>"#]]
    .assert_eq(&out2);
}

#[tokio::test]
async fn workspace_diagnostic_empty_workspace() {
    let mut server = TestServer::new().await;

    let resp = server.workspace_diagnostic().await;

    assert!(
        resp["error"].is_null(),
        "workspace/diagnostic error: {:?}",
        resp
    );
    let items = resp["result"]["items"]
        .as_array()
        .expect("expected 'items' array in workspace diagnostic report");
    assert!(
        items.is_empty(),
        "empty workspace should have no diagnostic items, got: {items:?}"
    );
}

#[tokio::test]
async fn workspace_diagnostic_named_arguments() {
    let mut server = TestServer::new().await;
    server
        .open(
            "ws_named_args.php",
            "<?php\nfunction foo(int $a, int $b): void {}\nfoo(a: 1, b: 2, a: 3);\n",
        )
        .await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    expect![[r#"
        ws_named_args.php
          2:16 foo() has no parameter named $a [InvalidNamedArgument] (error)"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn workspace_diagnostic_with_parse_error() {
    let mut server = TestServer::new().await;
    server
        .open("ws_parse_error.php", "<?php\nfunction f( {\n")
        .await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    // Parse error should produce diagnostic
    assert!(out.contains("ws_parse_error.php"));
    // The exact message varies by parser; just verify it exists
    let items = resp["result"]["items"].as_array().unwrap();
    assert!(!items.is_empty(), "parse error should produce diagnostic");
}

#[tokio::test]
async fn workspace_diagnostic_circular_inheritance() {
    let mut server = TestServer::new().await;
    server
        .open(
            "ws_circular.php",
            "<?php\nclass A extends B {}\nclass B extends A {}\n",
        )
        .await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    expect![[r#"
        ws_circular.php
          2:0 Class B has a circular inheritance chain [CircularInheritance] (error)"#]]
    .assert_eq(&out);
}

// ─────────────────────────────────────────────────────────────────────────
// REGRESSION TESTS - Verify bugs are fixed and don't regress
// ─────────────────────────────────────────────────────────────────────────

/// REGRESSION: result_id must be non-null for caching to work.
/// Previously: result_id was always None, breaking LSP caching protocol.
/// Fixed: result_id is now generated from diagnostic content hash.
/// This test ensures the fix stays in place.
#[tokio::test]
async fn regression_result_id_is_present() {
    let mut server = TestServer::new().await;
    server.open("test1.php", "<?php\n$x = 1;\n").await;

    let resp = server.workspace_diagnostic().await;
    let items = resp["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);

    let result_id = &items[0]["resultId"];
    assert!(
        !result_id.is_null(),
        "REGRESSION: resultId must be non-null. \
         Clients need this to implement caching via previousResultIds."
    );

    // Verify it's a string, not some other JSON type
    assert!(
        result_id.is_string(),
        "resultId should be a string (format: v1:hash)"
    );
}

/// REGRESSION: Files with parse errors must appear in workspace/diagnostic.
/// Previously: There was potential for parse-error-only files to be filtered out.
/// This test verifies parse errors are correctly included.
#[tokio::test]
async fn regression_parse_error_files_included() {
    let mut server = TestServer::new().await;
    server
        .open("parse_only.php", "<?php\nfunction broken( {\n")
        .await;

    let resp = server.workspace_diagnostic().await;
    let items = resp["result"]["items"].as_array().unwrap();

    // Parse error files must be included
    assert!(
        !items.is_empty(),
        "Parse error files must appear in workspace/diagnostic"
    );

    // Should have the parse error in diagnostics
    assert!(
        items[0]["items"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false),
        "File should have diagnostics (parse error)"
    );
}

/// REGRESSION: result_id must be unique per file for caching.
/// Previously: result_id was always None for all files.
/// Fixed: Each file now gets a deterministic result_id based on content hash.
#[tokio::test]
async fn regression_result_id_unique_per_file() {
    let mut server = TestServer::new().await;
    server.open("file1.php", "<?php\necho 'a';\n").await;
    server.open("file2.php", "<?php\necho 'b';\n").await;

    let resp = server.workspace_diagnostic().await;
    let items = resp["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);

    let id1 = items[0]["resultId"].as_str().unwrap();
    let id2 = items[1]["resultId"].as_str().unwrap();

    // Each file must have a result_id
    assert!(!id1.is_empty(), "file1 must have result_id");
    assert!(!id2.is_empty(), "file2 must have result_id");

    // Different files should have different result_ids (different content)
    assert_ne!(
        id1, id2,
        "Different files with different content should have different result_ids"
    );
}

/// REGRESSION: result_id must change when diagnostics change.
/// Previously: result_id was always None.
/// Fixed: result_id is now based on diagnostic content, so it changes when errors appear/disappear.
#[tokio::test]
async fn regression_result_id_changes_with_diagnostics() {
    let mut server = TestServer::new().await;
    server.open("changetest.php", "<?php\n$x = 1;\n").await;

    // Get result_id for clean file
    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();
    let id_clean = items1[0]["resultId"].as_str().unwrap().to_string();

    // Add an error to the file
    server
        .change("changetest.php", 2, "<?php\nundefined_function();\n")
        .await;

    // Get result_id for file with error
    let resp2 = server.workspace_diagnostic().await;
    let items2 = resp2["result"]["items"].as_array().unwrap();
    let id_with_error = items2[0]["resultId"].as_str().unwrap().to_string();

    // result_id must change when diagnostics change
    assert_ne!(
        id_clean, id_with_error,
        "result_id must change when diagnostics change"
    );

    // Verify the error is actually there
    assert!(
        !items2[0]["items"].as_array().unwrap().is_empty(),
        "File should have diagnostics after adding error"
    );

    // Fix the error
    server.change("changetest.php", 2, "<?php\n$x = 1;\n").await;

    // Get result_id for fixed file
    let resp3 = server.workspace_diagnostic().await;
    let items3 = resp3["result"]["items"].as_array().unwrap();
    let id_fixed = items3[0]["resultId"].as_str().unwrap().to_string();

    // Should revert to original result_id
    assert_eq!(
        id_clean, id_fixed,
        "result_id should revert when diagnostics return to original state"
    );
}

/// REGRESSION: document/diagnostic and workspace/diagnostic must both use result_id.
/// Previously: Both handlers set result_id to None.
/// Fixed: Both now generate consistent, deterministic result_ids.
#[tokio::test]
async fn regression_document_and_workspace_diagnostic_consistency() {
    let mut server = TestServer::new().await;
    server
        .open("consistency.php", "<?php\necho 'test';\n")
        .await;

    let doc_resp = server.pull_diagnostics("consistency.php").await;
    let ws_resp = server.workspace_diagnostic().await;

    // textDocument/diagnostic response structure: result has resultId directly (camelCase)
    let doc_result_id = &doc_resp["result"]["resultId"];

    // workspace/diagnostic response structure: items is array of reports
    // Each report can have result_id at root level or nested in full_document_diagnostic_report
    let ws_result = &ws_resp["result"];
    let ws_item = &ws_result["items"][0];
    let ws_result_id = &ws_item["resultId"];

    // Both must have resultId
    assert!(
        !doc_result_id.is_null(),
        "document/diagnostic must have resultId"
    );
    assert!(
        !ws_result_id.is_null(),
        "workspace/diagnostic must have resultId"
    );

    // Both should be strings
    let doc_id = doc_result_id.as_str();
    let ws_id = ws_result_id.as_str();
    assert!(
        doc_id.is_some() && ws_id.is_some(),
        "resultIds must be strings"
    );

    // They should be identical (same file, same content)
    assert_eq!(
        doc_id, ws_id,
        "Both endpoints should return same resultId for same file"
    );
}

/// REGRESSION: Error handling must propagate errors to client.
/// Previously: .unwrap_or_default() would silently hide task panics.
/// Fixed: Errors are now properly propagated via LSP error response.
#[tokio::test]
async fn regression_error_handling() {
    let mut server = TestServer::new().await;
    server.open("test.php", "<?php\n").await;

    let resp = server.workspace_diagnostic().await;

    // This should always succeed (no parse/semantic errors in clean file)
    assert!(
        resp["error"].is_null(),
        "workspace_diagnostic request should not error for valid files"
    );

    // Check that response structure is valid
    assert!(
        resp["result"]["items"].is_array(),
        "Response should contain items array"
    );
}

/// REGRESSION: result_id must be stable across consecutive requests.
/// Same file with same diagnostics should return same result_id.
#[tokio::test]
async fn regression_result_id_is_stable() {
    let mut server = TestServer::new().await;
    server.open("stable.php", "<?php\necho 'hello';\n").await;

    // First request
    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();
    let id1 = items1[0]["resultId"].as_str().unwrap().to_string();

    // Second request (no changes)
    let resp2 = server.workspace_diagnostic().await;
    let items2 = resp2["result"]["items"].as_array().unwrap();
    let id2 = items2[0]["resultId"].as_str().unwrap().to_string();

    // result_id must be identical (deterministic hash)
    assert_eq!(
        id1, id2,
        "result_id must be stable for unchanged file (deterministic hashing)"
    );
}

/// REGRESSION: result_id must account for all diagnostic types.
/// File with both parse errors and semantic errors should have result_id that reflects both.
#[tokio::test]
async fn regression_result_id_with_mixed_diagnostics() {
    let mut server = TestServer::new().await;

    // File with semantic error (no parse error)
    server
        .open(
            "semantic.php",
            "<?php\nfunction foo() {}\nundefined_func();\n",
        )
        .await;

    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();
    let id_semantic = items1[0]["resultId"].as_str().unwrap();

    // Different file with only parse error
    server
        .open("parse.php", "<?php\nfunction broken( {\n")
        .await;

    let resp2 = server.workspace_diagnostic().await;
    let items2 = resp2["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| {
            item["uri"]
                .as_str()
                .map(|uri| uri.contains("parse.php"))
                .unwrap_or(false)
        })
        .unwrap();
    let id_parse = items2["resultId"].as_str().unwrap();

    // Different error types should produce different result_ids
    assert_ne!(
        id_semantic, id_parse,
        "result_id should differ for different diagnostic types"
    );
}

/// REGRESSION: workspace_diagnostic must accept params without error.
/// The LSP spec allows clients to send previousResultIds in params.
/// Handler must accept params structure gracefully (even if not using Unchanged variant yet).
#[tokio::test]
async fn regression_params_structure_accepted() {
    let mut server = TestServer::new().await;
    server.open("param_test.php", "<?php\necho 'test';\n").await;

    // Request workspace/diagnostic (which accepts WorkspaceDiagnosticParams)
    let resp = server.workspace_diagnostic().await;

    // Should not error even though params include previousResultIds capability
    assert!(
        resp["error"].is_null(),
        "workspace_diagnostic must accept params without error"
    );

    // Should return valid response structure
    assert!(
        resp["result"]["items"].is_array(),
        "Should return items array"
    );
}

/// CRITICAL: result_id must change when diagnostic properties change.
/// Even if position and message are identical, severity changes must produce different result_id.
/// This was missing from initial hash implementation.
#[tokio::test]
async fn regression_result_id_reflects_all_diagnostic_properties() {
    let mut server = TestServer::new().await;

    // Open file with undefined function (error severity)
    server
        .open(
            "props1.php",
            "<?php\nfunction test() {}\nundefined_func();\n",
        )
        .await;

    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();
    assert_eq!(items1.len(), 1);

    // Get the diagnostic to verify its properties
    let diags1 = items1[0]["items"].as_array().unwrap();
    assert!(
        !diags1.is_empty(),
        "Should have undefined function diagnostic"
    );

    let result_id_1 = items1[0]["resultId"].as_str().unwrap().to_string();

    // Open different file with undefined variable (different code/severity)
    server
        .open("props2.php", "<?php\necho $undefined_var;\n")
        .await;

    let resp2 = server.workspace_diagnostic().await;
    let items2 = resp2["result"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| {
            item["uri"]
                .as_str()
                .map(|uri| uri.contains("props2.php"))
                .unwrap_or(false)
        })
        .unwrap();

    let result_id_2 = items2["resultId"].as_str().unwrap();

    // Different diagnostic codes/types should produce different result_ids
    // (UndefinedFunction vs UndefinedVariable)
    assert_ne!(
        result_id_1, result_id_2,
        "Different diagnostic codes should produce different result_ids \
         (even if both are 1 error). Hash must include code field."
    );
}

// ─────────────────────────────────────────────────────────────────────────
// EDGE CASE TESTS - Stress scenarios and boundary conditions
// ─────────────────────────────────────────────────────────────────────────

/// EDGE CASE: Very large workspace with many files.
/// workspace_diagnostic iterates all open files and runs semantic analysis on each.
/// Should verify it doesn't have quadratic behavior or memory issues.
#[tokio::test]
#[ignore = "O(N²) cross-file re-analysis per did_open — fix performance before re-enabling"]
async fn edge_case_workspace_diagnostic_many_files() {
    let mut server = TestServer::new().await;

    // Open 10 files
    for i in 0..10 {
        let code = format!("<?php\nfunction test{i}() {{ return 42; }}\n");
        server.open(&format!("file{i}.php"), &code).await;
    }

    let resp = server.workspace_diagnostic().await;
    let items = resp["result"]["items"].as_array().unwrap();

    assert_eq!(
        items.len(),
        10,
        "workspace_diagnostic should return diagnostics for all open files"
    );

    // All should be clean
    for item in items {
        assert!(
            item["items"].as_array().unwrap().is_empty(),
            "Clean files should have empty diagnostics array"
        );
    }
}

/// EDGE CASE: File closed after workspace_diagnostic starts but before completion.
/// The blocking task might try to get_doc_salsa for a URI that was just closed.
/// Result: silently filtered out (because filter_map), which is probably correct,
/// but worth documenting.
#[tokio::test]
async fn edge_case_file_closed_during_workspace_diagnostic() {
    let mut server = TestServer::new().await;
    server.open("temp.php", "<?php\nundefined();\n").await;

    // Immediately close and open another file
    // (This test verifies the handler doesn't panic, not a true race condition)
    let resp = server.workspace_diagnostic().await;

    assert!(
        resp["error"].is_null(),
        "workspace_diagnostic should handle file closure gracefully"
    );
}

#[tokio::test]
async fn requests_on_parse_error_file_do_not_error() {
    let mut server = TestServer::new().await;
    let notif = server
        .open("broken.php", "<?php\nfunction f( $x { // missing ): body\n")
        .await;

    let diags = notif["params"]["diagnostics"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !diags.is_empty(),
        "expected parse diagnostics for broken source"
    );

    let resp = server.hover("broken.php", 1, 10).await;
    assert!(resp["error"].is_null(), "hover errored: {resp:?}");

    let resp = server.document_symbols("broken.php").await;
    assert!(resp["error"].is_null(), "documentSymbol errored: {resp:?}");

    let resp = server.folding_range("broken.php").await;
    assert!(resp["error"].is_null(), "foldingRange errored: {resp:?}");
}

#[tokio::test]
async fn diagnostics_published_on_did_change_for_undefined_function() {
    let mut server = TestServer::new().await;
    server.open("change_test.php", "<?php\n").await;

    let notif = server
        .change("change_test.php", 2, "<?php\nnonexistent_function();\n")
        .await;
    let has = notif["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"].as_str() == Some("UndefinedFunction"));
    assert!(has, "expected UndefinedFunction after didChange: {notif:?}");
}

/// Regression for issue #177 — deprecated-call warnings must appear on did_open,
/// not only after the first did_change.
#[tokio::test]
async fn did_open_reports_deprecated_call_warning() {
    let mut server = TestServer::new().await;
    let notif = server
        .open(
            "deprecated_test.php",
            "<?php\n/** @deprecated Use newFunc() instead */\nfunction oldFunc(): void {}\n\noldFunc();\n",
        )
        .await;
    let diags = notif["params"]["diagnostics"].as_array().unwrap();
    let hit = diags.iter().find(|d| {
        d["code"].as_str() == Some("DeprecatedCall")
            && d["message"]
                .as_str()
                .map(|m| m.contains("oldFunc"))
                .unwrap_or(false)
    });
    assert!(
        hit.is_some(),
        "expected DeprecatedCall diagnostic for oldFunc on did_open, got: {diags:?}"
    );
}

#[tokio::test]
async fn undefined_function_detected_in_static_method() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
class Factory {
    public static function build(): void {
        nonexistent_function();
//      ^^^^^^^^^^^^^^^^^^^^^^ error: nonexistent_function
    }
}
"#,
        )
        .await;
}

#[tokio::test]
async fn undefined_function_detected_in_arrow_function() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
$fn = fn() => nonexistent_function();
//            ^^^^^^^^^^^^^^^^^^^^^^ error: nonexistent_function
"#,
        )
        .await;
}

#[tokio::test]
async fn undefined_function_detected_in_trait_method() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
trait Auditable {
    public function audit(): void {
        nonexistent_function();
//      ^^^^^^^^^^^^^^^^^^^^^^ error: nonexistent_function
    }
}
"#,
        )
        .await;
}

#[tokio::test]
async fn undefined_function_detected_in_closure() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
$fn = function() {
    nonexistent_function();
//  ^^^^^^^^^^^^^^^^^^^^^^ error: nonexistent_function
};
"#,
        )
        .await;
}

#[tokio::test]
async fn argument_count_too_few_detected() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
function needs_two(string $a, string $b): void {}
function wrap(): void {
    needs_two('x');
//  ^^^^^^^^^^^^^^ error: needs_two
}
"#,
        )
        .await;
}

#[tokio::test]
async fn argument_type_mismatch_detected() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
function takes_string(string $s): void {}
function wrap(): void {
    takes_string(42);
//               ^^ error: takes_string
}
"#,
        )
        .await;
}

/// PSR-4-resolvable classes must not produce UndefinedClass diagnostics even
/// when the background workspace scan has not yet reached the dependency file.
/// The fix (PSR-4 lazy-loading inside `get_semantic_issues_salsa`) reads the
/// dependency from disk before running semantic analysis, making the result
/// deterministic regardless of scan timing.
#[tokio::test]
async fn psr4_imported_class_not_flagged_before_workspace_scan() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    // Dependency: exists on disk; lazy-loading must find it via PSR-4.
    std::fs::create_dir_all(tmp.path().join("src/Model")).unwrap();
    std::fs::write(
        tmp.path().join("src/Model/Entity.php"),
        "<?php\nnamespace App\\Model;\nclass Entity {}\n",
    )
    .unwrap();

    // Consuming file: uses Entity as a parameter type — the analyzer resolves
    // parameter types through use statements, exercising the full lazy-load path.
    std::fs::create_dir_all(tmp.path().join("src/Service")).unwrap();
    let handler_src = "<?php\nnamespace App\\Service;\nuse App\\Model\\Entity;\nfunction handle(Entity $e): Entity { return $e; }\n";
    std::fs::write(tmp.path().join("src/Service/Handler.php"), handler_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Service/Handler.php", handler_src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Service/Handler.php
          <clean>"#]]
    .assert_eq(&out);
}

/// Same-namespace, single-base PSR-4: a class referencing a sibling in the
/// same namespace WITHOUT a `use` statement must not emit UndefinedClass.
/// This is the core bug — the previous pre-load path only covered `use`
/// imports and FQN-`new` refs, missing bare same-namespace type hints,
/// `extends`, `instanceof`, and static-member access.
#[tokio::test]
async fn same_namespace_bare_ref_not_flagged_as_undefined_class() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("src/Producer.php"),
        "<?php\nnamespace App;\nclass Producer {\n    public function make(): string { return 'p'; }\n}\n",
    )
    .unwrap();

    // Consumer references Producer in three positions (type hint, new,
    // instanceof) — all bare, no `use` because both live in `namespace App`.
    let consumer_src = "<?php\nnamespace App;\nclass Consumer {\n    public function __construct(private Producer $p) {}\n    public function fresh(): Producer {\n        return new Producer();\n    }\n    public function isProducer(mixed $x): bool {\n        return $x instanceof Producer;\n    }\n}\n";
    std::fs::write(tmp.path().join("src/Consumer.php"), consumer_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Consumer.php", consumer_src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Consumer.php
          <clean>"#]]
    .assert_eq(&out);
}

/// Same-namespace, single-base PSR-4: `extends` across files with no `use`.
/// `extends` is a separate AST position from type hints; this guards against a
/// regression where the visitor stops collecting one but not the other.
#[tokio::test]
async fn same_namespace_extends_not_flagged_as_undefined_class() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(
        tmp.path().join("src/Base.php"),
        "<?php\nnamespace App;\nabstract class Base {}\n",
    )
    .unwrap();

    let child_src = "<?php\nnamespace App;\nfinal class Child extends Base {}\n";
    std::fs::write(tmp.path().join("src/Child.php"), child_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Child.php", child_src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Child.php
          <clean>"#]]
    .assert_eq(&out);
}

/// Positive control for the above: a truly-missing same-namespace class must
/// still be flagged. Without this, the no-false-positive tests prove nothing.
#[tokio::test]
async fn same_namespace_truly_missing_class_is_flagged() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    // No `Missing` class exists anywhere on disk.
    let consumer_src = "<?php\nnamespace App;\nclass Consumer {\n    public function __construct(private Missing $m) {}\n}\n";
    std::fs::write(tmp.path().join("src/Consumer.php"), consumer_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Consumer.php", consumer_src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    assert!(
        out.contains("UndefinedClass") && out.contains("App\\Missing"),
        "expected UndefinedClass for App\\Missing, got:\n{out}"
    );
}

#[tokio::test]
async fn same_namespace_trait_use_truly_missing_is_flagged() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    let user_src = "<?php\nnamespace App;\nclass Person {\n    use MissingTrait;\n}\n";
    std::fs::write(tmp.path().join("src/Person.php"), user_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Person.php", user_src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    assert!(
        out.contains("MissingTrait"),
        "expected a diagnostic mentioning MissingTrait, got:\n{out}"
    );
}

#[tokio::test]
async fn invalid_trait_use_class_as_trait() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
class NotATrait {}
class User {
    use NotATrait;
//      ^^^^^^^^^ error: NotATrait is a class, not a trait
}
"#,
    )
    .await;
}

#[tokio::test]
async fn argument_count_too_many_detected() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
function takes_one(string $s): void {}
function wrap(): void {
    takes_one('a', 'b', 'c');
//                 ^^^ error: takes_one
}
"#,
        )
        .await;
}

/// Regression: `new ShortName()` where `use A\B\ShortName;` must not emit
/// UndefinedClass when the class is on disk (PSR-4 lazy-loading path).
/// Distinct from `psr4_imported_class_not_flagged_before_workspace_scan` which
/// only tested parameter type hints — this exercises the `new` expression path.
#[tokio::test]
async fn new_expr_with_use_import_not_flagged_as_undefined_class() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Model")).unwrap();
    std::fs::write(
        tmp.path().join("src/Model/Entity.php"),
        "<?php\nnamespace App\\Model;\nclass Entity {}\n",
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Service")).unwrap();
    let src = "<?php\nnamespace App\\Service;\nuse App\\Model\\Entity;\nfunction handle(): void { $e = new Entity(); }\n";
    std::fs::write(tmp.path().join("src/Service/Handler.php"), src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Service/Handler.php", src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Service/Handler.php
          <clean>"#]]
    .assert_eq(&out);
}

/// Regression: `use A\B\C as Alias; new Alias()` must not emit UndefinedClass.
/// The explicit `as` form writes a different key into `file_imports` than the
/// implicit short-name form, and is the primary path that was broken before
/// mir 0.14.0 populated `Codebase.file_imports` from `StubSlice.imports`.
#[tokio::test]
async fn new_expr_with_explicit_use_alias_not_flagged_as_undefined_class() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Model")).unwrap();
    std::fs::write(
        tmp.path().join("src/Model/Entity.php"),
        "<?php\nnamespace App\\Model;\nclass Entity {}\n",
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Service")).unwrap();
    let src = "<?php\nnamespace App\\Service;\nuse App\\Model\\Entity as EntityAlias;\nfunction handle(): void { $e = new EntityAlias(); }\n";
    std::fs::write(tmp.path().join("src/Service/Handler.php"), src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Service/Handler.php", src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Service/Handler.php
          <clean>"#]]
    .assert_eq(&out);
}

/// Sanity baseline: fully-qualified `new \App\Model\Entity()` (no `use` statement)
/// must not emit UndefinedClass when the class is PSR-4-resolvable.
#[tokio::test]
async fn new_expr_fully_qualified_not_flagged_as_undefined_class() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Model")).unwrap();
    std::fs::write(
        tmp.path().join("src/Model/Entity.php"),
        "<?php\nnamespace App\\Model;\nclass Entity {}\n",
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Service")).unwrap();
    let src = "<?php\nnamespace App\\Service;\nfunction handle(): void { $e = new \\App\\Model\\Entity(); }\n";
    std::fs::write(tmp.path().join("src/Service/Handler.php"), src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Service/Handler.php", src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Service/Handler.php
          <clean>"#]]
    .assert_eq(&out);
}

/// Positive control: a genuinely unknown class in a `new` expression must still
/// emit UndefinedClass so the above no-false-positive tests are meaningful.
#[tokio::test]
async fn new_expr_truly_unknown_class_is_flagged() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
function _wrap(): void {
    $x = new TrulyNonExistentClass9z();
//           ^^^^^^^^^^^^^^^^^^^^^^^ error: TrulyNonExistentClass9z
}
"#,
        )
        .await;
}

// ── named argument diagnostics ────────────────────────────────────────────────

#[tokio::test]
async fn duplicate_named_arg_in_function_call() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function foo(int $a, int $b): void {}
foo(a: 1, b: 2, a: 3);
//              ^^^^ error: foo() has no parameter named $a
"#,
    )
    .await;
}

#[tokio::test]
async fn duplicate_named_arg_in_method_call() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
class C {
    public function run(int $x, int $y): void {}
}
(new C())->run(x: 1, y: 2, x: 99);
//                         ^^^^^ error: run() has no parameter named $x
"#,
    )
    .await;
}

#[tokio::test]
async fn duplicate_named_arg_in_constructor() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
class Point {
    public function __construct(public int $x, public int $y) {}
}
new Point(x: 0, y: 1, x: 2);
//                    ^^^^ error: Point::__construct() has no parameter named $x
"#,
    )
    .await;
}

#[tokio::test]
async fn positional_after_named_arg() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function bar(int $a, int $b): void {}
bar(a: 1, 2);
//        ^ error: cannot use positional argument after named argument
//        ^ error: bar() has no parameter named $#2
"#,
    )
    .await;
}

#[tokio::test]
async fn valid_named_args_produce_no_diagnostic() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function greet(string $name, int $times): void {}
greet(name: 'Alice', times: 3);
"#,
    )
    .await;
}

// ── circular inheritance diagnostics ─────────────────────────────────────────

#[tokio::test]
async fn circular_inheritance_self_extends() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
  class A extends A {}
//^^^^^^^^^^^^^^^^^^^^ error: Class A has a circular inheritance chain
"#,
    )
    .await;
}

#[tokio::test]
async fn circular_inheritance_two_class_cycle() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
  class A extends B {}
  class B extends A {}
//^^^^^^^^^^^^^^^^^^^^ error: Class B has a circular inheritance chain
"#,
    )
    .await;
}

#[tokio::test]
async fn circular_inheritance_three_class_cycle() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
  class A extends B {}
  class B extends C {}
  class C extends A {}
//^^^^^^^^^^^^^^^^^^^^ error: Class C has a circular inheritance chain
"#,
    )
    .await;
}

/// Baseline: the bare PHP built-in `restore_error_handler()` resolves via mir's
/// bundled stubs and should produce no `UndefinedFunction` diagnostic.
#[tokio::test]
async fn builtin_restore_error_handler_is_known() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function _wrap(): void {
    restore_error_handler();
}
"#,
    )
    .await;
}

/// Reproducer: a project polyfill that conditionally redefines a built-in.
/// If `ingest_stub_slice` is last-write-wins and the project file's parsed
/// `function restore_error_handler` overrides mir's stub, the call site may
/// still resolve — but the polyfill body is what ends up authoritative. This
/// test asserts that the call is *not* flagged undefined when a user-land
/// polyfill exists in the workspace.
#[tokio::test]
async fn user_polyfill_does_not_break_builtin_restore_error_handler() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"//- /src/polyfill.php
<?php
if (!function_exists('restore_error_handler')) {
    function restore_error_handler(): bool { return true; }
}

//- /src/main.php
<?php
function _wrap(): void {
    restore_error_handler();
}
"#,
    )
    .await;
}

/// Reproducer: an unconditional user-land redefinition of a built-in.
/// PHP would refuse this at runtime, but the LSP still parses it; if the
/// stub-ingest path is last-write-wins, the project's body silently replaces
/// mir's stub. The call site should still resolve.
#[tokio::test]
async fn user_unconditional_redefinition_does_not_break_call() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"//- /src/redef.php
<?php
function restore_error_handler(): bool { return true; }

//- /src/main.php
<?php
function _wrap(): void {
    restore_error_handler();
}
"#,
    )
    .await;
}

#[tokio::test]
async fn circular_inheritance_suppressed_when_type_errors_disabled() {
    let (mut s, _resp) = TestServer::new_with_options(json!({
        "diagnostics": { "typeErrors": false }
    }))
    .await;
    s.check_diagnostics(
        r#"<?php
class A extends A {}
"#,
    )
    .await;
}

// --- workspace/diagnostic Unchanged variant tests (BUG #1 + BUG #3) ---

#[tokio::test]
async fn workspace_diagnostic_unchanged_on_repeated_request() {
    let mut server = TestServer::new().await;
    server.open("stable.php", "<?php\n$x = 1;\n").await;

    // First request: empty previousResultIds → all files must return Full
    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();
    assert_eq!(items1[0]["kind"].as_str().unwrap(), "full");

    let result_id = items1[0]["resultId"].as_str().unwrap().to_string();
    let uri = items1[0]["uri"].as_str().unwrap().to_string();

    // Second request with correct previousResultIds → must return Unchanged
    let resp2 = server
        .workspace_diagnostic_with_prev(vec![(uri, result_id)])
        .await;
    let items2 = resp2["result"]["items"].as_array().unwrap();

    assert_eq!(
        items2[0]["kind"].as_str().unwrap(),
        "unchanged",
        "second request with matching result_id must return Unchanged"
    );

    let out2 = render_workspace_diagnostic(&resp2, &server.uri(""));
    expect![[r#"
        stable.php
          <unchanged>"#]]
    .assert_eq(&out2);
}

#[tokio::test]
async fn workspace_diagnostic_full_after_file_change() {
    let mut server = TestServer::new().await;
    server.open("changing.php", "<?php\n$x = 1;\n").await;

    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();
    let old_id = items1[0]["resultId"].as_str().unwrap().to_string();
    let uri = items1[0]["uri"].as_str().unwrap().to_string();

    // Introduce a semantic error
    server
        .change("changing.php", 2, "<?php\nundefined_fn();\n")
        .await;

    // previousResultIds contains stale result_id → must return Full
    let resp2 = server
        .workspace_diagnostic_with_prev(vec![(uri, old_id.clone())])
        .await;
    let items2 = resp2["result"]["items"].as_array().unwrap();

    assert_eq!(
        items2[0]["kind"].as_str().unwrap(),
        "full",
        "stale previousResultId must yield Full"
    );

    let new_id = items2[0]["resultId"].as_str().unwrap();
    assert_ne!(
        new_id, old_id,
        "result_id must change when diagnostics change"
    );

    let out2 = render_workspace_diagnostic(&resp2, &server.uri(""));
    expect![[r#"
        changing.php
          1:0 Function undefined_fn() is not defined [UndefinedFunction] (error)"#]]
    .assert_eq(&out2);
}

#[tokio::test]
async fn workspace_diagnostic_mixed_unchanged_and_full() {
    let mut server = TestServer::new().await;
    server.open("stable.php", "<?php\n$x = 1;\n").await;
    server.open("breaking.php", "<?php\n$y = 2;\n").await;

    // First request: both Full
    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();

    // Only send previousResultId for stable.php
    let stable = items1
        .iter()
        .find(|i| i["uri"].as_str().unwrap_or("").contains("stable.php"))
        .unwrap();
    let prev = vec![(
        stable["uri"].as_str().unwrap().to_string(),
        stable["resultId"].as_str().unwrap().to_string(),
    )];

    // Change breaking.php before second request
    server
        .change("breaking.php", 2, "<?php\nundefined_fn();\n")
        .await;

    let resp2 = server.workspace_diagnostic_with_prev(prev).await;
    let out2 = render_workspace_diagnostic(&resp2, &server.uri(""));

    expect![[r#"
        breaking.php
          1:0 Function undefined_fn() is not defined [UndefinedFunction] (error)
        stable.php
          <unchanged>"#]]
    .assert_eq(&out2);
}

#[tokio::test]
async fn workspace_diagnostic_new_file_always_full() {
    let mut server = TestServer::new().await;
    server.open("existing.php", "<?php\n$x = 1;\n").await;

    let resp1 = server.workspace_diagnostic().await;
    let items1 = resp1["result"]["items"].as_array().unwrap();
    let prev = vec![(
        items1[0]["uri"].as_str().unwrap().to_string(),
        items1[0]["resultId"].as_str().unwrap().to_string(),
    )];

    // Open a new file not in previousResultIds
    server.open("newfile.php", "<?php\n$y = 2;\n").await;

    let resp2 = server.workspace_diagnostic_with_prev(prev).await;
    let items2 = resp2["result"]["items"].as_array().unwrap();

    let new_item = items2
        .iter()
        .find(|i| i["uri"].as_str().unwrap_or("").contains("newfile.php"))
        .expect("newfile.php must appear in response");

    assert_eq!(
        new_item["kind"].as_str().unwrap(),
        "full",
        "file absent from previousResultIds must return Full"
    );
}

#[tokio::test]
async fn workspace_diagnostic_wrong_result_id_returns_full() {
    let mut server = TestServer::new().await;
    server.open("test.php", "<?php\n$x = 1;\n").await;
    let uri = server.uri("test.php");

    // Send a result_id that doesn't match the real one
    let resp = server
        .workspace_diagnostic_with_prev(vec![(uri, "wrong-result-id".to_string())])
        .await;
    let items = resp["result"]["items"].as_array().unwrap();

    assert_eq!(
        items[0]["kind"].as_str().unwrap(),
        "full",
        "wrong previousResultId must produce Full, not Unchanged"
    );
}

#[tokio::test]
async fn workspace_diagnostic_empty_prev_ids_all_full() {
    let mut server = TestServer::new().await;
    server.open("a.php", "<?php\n$x = 1;\n").await;
    server.open("b.php", "<?php\n$y = 2;\n").await;

    // workspace_diagnostic() hardcodes empty previousResultIds
    let resp = server.workspace_diagnostic().await;
    let items = resp["result"]["items"].as_array().unwrap();

    for item in items {
        assert_eq!(
            item["kind"].as_str().unwrap(),
            "full",
            "empty previousResultIds must yield Full for all files"
        );
    }
}

// ── use-import edge cases ──────────────────────────────────────────────────────

/// Grouped `use` import (`use A\{B, C};`) must not produce UndefinedClass for
/// any name in the group when the classes are PSR-4-resolvable on disk.
#[tokio::test]
async fn new_expr_with_grouped_use_import_not_flagged_as_undefined_class() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Model")).unwrap();
    std::fs::write(
        tmp.path().join("src/Model/Foo.php"),
        "<?php\nnamespace App\\Model;\nclass Foo {}\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("src/Model/Bar.php"),
        "<?php\nnamespace App\\Model;\nclass Bar {}\n",
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Service")).unwrap();
    let src = "<?php\nnamespace App\\Service;\nuse App\\Model\\{Foo, Bar};\nfunction handle(): void { $a = new Foo(); $b = new Bar(); }\n";
    std::fs::write(tmp.path().join("src/Service/Handler.php"), src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Service/Handler.php", src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Service/Handler.php
          <clean>"#]]
    .assert_eq(&out);
}

/// `use` import of an interface must not be flagged UndefinedClass when the
/// interface is PSR-4-resolvable and used in an `implements` clause.
#[tokio::test]
async fn use_imported_interface_in_implements_not_flagged_as_undefined_class() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("composer.json"),
        r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Contract")).unwrap();
    std::fs::write(
        tmp.path().join("src/Contract/Runnable.php"),
        "<?php\nnamespace App\\Contract;\ninterface Runnable { public function run(): void; }\n",
    )
    .unwrap();

    std::fs::create_dir_all(tmp.path().join("src/Service")).unwrap();
    let src = "<?php\nnamespace App\\Service;\nuse App\\Contract\\Runnable;\nclass Worker implements Runnable { public function run(): void {} }\n";
    std::fs::write(tmp.path().join("src/Service/Worker.php"), src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.open("src/Service/Worker.php", src).await;

    let resp = s.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &s.uri(""));
    expect![[r#"
        src/Service/Worker.php
          <clean>"#]]
    .assert_eq(&out);
}
