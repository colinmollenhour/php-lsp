//! Semantic token coverage: full, range, delta, and delta-fallback cases.

use super::*;
use expect_test::expect;

async fn get_legend_types(init_resp: &serde_json::Value) -> Vec<&str> {
    init_resp["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect()
}

#[tokio::test]
async fn semantic_tokens_full_returned() {
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    let out = server
        .check_semantic_tokens_full(
            "<?php\nfunction tokenized(int $x): int { return $x; }\n",
            &legend_types,
        )
        .await;

    expect![[r#"
        1:9 len=9 type=function mods=0b1
        1:19 len=3 type=type mods=0b0
        1:23 len=2 type=parameter mods=0b1
        1:28 len=3 type=type mods=0b0
        1:41 len=2 type=variable mods=0b0"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn semantic_tokens_range_returns_data() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "st_range.php",
            "<?php\nfunction ranged(int $x): int { return $x; }\n",
        )
        .await;

    // Request semanticTokens/range from the already-open file (not via check_semantic_tokens_range
    // which would try to reopen the "st_range.php" string as PHP source code).
    let resp = server
        .semantic_tokens_range("st_range.php", 0, 0, 2, 0)
        .await;

    let out = render_semantic_tokens(&resp, &legend_types);
    // Range request from line 0-2 includes all tokens (whole file, the function on line 1)
    expect![[r#"
        1:9 len=6 type=function mods=0b1
        1:16 len=3 type=type mods=0b0
        1:20 len=2 type=parameter mods=0b1
        1:25 len=3 type=type mods=0b0
        1:38 len=2 type=variable mods=0b0"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn semantic_tokens_full_delta_returns_result() {
    let mut server = TestServer::new().await;
    server
        .open(
            "st_delta.php",
            "<?php\nfunction delta(int $x): int { return $x; }\n",
        )
        .await;

    let full = server.semantic_tokens_full("st_delta.php").await;
    assert!(
        full["error"].is_null(),
        "semanticTokens/full error: {:?}",
        full
    );
    let result_id = full["result"]["resultId"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(
        !result_id.is_empty(),
        "semanticTokens/full must return a resultId to support delta requests"
    );

    let resp = server
        .semantic_tokens_full_delta("st_delta.php", &result_id)
        .await;

    assert!(resp["error"].is_null(), "delta error: {:?}", resp);

    let result = &resp["result"];
    let has_edits = result["edits"].is_array();
    let has_data = result["data"].is_array();
    expect!["true"].assert_eq(&(has_edits || has_data).to_string());
}

/// Delta request with an unknown `previousResultId` must degrade gracefully
/// to a full-token response — the server must never error out or panic when
/// the client's baseline is stale / unknown (e.g. after a server restart).
#[tokio::test]
async fn semantic_tokens_delta_with_stale_previous_result_id_degrades_to_full() {
    let mut server = TestServer::new().await;
    server
        .open(
            "st_stale.php",
            "<?php\nfunction stale(int $x): int { return $x; }\n",
        )
        .await;

    let resp = server
        .semantic_tokens_full_delta("st_stale.php", "definitely-not-a-real-id")
        .await;

    assert!(resp["error"].is_null(), "delta error: {resp:?}");
    let result = &resp["result"];
    assert!(!result.is_null(), "expected a result payload, got null");
    // Stale resultId must degrade to full response with data array
    let has_data = result["data"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    expect!["true"].assert_eq(&has_data.to_string());
}

#[tokio::test]
async fn semantic_tokens_delta_without_baseline_degrades_to_full() {
    let mut server = TestServer::new().await;
    server
        .open(
            "st_noprior.php",
            "<?php\nfunction nobaseline(): int { return 1; }\n",
        )
        .await;

    let resp = server
        .semantic_tokens_full_delta("st_noprior.php", "0")
        .await;

    assert!(resp["error"].is_null(), "delta error: {resp:?}");
    let result = &resp["result"];
    assert!(!result.is_null(), "expected a result, got null");
    // Missing baseline must degrade to full response with data array
    let has_data = result["data"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    expect!["true"].assert_eq(&has_data.to_string());
}

/// After `didChange`, requesting delta with the pre-edit resultId must reflect
/// the new content. Either an `edits` diff or a full `data` set is acceptable,
/// but the post-edit token count must exceed the pre-edit count since we added
/// an entire function.
#[tokio::test]
async fn semantic_tokens_delta_after_didchange_reflects_new_content() {
    let mut server = TestServer::new().await;
    server
        .open("st_edit.php", "<?php\nfunction one(): int { return 1; }\n")
        .await;

    let full = server.semantic_tokens_full("st_edit.php").await;
    let pre_id = full["result"]["resultId"]
        .as_str()
        .expect("resultId")
        .to_string();
    let pre_data_len = full["result"]["data"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    server
        .change(
            "st_edit.php",
            2,
            "<?php\nfunction one(): int { return 1; }\nfunction two(): int { return 2; }\n",
        )
        .await;

    let resp = server
        .semantic_tokens_full_delta("st_edit.php", &pre_id)
        .await;
    assert!(resp["error"].is_null(), "delta error: {resp:?}");
    let result = &resp["result"];

    let got_full = result["data"].is_array();
    let got_edits = result["edits"].is_array();
    let has_result = got_full || got_edits;
    expect!["true"].assert_eq(&has_result.to_string());

    if got_full {
        let post_len = result["data"].as_array().unwrap().len();
        assert!(
            post_len > pre_data_len,
            "post-edit tokens ({post_len}) must exceed pre-edit tokens ({pre_data_len})"
        );
    } else {
        let edits = result["edits"].as_array().unwrap();
        let has_data = edits
            .iter()
            .any(|e| e["data"].as_array().map(|d| !d.is_empty()).unwrap_or(false));
        assert!(
            has_data,
            "delta edits must carry new token data, got: {edits:?}"
        );
    }
}

/// Verify that semantic tokens can be decoded and contain specific token types.
/// This test decodes raw token integers and snapshots the full token stream,
/// ensuring that function declarations and parameters are properly tokenized.
#[tokio::test]
async fn semantic_tokens_decode_function_tokens() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "decode.php",
            "<?php\nfunction greet(string $name): void { echo $name; }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("decode.php").await;
    assert!(resp["error"].is_null(), "error: {resp:?}");

    let out = render_semantic_tokens(&resp, &legend_types);
    expect![[r#"
        1:9 len=5 type=function mods=0b1
        1:15 len=6 type=type mods=0b0
        1:22 len=5 type=parameter mods=0b1
        1:30 len=4 type=type mods=0b0
        1:42 len=5 type=variable mods=0b0"#]]
    .assert_eq(&out);
}

/// Verify that `semanticTokens/range` request respects range boundaries.
/// LSP range is [start_line:start_char, end_line:end_char). This test requests
/// from line 1 char 0 to line 2 char 0 (exclusive end), which captures line 1 only.
/// In a two-function file, only the first function's tokens are returned.
#[tokio::test]
async fn semantic_tokens_range_bounds_respected() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    let src = "<?php\nfunction one(): int { return 1; }\nfunction two(): int { return 2; }\n";
    server.open("range.php", src).await;

    // Request semanticTokens/range for line 1 only (range is [start, end) exclusive end).
    let resp = server.semantic_tokens_range("range.php", 1, 0, 2, 0).await;
    assert!(resp["error"].is_null(), "error: {resp:?}");

    let out = render_semantic_tokens(&resp, &legend_types);
    // Only line 1 tokens returned; line 2 (second function) is excluded
    expect![[r#"
        1:9 len=3 type=function mods=0b1
        1:16 len=3 type=type mods=0b0
        1:29 len=1 type=number mods=0b0"#]]
    .assert_eq(&out);
}

/// Verify that deprecated functions get the `deprecated` modifier.
#[tokio::test]
async fn semantic_tokens_deprecated_function_modifier() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "deprecated.php",
            "<?php\n/** @deprecated */ function old(): void {}\n",
        )
        .await;

    let resp = server.semantic_tokens_full("deprecated.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // deprecated modifier is bit 4 (value 16 = 0b10000)
    expect![[r#"
        1:0 len=18 type=comment mods=0b0
        1:28 len=3 type=function mods=0b10001
        1:35 len=4 type=type mods=0b0"#]]
    .assert_eq(&out);
}

/// Verify that classes, interfaces, and enums are properly tokenized.
/// Classes and enums use type=class, while interfaces use type=interface.
#[tokio::test]
async fn semantic_tokens_class_interface_enum() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "types.php",
            "<?php\nclass C {}\ninterface I {}\nenum E {}\n",
        )
        .await;

    let resp = server.semantic_tokens_full("types.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Classes and enums use type=class; interfaces use type=interface
    // All have declaration modifier (0b1)
    expect![[r#"
        1:6 len=1 type=class mods=0b1
        2:10 len=1 type=interface mods=0b1
        3:5 len=1 type=class mods=0b1"#]]
    .assert_eq(&out);
}

/// Verify that static methods are marked with the `static` modifier.
/// Static methods have declaration (bit 0) + static (bit 1) = 0b11 = 3
#[tokio::test]
async fn semantic_tokens_static_method_modifier() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "static.php",
            "<?php\nclass C { static function m(): void {} }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("static.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Modifiers: declaration=bit 0, static=bit 1, so static method = 0b11
    expect![[r#"
        1:6 len=1 type=class mods=0b1
        1:26 len=1 type=method mods=0b11
        1:31 len=4 type=type mods=0b0"#]]
    .assert_eq(&out);
}

/// Verify that empty files return empty token data (not an error).
#[tokio::test]
async fn semantic_tokens_empty_file() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server.open("empty.php", "<?php\n").await;

    let resp = server.semantic_tokens_full("empty.php").await;
    assert!(resp["error"].is_null(), "error: {resp:?}");

    let out = render_semantic_tokens(&resp, &legend_types);
    expect!["<no tokens>"].assert_eq(&out);
}

/// Verify that parse errors don't break semantic token reporting.
#[tokio::test]
async fn semantic_tokens_with_parse_error() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open("broken.php", "<?php\nfunction broken(;\n")
        .await;

    let resp = server.semantic_tokens_full("broken.php").await;
    // Parse errors should not cause a protocol error
    assert!(resp["error"].is_null(), "error: {resp:?}");
    let out = render_semantic_tokens(&resp, &legend_types);
    // Should have some tokens despite the parse error
    assert!(
        !out.is_empty() && !out.contains("malformed"),
        "expected tokens even with parse error, got: {out}"
    );
}

/// Verify that class properties are tokenized as property type.
#[tokio::test]
async fn semantic_tokens_class_properties() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "props.php",
            "<?php\nclass C { public string $name; private int $age; }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("props.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Properties should be tokenized with type=property
    expect![[r#"
        1:6 len=1 type=class mods=0b1
        1:17 len=6 type=type mods=0b0
        1:25 len=4 type=property mods=0b1
        1:39 len=3 type=type mods=0b0
        1:44 len=3 type=property mods=0b1"#]]
    .assert_eq(&out);
}

/// Verify that enum declarations and cases are tokenized properly.
/// Enum cases are tokenized as `type=property` (similar to class properties).
#[tokio::test]
async fn semantic_tokens_enum_cases() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "enums.php",
            "<?php\nenum Status { case Pending; case Active; }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("enums.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Enum declaration (Status) and cases (Pending, Active) are tokenized.
    // Cases use type=property (declaration modifier 0b1).
    expect![[r#"
        1:5 len=6 type=class mods=0b1
        1:19 len=7 type=property mods=0b1
        1:33 len=6 type=property mods=0b1"#]]
    .assert_eq(&out);
}

/// Verify that traits are tokenized as class type.
#[tokio::test]
async fn semantic_tokens_traits() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "traits.php",
            "<?php\ntrait Logger { public function log(): void {} }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("traits.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Traits use type=class, methods use type=method
    expect![[r#"
        1:6 len=6 type=class mods=0b1
        1:31 len=3 type=method mods=0b1
        1:38 len=4 type=type mods=0b0"#]]
    .assert_eq(&out);
}

/// Verify that readonly properties are tokenized.
/// NOTE: This test verifies that readonly properties are recognized as properties,
/// but does NOT verify the exact modifier value since the server may not distinguish
/// readonly properties with a specific modifier bit.
#[tokio::test]
async fn semantic_tokens_readonly_property() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "readonly.php",
            "<?php\nclass C { readonly string $value; }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("readonly.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Verify readonly property is tokenized as a property
    // The exact modifier value may vary depending on how the server handles readonly
    expect![[r#"
        1:6 len=1 type=class mods=0b1
        1:19 len=6 type=type mods=0b0
        1:27 len=5 type=property mods=0b1"#]]
    .assert_eq(&out);
}

/// Verify that abstract methods are tokenized with declaration modifier.
#[tokio::test]
async fn semantic_tokens_abstract_method() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "abstract.php",
            "<?php\nabstract class Base { abstract function process(): void; }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("abstract.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Abstract class and method should have declaration modifier
    expect![[r#"
        1:15 len=4 type=class mods=0b1
        1:40 len=7 type=method mods=0b101
        1:51 len=4 type=type mods=0b0"#]]
    .assert_eq(&out);
}

/// Verify that union types are tokenized correctly.
/// Union type syntax (int|string, int|null) should tokenize both types.
#[tokio::test]
async fn semantic_tokens_union_types() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "union.php",
            "<?php\nfunction process(int|string $value): int|null {}\n",
        )
        .await;

    let resp = server.semantic_tokens_full("union.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Union types should tokenize each type in the union
    // The | operator may or may not be tokenized separately
    expect![[r#"
        1:9 len=7 type=function mods=0b1
        1:17 len=3 type=type mods=0b0
        1:21 len=6 type=type mods=0b0
        1:28 len=6 type=parameter mods=0b1
        1:37 len=3 type=type mods=0b0
        1:41 len=4 type=type mods=0b0"#]]
    .assert_eq(&out);
}

/// Verify that zero-width range requests work correctly.
#[tokio::test]
async fn semantic_tokens_zero_width_range() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open("zero.php", "<?php\nfunction test(): void {}\n")
        .await;

    // Request a zero-width range (start == end)
    let resp = server.semantic_tokens_range("zero.php", 1, 9, 1, 9).await;

    let out = render_semantic_tokens(&resp, &legend_types);
    // Zero-width range should return no tokens (or possibly empty)
    expect!["<no tokens>"].assert_eq(&out);
}

/// Verify that final methods are marked with declaration modifier.
#[tokio::test]
async fn semantic_tokens_final_method() {
    use common::render_semantic_tokens;
    use serde_json::json;

    let (mut server, init_resp) = TestServer::new_with_options(json!({
        "diagnostics": { "enabled": true }
    }))
    .await;
    let legend_types = get_legend_types(&init_resp).await;

    server
        .open(
            "final.php",
            "<?php\nclass C { final function lock(): void {} }\n",
        )
        .await;

    let resp = server.semantic_tokens_full("final.php").await;
    let out = render_semantic_tokens(&resp, &legend_types);
    // Final method should have declaration modifier (bit 0)
    expect![[r#"
        1:6 len=1 type=class mods=0b1
        1:25 len=4 type=method mods=0b1
        1:33 len=4 type=type mods=0b0"#]]
    .assert_eq(&out);
}
