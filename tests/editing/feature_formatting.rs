//! Text formatting: document formatting, range formatting, and on-type formatting.

use super::*;

#[tokio::test]
async fn formatting_returns_null_or_valid_edits() {
    let mut server = TestServer::new().await;
    server
        .open("fmt.php", "<?php\nfunction ugly( $x ){return $x;}\n")
        .await;

    let resp = server.formatting("fmt.php").await;

    assert!(resp["error"].is_null(), "formatting error: {:?}", resp);
    match resp["result"].as_array() {
        None => assert!(
            resp["result"].is_null(),
            "expected null (no formatter) or TextEdit array, got: {:?}",
            resp["result"]
        ),
        Some(edits) => {
            assert!(!edits.is_empty(), "formatter returned empty edit array");
            for edit in edits {
                assert!(
                    edit["range"].is_object(),
                    "TextEdit missing 'range': {:?}",
                    edit
                );
                assert!(
                    edit["newText"].is_string(),
                    "TextEdit missing 'newText': {:?}",
                    edit
                );
            }
        }
    }
}

#[tokio::test]
async fn range_formatting_returns_null_or_valid_edits() {
    let mut server = TestServer::new().await;
    server
        .open("rfmt.php", "<?php\nfunction ugly( $x ){return $x;}\n")
        .await;

    let resp = server.range_formatting("rfmt.php", 0, 0, 2, 0).await;

    assert!(resp["error"].is_null(), "rangeFormatting error: {:?}", resp);
    match resp["result"].as_array() {
        None => assert!(
            resp["result"].is_null(),
            "expected null (no formatter) or TextEdit array, got: {:?}",
            resp["result"]
        ),
        Some(edits) => {
            assert!(!edits.is_empty(), "formatter returned empty edit array");
            for edit in edits {
                assert!(
                    edit["range"].is_object(),
                    "TextEdit missing 'range': {:?}",
                    edit
                );
                assert!(
                    edit["newText"].is_string(),
                    "TextEdit missing 'newText': {:?}",
                    edit
                );
            }
        }
    }
}

/// Unknown trigger characters must return null — the handler only supports `}` and `\n`.
#[tokio::test]
async fn on_type_formatting_unknown_trigger_returns_null() {
    let mut server = TestServer::new().await;
    server.open("otfmt.php", "<?php\nif (true) {\n").await;

    let resp = server.on_type_formatting("otfmt.php", 1, 10, "{").await;

    assert!(
        resp["error"].is_null(),
        "onTypeFormatting error: {:?}",
        resp
    );
    assert!(
        resp["result"].is_null(),
        "expected null for unhandled trigger '{{', got: {:?}",
        resp["result"]
    );
}

/// The `}` trigger is handled in-process (no external tool needed) and must
/// de-indent the closing brace to match the indentation of the opening `{`.
#[tokio::test]
async fn on_type_formatting_close_brace_deindents() {
    let mut server = TestServer::new().await;
    server
        .open("otfmt2.php", "<?php\nif (true) {\n    }\n")
        .await;

    let resp = server.on_type_formatting("otfmt2.php", 2, 4, "}").await;

    assert!(
        resp["error"].is_null(),
        "onTypeFormatting error: {:?}",
        resp
    );
    let edits = resp["result"]
        .as_array()
        .expect("} trigger must produce a TextEdit array");
    assert_eq!(edits.len(), 1, "expected exactly one de-indent edit");

    let edit = &edits[0];
    assert_eq!(
        edit["range"]["start"],
        serde_json::json!({"line": 2, "character": 0}),
        "edit start must be at line 2, character 0"
    );
    assert_eq!(
        edit["range"]["end"],
        serde_json::json!({"line": 2, "character": 4}),
        "edit end must be at line 2, character 4 (replacing 4-space indent)"
    );
    assert_eq!(
        edit["newText"].as_str().unwrap(),
        "",
        "newText must be empty (de-indent to column 0)"
    );
}

/// Close brace in nested block must align to the inner `if` indent.
#[tokio::test]
async fn on_type_formatting_close_brace_nested_block() {
    let mut server = TestServer::new().await;
    server
        .open(
            "otfmt_nested.php",
            "<?php\nif (true) {\n    if (false) {\n        }\n}\n",
        )
        .await;

    let resp = server
        .on_type_formatting("otfmt_nested.php", 3, 8, "}")
        .await;

    assert!(
        resp["error"].is_null(),
        "onTypeFormatting error: {:?}",
        resp
    );
    let edits = resp["result"]
        .as_array()
        .expect("} trigger must produce a TextEdit array");
    assert_eq!(edits.len(), 1, "expected exactly one edit for nested block");

    let edit = &edits[0];
    assert_eq!(
        edit["range"]["start"],
        serde_json::json!({"line": 3, "character": 0}),
        "edit must start at column 0"
    );
    assert_eq!(
        edit["range"]["end"],
        serde_json::json!({"line": 3, "character": 8}),
        "edit must replace 8-space indent"
    );
    assert_eq!(
        edit["newText"].as_str().unwrap(),
        "    ",
        "newText must be 4-space indent (matching inner if)"
    );
}

/// Close brace already at correct indent produces no edits.
#[tokio::test]
async fn on_type_formatting_close_brace_already_aligned() {
    let mut server = TestServer::new().await;
    server
        .open("otfmt_aligned.php", "<?php\nif (true) {\n}\n")
        .await;

    let resp = server
        .on_type_formatting("otfmt_aligned.php", 2, 0, "}")
        .await;

    assert!(
        resp["error"].is_null(),
        "onTypeFormatting error: {:?}",
        resp
    );
    let result = &resp["result"];
    assert!(
        result.is_null(),
        "no edit needed when already aligned; expected null, got: {:?}",
        result
    );
}

/// Range formatting with a single-line range.
#[tokio::test]
async fn range_formatting_single_line_range() {
    let mut server = TestServer::new().await;
    server
        .open(
            "rfmt_single.php",
            "<?php\nfunction ugly( $x ){return $x;}\n",
        )
        .await;

    let resp = server
        .range_formatting("rfmt_single.php", 1, 0, 1, 38)
        .await;

    assert!(resp["error"].is_null(), "rangeFormatting error: {:?}", resp);
    match resp["result"].as_array() {
        None => assert!(
            resp["result"].is_null(),
            "expected null (no formatter) or TextEdit array, got: {:?}",
            resp["result"]
        ),
        Some(edits) => {
            for edit in edits {
                let start_line = edit["range"]["start"]["line"].as_u64().unwrap_or(0);
                let end_line = edit["range"]["end"]["line"].as_u64().unwrap_or(0);
                assert!(
                    start_line >= 1 && end_line <= 1,
                    "all edits should be within the specified range (line 1)"
                );
            }
        }
    }
}

/// Range formatting covering the entire file.
#[tokio::test]
async fn range_formatting_entire_file_range() {
    let mut server = TestServer::new().await;
    server
        .open("rfmt_all.php", "<?php\nfunction ugly( $x ){return $x;}\n\n")
        .await;

    let resp = server.range_formatting("rfmt_all.php", 0, 0, 3, 0).await;

    assert!(resp["error"].is_null(), "rangeFormatting error: {:?}", resp);
    match resp["result"].as_array() {
        None => assert!(
            resp["result"].is_null(),
            "expected null (no formatter) or TextEdit array, got: {:?}",
            resp["result"]
        ),
        Some(edits) => {
            assert!(
                !edits.is_empty(),
                "when formatter is available, whole-file range should produce edits"
            );
            for edit in edits {
                assert!(
                    edit["range"].is_object(),
                    "TextEdit missing 'range': {:?}",
                    edit
                );
                assert!(
                    edit["newText"].is_string(),
                    "TextEdit missing 'newText': {:?}",
                    edit
                );
            }
        }
    }
}
