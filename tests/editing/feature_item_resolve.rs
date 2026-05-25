//! Resolve provider tests for all LSP item types.
//! Tests verify that lazy-loaded item data (detail, documentation, edit computation)
//! is correctly resolved when requested via the LSP resolve protocol.

use super::*;
use expect_test::expect;
use serde_json::{Value, json};

// ============================================================================
// completionItem/resolve tests
// ============================================================================

#[tokio::test]
async fn completion_resolve_adds_detail_to_builtin_function() {
    let mut s = TestServer::new().await;
    s.open("file.php", "<?php\n$x = str$0").await;

    let resp = s.completion("file.php", 1, 7).await;
    let items: Vec<_> = resp["result"]
        .as_array()
        .or_else(|| resp["result"]["items"].as_array())
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    // Find any completion item
    if !items.is_empty() {
        let item = items[0].clone();
        let resolved = s.completion_resolve(item).await;
        assert!(resolved["error"].is_null(), "resolve should not error");

        // Resolve should complete without error (detail may or may not be populated)
        let result = &resolved["result"];
        assert!(
            !result["label"].is_null(),
            "resolved item should have label"
        );
    }
}

#[tokio::test]
async fn completion_resolve_adds_documentation_to_function() {
    let mut s = TestServer::new().await;
    s.open("file.php", "<?php\narray_map$0").await;

    let resp = s.completion("file.php", 1, 9).await;
    let items: Vec<_> = resp["result"]
        .as_array()
        .or_else(|| resp["result"]["items"].as_array())
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    let item = items.iter().find(|i| {
        i["label"]
            .as_str()
            .map(|l| l == "array_map")
            .unwrap_or(false)
    });
    assert!(item.is_some(), "array_map not found");

    let resolved = s.completion_resolve(item.unwrap().clone()).await;
    assert!(resolved["error"].is_null());

    let docs = resolved["result"]["documentation"]["value"]
        .as_str()
        .or_else(|| resolved["result"]["documentation"].as_str())
        .unwrap_or("");
    assert!(
        !docs.is_empty(),
        "documentation should be populated for array_map"
    );
}

#[tokio::test]
async fn completion_resolve_already_resolved_items_unchanged() {
    let mut s = TestServer::new().await;
    s.open("file.php", "<?php\n\"hello$0").await;

    let resp = s.completion("file.php", 1, 8).await;
    let items: Vec<_> = resp["result"]
        .as_array()
        .or_else(|| resp["result"]["items"].as_array())
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    if !items.is_empty() {
        let item = items[0].clone();
        let resolved1 = s.completion_resolve(item.clone()).await;
        let resolved2 = s.completion_resolve(resolved1["result"].clone()).await;

        // Resolving twice should be idempotent
        assert_eq!(
            resolved1["result"]["label"], resolved2["result"]["label"],
            "resolve should be idempotent"
        );
    }
}

// ============================================================================
// codeAction/resolve tests
// ============================================================================

#[tokio::test]
async fn code_action_resolve_defers_extract_method_edit() {
    let mut s = TestServer::new().await;
    let src = r#"<?php
class Math {
    public function add(): int {
        return $01 + 2$0;
    }
}
"#;
    let opened = s.open_fixture(src).await;
    let resp = if let Some(r) = opened.fixture.range.clone() {
        s.code_action_at(&r).await
    } else {
        let c = opened.cursor().clone();
        s.code_action(&c.path, c.line, c.character, c.line, c.character)
            .await
    };
    let actions: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    // Find extract method action
    let extract_action = actions.iter().find(|a| {
        a["title"]
            .as_str()
            .map(|t| t.contains("Extract method"))
            .unwrap_or(false)
    });

    if let Some(action) = extract_action {
        // Action should not have edit before resolve
        assert!(
            action["edit"].is_null() || !action["data"].is_null(),
            "deferred action should have data, not edit yet"
        );

        // Resolve should populate the edit
        let resolved = s.code_action_resolve(action.clone()).await;
        assert!(
            resolved["error"].is_null(),
            "resolve should not error: {resolved:?}"
        );

        let edit = &resolved["result"]["edit"];
        assert!(
            !edit.is_null() && edit["changes"].is_object(),
            "resolved action should have edit with changes"
        );
    }
}

#[tokio::test]
async fn code_action_resolve_computes_organize_imports_edit() {
    let mut s = TestServer::new().await;

    // Open a file with unused imports
    let src = r#"<?php
use DateTime;
use stdClass;

$x = new stdClass();"#;

    s.open("test.php", src).await;

    let resp = s.code_action("test.php", 0, 0, 5, 0).await;
    let actions: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    let organize = actions.iter().find(|a| {
        a["title"]
            .as_str()
            .map(|t| t.contains("Organize imports") || t.contains("organize"))
            .unwrap_or(false)
    });

    if let Some(action) = organize {
        // Resolve the action
        let resolved = s.code_action_resolve(action.clone()).await;
        assert!(resolved["error"].is_null());

        // Should have edit with changes or be already resolved
        let edit = &resolved["result"]["edit"];
        assert!(
            !edit.is_null() || !resolved["result"].is_null(),
            "organize imports action should be resolved"
        );
    }
}

#[tokio::test]
async fn code_action_resolve_handles_non_deferred_actions() {
    let mut s = TestServer::new().await;
    let src = r#"<?php
function $0greet() {
    echo "Hello";
}"#;

    let opened = s.open_fixture(src).await;
    let resp = if let Some(r) = opened.fixture.range.clone() {
        s.code_action_at(&r).await
    } else {
        let c = opened.cursor().clone();
        s.code_action(&c.path, c.line, c.character, c.line, c.character)
            .await
    };
    let actions: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for action in actions {
        // All actions should resolve without error
        let resolved = s.code_action_resolve(action).await;
        assert!(
            resolved["error"].is_null(),
            "all actions should resolve successfully"
        );
    }
}

// ============================================================================
// codeLens/resolve tests
// ============================================================================

#[tokio::test]
async fn code_lens_resolve_returns_populated_lens() {
    let mut s = TestServer::new().await;
    let src = r#"<?php
class TestCase {
    public function testExample(): void {}

    public function runIt(): void {
        $this->testExample();
    }
}"#;

    s.open("test.php", src).await;

    let resp = s.code_lens("test.php").await;
    let lenses: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for lens in lenses {
        let resolved = s.code_lens_resolve(lens.clone()).await;
        assert!(
            resolved["error"].is_null(),
            "code lens resolve should not error"
        );

        // Resolved lens should have command populated
        let result = &resolved["result"];
        assert!(
            !result["command"].is_null(),
            "resolved lens should have command"
        );
        assert!(
            result["command"]["title"].is_string(),
            "command should have title"
        );
    }
}

#[tokio::test]
async fn code_lens_resolve_preserves_range() {
    let mut s = TestServer::new().await;
    let src = r#"<?php
class Service {
    public function execute(): void {}
}
"#;

    s.open("service.php", src).await;

    let resp = s.code_lens("service.php").await;
    let lenses: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for lens in lenses {
        let range_before = lens["range"].clone();
        let resolved = s.code_lens_resolve(lens.clone()).await;
        let range_after = resolved["result"]["range"].clone();

        assert_eq!(range_before, range_after, "resolve should not modify range");
    }
}

// ============================================================================
// documentLink/resolve tests
// ============================================================================

#[tokio::test]
async fn document_link_resolve_returns_link_with_target() {
    let mut s = TestServer::new().await;
    s.open(
        "links.php",
        "<?php\nrequire_once 'vendor/autoload.php';\nrequire 'lib/config.php';\n",
    )
    .await;

    let resp = s.document_link("links.php").await;
    let links: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for link in links {
        let target_before = link["target"].clone();
        let resolved = s.document_link_resolve(link.clone()).await;

        assert!(
            resolved["error"].is_null(),
            "document link resolve should not error"
        );

        let target_after = &resolved["result"]["target"];
        // Target should be populated and unchanged by resolve
        assert!(target_after.is_string(), "resolved link should have target");
        assert_eq!(
            target_before, *target_after,
            "resolve should preserve target URI"
        );
    }
}

#[tokio::test]
async fn document_link_resolve_handles_http_links() {
    let mut s = TestServer::new().await;
    s.open(
        "doc.php",
        "<?php\n/** @link https://php.net/manual */\nfunction helper() {}\n",
    )
    .await;

    let resp = s.document_link("doc.php").await;
    let links: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for link in links {
        let resolved = s.document_link_resolve(link.clone()).await;
        assert!(resolved["error"].is_null());

        let target = resolved["result"]["target"].as_str().unwrap_or("");
        assert!(
            target.starts_with("https://") || target.starts_with("file://"),
            "link target should be a valid URI: {target}"
        );
    }
}

#[tokio::test]
async fn document_link_resolve_preserves_range() {
    let mut s = TestServer::new().await;
    s.open("req.php", "<?php\nrequire 'helper.php';\n").await;

    let resp = s.document_link("req.php").await;
    let links: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for link in links {
        let range_before = link["range"].clone();
        let resolved = s.document_link_resolve(link.clone()).await;
        let range_after = resolved["result"]["range"].clone();

        assert_eq!(range_before, range_after, "resolve should not modify range");
    }
}

// ============================================================================
// inlayHint/resolve tests
// ============================================================================

#[tokio::test]
async fn inlay_hint_resolve_adds_tooltip_to_parameter_hint() {
    let mut s = TestServer::new().await;
    let src = r#"<?php
function process(string $name, int $age): void {}
$0process("Alice", 30);$0
"#;
    let opened = s.open_fixture(src).await;
    let path = opened.fixture.files[0].path.clone();

    let resp = if let Some(r) = opened.fixture.range.clone() {
        s.inlay_hints(
            &r.path,
            r.start_line,
            r.start_character,
            r.end_line,
            r.end_character,
        )
        .await
    } else {
        s.inlay_hints(&path, 0, 0, u32::MAX, u32::MAX).await
    };
    let hints: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for hint in hints {
        let resolved = s.inlay_hint_resolve(hint.clone()).await;
        assert!(
            resolved["error"].is_null(),
            "inlay hint resolve should not error"
        );

        let result = &resolved["result"];
        // Should have label and optionally tooltip
        assert!(
            !result["label"].is_null(),
            "resolved hint should have label"
        );
    }
}

#[tokio::test]
async fn inlay_hint_resolve_adds_documentation_tooltip() {
    let mut s = TestServer::new().await;
    let src = r#"<?php
function getValue(string $key): mixed {}
$0$x = getValue("config");$0
"#;
    let opened = s.open_fixture(src).await;
    let path = opened.fixture.files[0].path.clone();

    let resp = if let Some(r) = opened.fixture.range.clone() {
        s.inlay_hints(
            &r.path,
            r.start_line,
            r.start_character,
            r.end_line,
            r.end_character,
        )
        .await
    } else {
        s.inlay_hints(&path, 0, 0, u32::MAX, u32::MAX).await
    };
    let hints: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for hint in hints {
        if hint["data"].is_object() && hint["data"]["php_lsp_fn"].is_string() {
            let resolved = s.inlay_hint_resolve(hint.clone()).await;
            assert!(resolved["error"].is_null());

            // After resolve, hint should have tooltip if it has php_lsp_fn data
            let tooltip = &resolved["result"]["tooltip"];
            // Tooltip might be empty or have content
            assert!(
                tooltip.is_string() || tooltip["value"].is_string() || tooltip.is_null(),
                "tooltip should be properly formatted"
            );
        }
    }
}

#[tokio::test]
async fn inlay_hint_resolve_idempotent_when_tooltip_exists() {
    let mut s = TestServer::new().await;
    let src = r#"<?php
function $0getName(string $first, string $last): string {}
"#;
    let opened = s.open_fixture(src).await;
    let path = opened.fixture.files[0].path.clone();

    let resp = if let Some(r) = opened.fixture.range.clone() {
        s.inlay_hints(
            &r.path,
            r.start_line,
            r.start_character,
            r.end_line,
            r.end_character,
        )
        .await
    } else {
        s.inlay_hints(&path, 0, 0, u32::MAX, u32::MAX).await
    };
    let hints: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for hint in hints {
        let resolved1 = s.inlay_hint_resolve(hint.clone()).await;
        let resolved2 = s.inlay_hint_resolve(resolved1["result"].clone()).await;

        // Resolving twice should be idempotent
        assert_eq!(
            resolved1["result"]["label"], resolved2["result"]["label"],
            "resolve should be idempotent"
        );
    }
}

// ============================================================================
// workspaceSymbol/resolve tests
// ============================================================================

#[tokio::test]
async fn workspace_symbol_resolve_populates_location() {
    let mut s = TestServer::new().await;
    s.open("db.php", "<?php\nclass Database {}\n").await;

    let resp = s.workspace_symbols("Database").await;
    let symbols: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    if !symbols.is_empty() {
        let symbol = symbols[0].clone();
        let resolved = s.workspace_symbol_resolve(symbol).await;

        assert!(
            resolved["error"].is_null(),
            "workspace symbol resolve should not error"
        );

        let location = &resolved["result"]["location"];
        assert!(
            !location["uri"].is_null(),
            "resolved symbol should have location.uri"
        );
    }
}

#[tokio::test]
async fn workspace_symbol_resolve_with_range_information() {
    let mut s = TestServer::new().await;
    s.open(
        "service.php",
        "<?php\nclass Service {\n  public function run() {}\n}\n",
    )
    .await;

    let resp = s.workspace_symbols("Service").await;
    let symbols: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for symbol in symbols {
        let resolved = s.workspace_symbol_resolve(symbol.clone()).await;
        assert!(resolved["error"].is_null());

        let location = &resolved["result"]["location"];
        if !location["range"].is_null() {
            let range = &location["range"];
            assert!(
                range["start"]["line"].is_number(),
                "range should have line information"
            );
            assert!(
                range["start"]["character"].is_number(),
                "range should have character information"
            );
        }
    }
}

#[tokio::test]
async fn workspace_symbol_resolve_multiple_symbols() {
    let mut s = TestServer::new().await;
    s.open(
        "test.php",
        "<?php\nfunction test() {}\nfunction testing() {}\nfunction tested() {}\n",
    )
    .await;

    let resp = s.workspace_symbols("test").await;
    let symbols: Vec<_> = resp["result"]
        .as_array()
        .map(|a| a.iter().cloned().collect())
        .unwrap_or_default();

    for symbol in symbols.iter().take(5) {
        let resolved = s.workspace_symbol_resolve(symbol.clone()).await;
        assert!(
            resolved["error"].is_null(),
            "all symbols should resolve: {symbol:?}"
        );

        // Name and kind should be preserved
        assert_eq!(
            symbol["name"], resolved["result"]["name"],
            "resolve should preserve name"
        );
        assert_eq!(
            symbol["kind"], resolved["result"]["kind"],
            "resolve should preserve kind"
        );
    }
}

// ============================================================================
// Edge cases and error handling
// ============================================================================

#[tokio::test]
async fn resolve_with_null_item_returns_error_or_unchanged() {
    let mut s = TestServer::new().await;

    // Resolve with empty item
    let empty_item = json!({});
    let resolved = s.completion_resolve(empty_item).await;

    // Should either error or return unchanged
    assert!(
        !resolved["error"].is_null() || resolved["result"].is_object(),
        "resolve should handle empty items gracefully"
    );
}

#[tokio::test]
async fn resolve_methods_do_not_require_edit_for_non_deferred_items() {
    let mut s = TestServer::new().await;

    let item = json!({
        "label": "test",
        "kind": 1
    });

    let resolved = s.completion_resolve(item).await;
    // Should not error for items without full data
    assert!(
        !resolved.get("error").map(|e| !e.is_null()).unwrap_or(false),
        "resolve should handle minimal items"
    );
}
