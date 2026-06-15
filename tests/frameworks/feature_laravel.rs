//! Protocol-wired regression tests for all major LSP features against the real
//! Laravel framework corpus (~1 600 PHP files, ~2 900 total with types stubs).
//!
//! Each test exercises one feature area end-to-end through the wire protocol:
//! workspace scan → indexReady → open file → LSP request.  Running independently
//! (separate `TestServer`) prevents cross-test interference and timeouts from
//! cascading.
//!
//! # Setup
//!
//! ```bash
//! scripts/setup_laravel_fixture.sh
//! ```
//!
//! # Running
//!
//! ```bash
//! cargo test --test frameworks laravel -- --ignored --nocapture
//! ```
//!
//! The tests are `#[ignore]` so they don't run in CI by default — the Laravel
//! fixture is large and lives outside the normal test fixtures.

use super::*;
use expect_test::expect;

// ── Fixture constants ─────────────────────────────────────────────────────────

/// Root of the Laravel framework source tree (the `src/` subtree).
const LARAVEL_SRC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/benches/fixtures/laravel/src");

fn laravel_available() -> bool {
    std::path::Path::new(LARAVEL_SRC)
        .join("Illuminate/Auth/AuthManager.php")
        .exists()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(LARAVEL_SRC).join(rel))
        .unwrap_or_else(|e| panic!("cannot read {rel}: {e}"))
}

// ── Go to Definition ──────────────────────────────────────────────────────────

/// GoToDef for a class name on its own declaration line resolves to itself.
/// Guards against "definition on declaration returns null" regression.
#[ignore]
#[tokio::test]
async fn laravel_definition_class_declaration() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 17 (0-based) = `class AuthManager implements FactoryContract`
    // Character 6 = start of "AuthManager".
    let resp = s.definition("Illuminate/Auth/AuthManager.php", 17, 6).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["Illuminate/Auth/AuthManager.php:17:6-17:17"].assert_eq(&out);
}

/// GoToDef on a method declaration resolves to that method's own range.
/// Guards against same-file method definition returning null.
#[ignore]
#[tokio::test]
async fn laravel_definition_method_declaration() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 69 (0-based) = `    public function guard($name = null)`
    // Character 20 = start of "guard".
    let resp = s
        .definition("Illuminate/Auth/AuthManager.php", 69, 20)
        .await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["Illuminate/Auth/AuthManager.php:69:20-69:25"].assert_eq(&out);
}

/// GoToDef on a cross-file import (`use ... as`) resolves into the target file.
/// Guards against cross-file navigation returning null after a workspace scan.
#[ignore]
#[tokio::test]
async fn laravel_definition_cross_file_use_import() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 5 (0-based) = `use Illuminate\Contracts\Auth\Factory as FactoryContract;`
    // Character 30 = start of "Factory" in the qualified name.
    let resp = s.definition("Illuminate/Auth/AuthManager.php", 5, 30).await;
    let out = render_locations(&resp, &s.uri(""));
    // Navigates into Contracts/Auth/Factory.php.
    assert!(
        out.contains("Contracts/Auth/Factory.php"),
        "expected Factory.php, got: {out}"
    );
}

/// GoToDef on an interface name in the `implements` clause navigates cross-file.
/// Guards against cross-file navigation on the implements clause returning null.
#[ignore]
#[tokio::test]
async fn laravel_definition_cross_file_implements() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 17 (0-based) = `class AuthManager implements FactoryContract`
    // Character 29 = start of "FactoryContract".
    let resp = s
        .definition("Illuminate/Auth/AuthManager.php", 17, 29)
        .await;
    let out = render_locations(&resp, &s.uri(""));
    assert!(
        out.contains("Contracts/Auth/Factory.php"),
        "expected Factory.php, got: {out}"
    );
}

// ── Hover ─────────────────────────────────────────────────────────────────────

/// Hover on a class name shows the class signature and PHPDoc.
/// Guards against class hover returning empty after index.
#[ignore]
#[tokio::test]
async fn laravel_hover_class_declaration() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 17 (0-based), character 6 = "AuthManager".
    let resp = s.hover("Illuminate/Auth/AuthManager.php", 17, 6).await;
    let out = render_hover(&resp);
    assert!(
        out.contains("AuthManager"),
        "hover should contain class name, got: {out}"
    );
    assert!(
        out.contains("```php"),
        "hover should be markdown, got: {out}"
    );
}

/// Hover on a method name shows its signature and PHPDoc summary.
/// Guards against method hover returning empty.
#[ignore]
#[tokio::test]
async fn laravel_hover_method_shows_signature_and_doc() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 69 (0-based) = `    public function guard($name = null)`
    // Character 20 = "guard".
    let resp = s.hover("Illuminate/Auth/AuthManager.php", 69, 20).await;
    let out = render_hover(&resp);
    assert!(
        out.contains("guard"),
        "hover should contain method name, got: {out}"
    );
    assert!(
        out.contains("```php"),
        "hover should be markdown, got: {out}"
    );
}

/// Hover on a property declaration shows its type.
/// Currently broken — property hover returns empty.
///
/// **Gap**: property hover not implemented; tracked to prevent silent regression.
/// When fixed, update the snapshot to assert the actual type is shown.
#[ignore]
#[tokio::test]
async fn laravel_hover_property_declaration() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 40 (0-based) = `    protected $guards = [];`
    // Character 14 = "$guards".
    let resp = s.hover("Illuminate/Auth/AuthManager.php", 40, 14).await;
    let out = render_hover(&resp);
    // Currently returns empty — snapshot documents broken state.
    expect!["<no hover>"].assert_eq(&out);
}

/// Hover on an interface name at an `implements` clause shows the interface.
#[ignore]
#[tokio::test]
async fn laravel_hover_implements_interface() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 17, character 29 = "FactoryContract".
    let resp = s.hover("Illuminate/Auth/AuthManager.php", 17, 29).await;
    let out = render_hover(&resp);
    assert!(
        out.contains("Factory") || out.contains("interface"),
        "hover on implements clause should show interface, got: {out}"
    );
}

// ── Find References ───────────────────────────────────────────────────────────

/// `Str::lower` is called from many files in the framework; references must
/// include at least 8 call sites and contain the known caller
/// `QueriesRelationships.php`.
///
/// Guards against cross-file reference discovery breaking after index changes.
#[ignore]
#[tokio::test]
async fn laravel_references_static_method_cross_file() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Support/Str.php",
        &read("Illuminate/Support/Str.php"),
    )
    .await;

    // Line 755 (0-based) = `    public static function lower($value)`
    // Character 27 = "lower".
    let resp = s
        .references("Illuminate/Support/Str.php", 755, 27, false)
        .await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let locs = resp["result"].as_array().expect("array");
    assert!(
        locs.len() >= 8,
        "expected ≥8 references to Str::lower, got {}",
        locs.len()
    );
    let uris: Vec<&str> = locs.iter().map(|l| l["uri"].as_str().unwrap()).collect();
    assert!(
        uris.iter().any(|u| u.contains("QueriesRelationships")),
        "QueriesRelationships.php missing from references: {uris:?}"
    );
}

/// References to the `guard()` method in AuthManager includes the declaration
/// and at least the intra-file self-calls.
/// Guards against method references returning empty.
#[ignore]
#[tokio::test]
async fn laravel_references_method_includes_declaration() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 69 (0-based) = `    public function guard($name = null)`
    // Character 20 = "guard".
    let resp = s
        .references("Illuminate/Auth/AuthManager.php", 69, 20, true)
        .await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let locs = resp["result"].as_array().expect("array");
    assert!(
        !locs.is_empty(),
        "expected at least the declaration in references, got 0"
    );
}

// ── Completion ────────────────────────────────────────────────────────────────

/// Completing `$this->` inside AuthManager returns the class's own members.
/// Guards against member completion returning empty after a workspace scan.
#[ignore]
#[tokio::test]
async fn laravel_completion_this_members() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 59 (0-based) = `        $this->app = $app;`
    // Character 15 = immediately after `$this->`.
    let resp = s
        .completion("Illuminate/Auth/AuthManager.php", 59, 15)
        .await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let items = resp["result"]["items"]
        .as_array()
        .or_else(|| resp["result"].as_array())
        .expect("completion items array");
    assert!(
        items.len() >= 5,
        "expected ≥5 completion items for $this->, got {}",
        items.len()
    );
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    // AuthManager has $app, $guards, $customCreators, guard(), resolve(), …
    let has_guard = labels.iter().any(|l| *l == "guard" || *l == "guard()");
    assert!(
        has_guard,
        "completion for $this-> should include 'guard', got: {labels:?}"
    );
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

/// Opening a clean Laravel framework file produces no diagnostics.
/// Guards against false-positive noise on real-world code.
#[ignore]
#[tokio::test]
async fn laravel_diagnostics_clean_file_no_noise() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    // No wait_for_index_ready — diagnostics are push-based on didOpen.
    let diag = s
        .open(
            "Illuminate/Auth/AuthManager.php",
            &read("Illuminate/Auth/AuthManager.php"),
        )
        .await;
    let empty = vec![];
    let all = diag["params"]["diagnostics"].as_array().unwrap_or(&empty);
    let errors: Vec<_> = all
        .iter()
        .filter(|d| d["severity"].as_u64() == Some(1))
        .collect();
    assert!(
        errors.is_empty(),
        "expected 0 errors in clean AuthManager.php, got: {errors:#?}"
    );
}

/// A file with a return-type mismatch produces an error diagnostic.
/// Guards against type-error diagnostics silently stopping after index.
#[ignore]
#[tokio::test]
async fn laravel_diagnostics_type_error_fires() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    // Inject a synthetic file with a known return-type violation.
    let bad = "<?php\nnamespace Illuminate\\Auth;\nfunction bad_func(): string { return 42; }\n";
    let diag = s.open("Illuminate/Auth/__test_diag.php", bad).await;
    let empty = vec![];
    let all = diag["params"]["diagnostics"].as_array().unwrap_or(&empty);
    let errors: Vec<_> = all
        .iter()
        .filter(|d| d["severity"].as_u64() == Some(1))
        .collect();
    assert!(
        !errors.is_empty(),
        "expected a type-error diagnostic for 'return 42' where string declared, got none"
    );
    let msg = errors[0]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("string") || msg.contains("Return") || msg.contains("compatible"),
        "unexpected error message: {msg}"
    );
}

/// `didChange` on an open file triggers a fresh `publishDiagnostics`.
/// Guards against diagnostic updates stalling after an edit.
#[ignore]
#[tokio::test]
async fn laravel_diagnostics_update_on_did_change() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    let clean = "<?php\nnamespace Illuminate\\Auth;\nfunction ok(): string { return 'hi'; }\n";
    s.open("Illuminate/Auth/__test_change.php", clean).await;

    // Introduce a return-type error via didChange.
    let bad = "<?php\nnamespace Illuminate\\Auth;\nfunction ok(): string { return 42; }\n";
    let diag = s.change("Illuminate/Auth/__test_change.php", 2, bad).await;
    let empty = vec![];
    let all = diag["params"]["diagnostics"].as_array().unwrap_or(&empty);
    let errors: Vec<_> = all
        .iter()
        .filter(|d| d["severity"].as_u64() == Some(1))
        .collect();
    assert!(
        !errors.is_empty(),
        "expected error after didChange introduced a type mismatch, got none"
    );
}

// ── Document Symbols ──────────────────────────────────────────────────────────

/// `documentSymbol` for AuthManager returns a hierarchical structure with the
/// class at the top and its methods and properties as children.
/// Guards against document symbols returning empty after index.
#[ignore]
#[tokio::test]
async fn laravel_document_symbols_hierarchical() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    let resp = s.document_symbols("Illuminate/Auth/AuthManager.php").await;
    let out = render_document_symbols(&resp);
    // AuthManager is the top-level class; its members appear as children.
    assert!(
        out.contains("AuthManager"),
        "document symbols should contain 'AuthManager', got: {out}"
    );
    assert!(
        out.contains("guard"),
        "document symbols should include 'guard' method, got: {out}"
    );
    // At least the class + several methods/properties.
    let line_count = out.lines().count();
    assert!(
        line_count >= 10,
        "expected ≥10 symbols (class + members), got {line_count}: {out}"
    );
}

/// `documentSymbol` for the Eloquent Model (a large file with ~200 members)
/// completes without timeout and returns all members.
/// Guards against large-file document symbol requests stalling.
#[ignore]
#[tokio::test]
async fn laravel_document_symbols_large_file() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Database/Eloquent/Model.php",
        &read("Illuminate/Database/Eloquent/Model.php"),
    )
    .await;

    let resp = s
        .document_symbols("Illuminate/Database/Eloquent/Model.php")
        .await;
    let out = render_document_symbols(&resp);
    assert!(
        out.contains("Model"),
        "document symbols should contain 'Model' class, got: {out}"
    );
    // Model.php has ~200 methods + properties; at least 50 should appear.
    let line_count = out.lines().count();
    assert!(
        line_count >= 50,
        "expected ≥50 symbols for Model.php, got {line_count}"
    );
}

// ── Workspace Symbols ─────────────────────────────────────────────────────────

/// `workspace/symbol` for "AuthManager" resolves after the index completes.
/// Guards against workspace symbol returning empty on a real codebase.
#[ignore]
#[tokio::test]
async fn laravel_workspace_symbols_class_name() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;

    let resp = s.workspace_symbols("AuthManager").await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let items = resp["result"].as_array().expect("array");
    assert!(
        !items.is_empty(),
        "expected ≥1 symbol for 'AuthManager', got 0"
    );
    let names: Vec<&str> = items.iter().filter_map(|i| i["name"].as_str()).collect();
    assert!(
        names.iter().any(|n| *n == "AuthManager"),
        "expected 'AuthManager' in results, got: {names:?}"
    );
}

/// `workspace/symbol` for "Guard" returns multiple guard-related symbols.
/// Guards against symbol search being too restrictive.
#[ignore]
#[tokio::test]
async fn laravel_workspace_symbols_partial_query() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;

    let resp = s.workspace_symbols("Guard").await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let items = resp["result"].as_array().expect("array");
    assert!(
        items.len() >= 3,
        "expected ≥3 symbols matching 'Guard', got {}",
        items.len()
    );
}

/// `workspace/symbol` for the `Str` class returns the class from Support.
/// Guards against workspace symbols missing commonly-used utility classes.
#[ignore]
#[tokio::test]
async fn laravel_workspace_symbols_str_class() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;

    let resp = s.workspace_symbols("Str").await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let items = resp["result"].as_array().expect("array");
    let uris: Vec<&str> = items
        .iter()
        .filter_map(|i| i["location"]["uri"].as_str())
        .collect();
    assert!(
        uris.iter().any(|u| u.contains("Support/Str.php")),
        "expected Support/Str.php in workspace symbols for 'Str', got: {uris:?}"
    );
}

// ── Code Actions ──────────────────────────────────────────────────────────────

/// Code actions on a class declaration line return at least one action.
/// Guards against code actions returning empty on real-world classes.
#[ignore]
#[tokio::test]
async fn laravel_code_actions_class_declaration() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 17 (0-based) = class declaration.
    let resp = s
        .code_action("Illuminate/Auth/AuthManager.php", 17, 0, 17, 50)
        .await;
    // Result may be null when all interface methods are already implemented,
    // imports are sorted, and no methods fall in the narrow range.
    // The test guards that the request completes without error or timeout.
    assert!(
        resp["error"].is_null(),
        "code action request failed: {resp:#}"
    );
}

// ── Signature Help ────────────────────────────────────────────────────────────

/// Signature help inside a function call shows the function's parameter list.
/// Guards against signature help returning no signatures on real code.
#[ignore]
#[tokio::test]
async fn laravel_signature_help_inside_call() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    // Use a self-contained synthetic file within the Laravel namespace so
    // PSR-4 resolution still applies.
    let src = "<?php\nnamespace Illuminate\\Auth;\n\
               function make_guard(string $name, array $config): string { return ''; }\n\
               $g = make_guard(\n";
    s.open("Illuminate/Auth/__test_sighel.php", src).await;

    // Line 3 (0-based) = `$g = make_guard(` — cursor after `(`.
    let resp = s
        .signature_help("Illuminate/Auth/__test_sighel.php", 3, 16)
        .await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let sigs = resp["result"]["signatures"]
        .as_array()
        .expect("signatures array");
    assert!(
        !sigs.is_empty(),
        "expected ≥1 signature inside make_guard(, got 0"
    );
    let label = sigs[0]["label"].as_str().unwrap_or("");
    assert!(
        label.contains("make_guard"),
        "signature label should contain function name, got: {label}"
    );
}

// ── Inlay Hints ───────────────────────────────────────────────────────────────

/// Inlay hints for the AuthManager method bodies are returned without timeout.
/// Guards against inlay hint requests stalling on real-world files.
#[ignore]
#[tokio::test]
async fn laravel_inlay_hints_method_bodies() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    let auth_line_count = read("Illuminate/Auth/AuthManager.php").lines().count() as u32;
    let resp = s
        .inlay_hints("Illuminate/Auth/AuthManager.php", 0, 0, auth_line_count, 0)
        .await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    // Hints may be empty for a file without inferred types, but the request must complete.
    assert!(
        !resp["result"].is_null(),
        "inlay hints response should not be null"
    );
}

// ── Rename ────────────────────────────────────────────────────────────────────

/// Renaming the `AuthManager` class at its declaration produces a workspace edit.
/// Guards against rename returning null.
#[ignore]
#[tokio::test]
async fn laravel_rename_class_declaration() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;

    // Line 17 (0-based), character 6 = "AuthManager".
    let resp = s
        .rename("Illuminate/Auth/AuthManager.php", 17, 6, "AuthManagerV2")
        .await;
    assert!(resp["error"].is_null(), "rename returned error: {resp:#}");
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "rename should return a workspace edit, got null"
    );
    // The edit must touch at least the declaration file.
    let changes = result["changes"]
        .as_object()
        .map(|o| o.len())
        .or_else(|| result["documentChanges"].as_array().map(|a| a.len()))
        .unwrap_or(0);
    assert!(changes >= 1, "rename should affect ≥1 file, got {changes}");
}

// ── Find Implementations ─────────────────────────────────────────────────────

/// `Factory::guard()` is an interface method; implementations should include
/// `AuthManager::guard()` from the concrete class.
///
/// Currently broken — returns 0 implementations.
/// **Gap**: cross-file implementation search not returning results for interface methods.
#[ignore]
#[tokio::test]
async fn laravel_find_implementations_interface_method() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Contracts/Auth/Factory.php",
        &read("Illuminate/Contracts/Auth/Factory.php"),
    )
    .await;

    // Line 12 (0-based) = `    public function guard($name = null);`
    // Character 20 = "guard".
    let resp = s
        .implementation("Illuminate/Contracts/Auth/Factory.php", 12, 20)
        .await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let locs = resp["result"].as_array().unwrap_or(&vec![]).to_vec();
    // Currently 0 — snapshot documents broken state; update when fixed.
    assert!(
        locs.is_empty(),
        "NOTE: find-implementations now returns results — update this test to assert correctness. Got: {locs:#?}"
    );
}

// ── Under-load stability ──────────────────────────────────────────────────────

/// Open multiple large files concurrently, then verify each feature still
/// responds in time.  Protects against the server request queue serializing
/// or salsa contention causing timeouts when several heavy files are open.
#[ignore]
#[tokio::test]
async fn laravel_features_stable_with_multiple_open_files() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;

    // Open four large files sequentially; each open() waits for diagnostics
    // (parse + analysis complete) before returning, so requests afterwards
    // do not race against in-progress work.
    s.open(
        "Illuminate/Auth/AuthManager.php",
        &read("Illuminate/Auth/AuthManager.php"),
    )
    .await;
    s.open(
        "Illuminate/Database/Eloquent/Model.php",
        &read("Illuminate/Database/Eloquent/Model.php"),
    )
    .await;
    s.open(
        "Illuminate/Support/Str.php",
        &read("Illuminate/Support/Str.php"),
    )
    .await;
    s.open(
        "Illuminate/Contracts/Auth/Factory.php",
        &read("Illuminate/Contracts/Auth/Factory.php"),
    )
    .await;

    // Hover must complete without timeout.
    let hover = s.hover("Illuminate/Auth/AuthManager.php", 69, 20).await;
    assert!(
        hover["error"].is_null(),
        "hover failed under load: {hover:#}"
    );
    assert!(!hover["result"].is_null(), "hover returned null under load");

    // Document symbols must complete without timeout.
    let syms = s.document_symbols("Illuminate/Auth/AuthManager.php").await;
    assert!(
        syms["error"].is_null(),
        "documentSymbol failed under load: {syms:#}"
    );

    // Completion must complete without timeout.
    let comp = s
        .completion("Illuminate/Auth/AuthManager.php", 59, 15)
        .await;
    assert!(
        comp["error"].is_null(),
        "completion failed under load: {comp:#}"
    );
    let items = comp["result"]["items"]
        .as_array()
        .or_else(|| comp["result"].as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(items >= 1, "completion returned 0 items under load");

    // Workspace symbols must complete without timeout.
    let ws = s.workspace_symbols("Guard").await;
    assert!(
        ws["error"].is_null(),
        "workspace/symbol failed under load: {ws:#}"
    );

    // References must complete without timeout (result may be small but must not panic).
    let refs = s
        .references("Illuminate/Auth/AuthManager.php", 69, 20, true)
        .await;
    assert!(
        refs["error"].is_null(),
        "references failed under load: {refs:#}"
    );
}
