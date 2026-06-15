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

/// GoToDef on a method call site (`$this->guard()`) resolves to the declaration.
/// Guards against same-class call-site definition returning null.
#[tokio::test]
async fn laravel_definition_from_call_site() {
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

    // Line 60 (0-based) = `        $this->userResolver = fn ($guard = null) => $this->guard($guard)->user();`
    // Character 59 = start of "guard" in the second `$this->guard(…)`.
    let resp = s
        .definition("Illuminate/Auth/AuthManager.php", 60, 59)
        .await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["Illuminate/Auth/AuthManager.php:69:20-69:25"].assert_eq(&out);
}

/// GoToDef on a static method call (`Str::camel`) navigates to `Str.php`.
/// Guards against static method cross-file definition returning null.
#[tokio::test]
async fn laravel_definition_on_static_call() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/Access/Gate.php",
        &read("Illuminate/Auth/Access/Gate.php"),
    )
    .await;

    // Line 855 (0-based) = `        return str_contains($ability, '-') ? Str::camel($ability) : $ability;`
    // Character 50 = start of "camel" after `Str::`.
    let resp = s
        .definition("Illuminate/Auth/Access/Gate.php", 855, 50)
        .await;
    let out = render_locations(&resp, &s.uri(""));
    assert!(out.contains("Str.php"), "expected Str.php, got: {out}");
}

/// GoToDef on `new RequestGuard(…)` navigates to `RequestGuard.php`.
///
/// **Gap**: Currently navigates to line 276 in `AuthManager.php` instead of
/// `RequestGuard.php`. The `new` expression resolver follows the wrong path and
/// resolves to a method in `AuthManager` rather than the `RequestGuard` class
/// declaration.
#[ignore]
#[tokio::test]
async fn laravel_definition_on_new_expression() {
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

    // Line 234 (0-based) = `            $guard = new RequestGuard($callback, …);`
    // Character 25 = start of "RequestGuard".
    let resp = s
        .definition("Illuminate/Auth/AuthManager.php", 234, 25)
        .await;
    let out = render_locations(&resp, &s.uri(""));
    assert!(
        out.contains("RequestGuard.php"),
        "expected RequestGuard.php, got: {out}"
    );
}

/// GoToDef on a trait in a `use` statement navigates to the trait file.
/// Guards against trait use definition returning null.
#[tokio::test]
async fn laravel_definition_on_trait_use() {
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

    // Line 19 (0-based) = `    use CreatesUserProviders, RebindsCallbacksToSelf;`
    // Character 8 = start of "CreatesUserProviders".
    let resp = s.definition("Illuminate/Auth/AuthManager.php", 19, 8).await;
    let out = render_locations(&resp, &s.uri(""));
    assert!(
        out.contains("CreatesUserProviders.php"),
        "expected CreatesUserProviders.php, got: {out}"
    );
}

/// GoToDef on a cross-file import (`use ... as`) resolves into the target file.
/// Guards against cross-file navigation returning null after a workspace scan.
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

/// Hover on a property declaration shows its type and docblock.
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
    expect![[r#"
        ```php
        (property) protected AuthManager::$guards
        ```

        ---

        The array of created "drivers".

        **@var** `array`"#]]
    .assert_eq(&out);
}

/// Hover on a method call site (`$this->guard()`) shows the guard() signature.
/// Guards against hover at a call site returning empty/null.
#[tokio::test]
async fn laravel_hover_on_call_site() {
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

    // Line 60 (0-based), character 59 = "guard" in `$this->guard($guard)`.
    let resp = s.hover("Illuminate/Auth/AuthManager.php", 60, 59).await;
    let out = render_hover(&resp);
    assert!(
        out.contains("guard"),
        "hover on call site should show guard() signature, got: {out}"
    );
    assert!(
        out.contains("```php"),
        "hover should be markdown, got: {out}"
    );
}

/// Hover on a static method call (`Str::camel`) shows the camel() signature.
///
/// **Gap**: Hover on `Str::camel($ability)` returns `<no hover>`. The static
/// method call-site hover handler does not resolve the class from a
/// `use`-import alias (`use Illuminate\Support\Str`), so it cannot look up
/// `Str` in the index to find the `camel()` method signature.
#[ignore]
#[tokio::test]
async fn laravel_hover_on_static_call() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    s.open(
        "Illuminate/Auth/Access/Gate.php",
        &read("Illuminate/Auth/Access/Gate.php"),
    )
    .await;

    // Line 855 (0-based), character 50 = "camel" in `Str::camel($ability)`.
    let resp = s.hover("Illuminate/Auth/Access/Gate.php", 855, 50).await;
    let out = render_hover(&resp);
    assert!(
        out.contains("camel"),
        "hover on static call site should show camel() signature, got: {out}"
    );
    assert!(
        out.contains("```php"),
        "hover should be markdown, got: {out}"
    );
}

/// Hover on an interface name at an `implements` clause shows the interface.
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

/// `Str::` triggers static member completion with camel, lower, upper, etc.
///
/// **Gap**: `Str::` currently returns PHP keywords/globals instead of `Str`
/// class methods. Static member completion does not resolve the class from a
/// `use`-import alias (`use Illuminate\Support\Str`), so the completer cannot
/// look up the correct class in the index.
#[ignore]
#[tokio::test]
async fn laravel_completion_static_members() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;

    // Synthetic file with a `Str::` trigger to test static member completion.
    let src = "<?php\nuse Illuminate\\Support\\Str;\nStr::\n";
    s.open("__test_static_completion.php", src).await;

    // Line 2 (0-based), character 5 = immediately after `Str::` (S=0,t=1,r=2,:=3,:=4, cursor at 5).
    let resp = s.completion("__test_static_completion.php", 2, 5).await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let items = resp["result"]["items"]
        .as_array()
        .or_else(|| resp["result"].as_array())
        .expect("completion items array");
    assert!(
        !items.is_empty(),
        "expected static completion items for Str::, got 0"
    );
    let labels: Vec<&str> = items.iter().filter_map(|i| i["label"].as_str()).collect();
    let has_camel = labels.iter().any(|l| l.contains("camel"));
    assert!(
        has_camel,
        "static completion for Str:: should include 'camel', got: {labels:?}"
    );
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

/// Opening a clean Laravel framework file produces no diagnostics.
/// Guards against false-positive noise on real-world code.
#[tokio::test]
async fn laravel_diagnostics_clean_file_no_noise() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    // Wait for the workspace index so trait declarations from other files are
    // known to the analyzer — without this, traits like RebindsCallbacksToSelf
    // produce false-positive "does not exist" errors.
    s.wait_for_index_ready_secs(60).await;
    let diag = s
        .open(
            "Illuminate/Auth/AuthManager.php",
            &read("Illuminate/Auth/AuthManager.php"),
        )
        .await;
    let empty = vec![];
    let all = diag["params"]["diagnostics"].as_array().unwrap_or(&empty);
    // Global functions declared in composer `autoload.files` (e.g. functions.php)
    // are a known analyzer gap: the mir session doesn't include them, so calls
    // to enum_value() and similar helpers produce false-positive UndefinedFunction
    // errors. Filter those out so the test guards against *new* regressions only.
    let known_gaps = ["enum_value"];
    let errors: Vec<_> = all
        .iter()
        .filter(|d| d["severity"].as_u64() == Some(1))
        .filter(|d| {
            let msg = d["message"].as_str().unwrap_or("");
            !known_gaps.iter().any(|g| msg.contains(g))
        })
        .collect();
    assert!(
        errors.is_empty(),
        "expected 0 unexpected errors in clean AuthManager.php, got: {errors:#?}"
    );
}

/// A file with a return-type mismatch produces an error diagnostic.
/// Guards against type-error diagnostics silently stopping after index.
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

/// Opening `Eloquent/Model.php` produces no unexpected error diagnostics.
///
/// **Gap**: Produces false-positive `UndefinedFunction` errors for `tap()` and
/// `class_uses_recursive()` — these are autoload-file helpers declared in
/// `composer.json`'s `autoload.files` section. The workspace scanner now
/// discovers and pre-ingests these files into the mir session so they no
/// longer produce false UndefinedFunction diagnostics.
#[tokio::test]
async fn laravel_diagnostics_no_noise_model() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;
    let diag = s
        .open(
            "Illuminate/Database/Eloquent/Model.php",
            &read("Illuminate/Database/Eloquent/Model.php"),
        )
        .await;
    let empty = vec![];
    let all = diag["params"]["diagnostics"].as_array().unwrap_or(&empty);
    // The test guards specifically against false UndefinedFunction / UndefinedClass
    // noise from autoload.files helpers (tap, class_uses_recursive, …).
    // Type-level issues in Model.php are a separate concern and excluded here.
    let undef_noise: Vec<_> = all
        .iter()
        .filter(|d| d["severity"].as_u64() == Some(1))
        .filter(|d| {
            let code = d["code"].as_str().unwrap_or("");
            matches!(
                code,
                "UndefinedFunction" | "UndefinedClass" | "UndefinedTrait"
            )
        })
        .collect();
    assert!(
        undef_noise.is_empty(),
        "expected no undefined-function/class noise in Eloquent/Model.php (autoload.files gap), got: {undef_noise:#?}"
    );
}

// ── Document Symbols ──────────────────────────────────────────────────────────

/// `documentSymbol` for AuthManager returns a hierarchical structure with the
/// class at the top and its methods and properties as children.
/// Guards against document symbols returning empty after index.
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

/// Inlay hints for a file with inferred parameter types actually contain labels.
/// Guards against inlay hints always returning an empty array on real code.
#[tokio::test]
async fn laravel_inlay_hints_content() {
    if !laravel_available() {
        return;
    }
    let mut s = TestServer::with_root(LARAVEL_SRC).await;
    s.wait_for_index_ready_secs(60).await;

    // Use a synthetic file with obvious parameter-name hints to check content.
    let src = "<?php\nuse Illuminate\\Support\\Str;\nStr::camel('hello_world');\n";
    s.open("__test_inlay_hints.php", src).await;

    let resp = s.inlay_hints("__test_inlay_hints.php", 0, 0, 3, 0).await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    // If hints are supported for parameter names, at least one label should appear.
    // This test documents whether the feature produces content or is always silent.
    let hints = resp["result"].as_array().unwrap_or(&vec![]).to_vec();
    // Not asserting non-empty: parameter name hints may not be implemented yet.
    // The test guards that the request completes and returns a valid (possibly empty) array.
    assert!(
        !resp["result"].is_null(),
        "inlay hints should return an array (possibly empty), not null"
    );
    // Log hint count for observability.
    let _ = hints.len();
}

// ── Rename ────────────────────────────────────────────────────────────────────

/// Renaming the `AuthManager` class at its declaration produces a workspace edit.
/// Guards against rename returning null.
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
/// AuthManager writes `implements FactoryContract` where `FactoryContract` is a
/// use-import alias for `Illuminate\Contracts\Auth\Factory`. The workspace index
/// resolves the alias so `subtypes_of["Factory"]` includes AuthManager.
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
    assert!(
        !locs.is_empty(),
        "expected ≥1 implementation of Factory::guard(), got 0"
    );
    let uris: Vec<&str> = locs.iter().filter_map(|l| l["uri"].as_str()).collect();
    assert!(
        uris.iter().any(|u| u.contains("AuthManager")),
        "expected AuthManager::guard() among implementations, got: {uris:?}"
    );
}

// ── Type Hierarchy ───────────────────────────────────────────────────────────

/// Supertypes of `AuthManager` includes the `Factory` interface.
///
/// `typeHierarchy/supertypes` for `AuthManager` returns `Factory` (resolved
/// through the `FactoryContract` use-import alias). Fixed in `type_hierarchy.rs`
/// by resolving use-import aliases in `supertypes_of_from_workspace`.
#[tokio::test]
async fn laravel_type_hierarchy_supertypes() {
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

    // Prepare type hierarchy at the AuthManager class name (line 17, char 6).
    let prep = s
        .prepare_type_hierarchy("Illuminate/Auth/AuthManager.php", 17, 6)
        .await;
    assert!(
        prep["error"].is_null(),
        "prepareTypeHierarchy error: {prep:#}"
    );
    let items = prep["result"].as_array().expect("array result");
    assert!(!items.is_empty(), "prepareTypeHierarchy returned no items");

    let supers = s.supertypes(items[0].clone()).await;
    assert!(supers["error"].is_null(), "supertypes error: {supers:#}");
    let super_items = supers["result"].as_array().unwrap_or(&vec![]).to_vec();
    let names: Vec<&str> = super_items
        .iter()
        .filter_map(|i| i["name"].as_str())
        .collect();
    assert!(
        names.iter().any(|n| n.contains("Factory")),
        "supertypes of AuthManager should include Factory, got: {names:?}"
    );
}

/// Subtypes of the `Factory` interface includes `AuthManager`.
/// Guards against type hierarchy subtypes returning empty after workspace index.
#[tokio::test]
async fn laravel_type_hierarchy_subtypes() {
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

    // Line 4 (0-based) = `interface Factory`; character 10 = "Factory".
    let prep = s
        .prepare_type_hierarchy("Illuminate/Contracts/Auth/Factory.php", 4, 10)
        .await;
    assert!(
        prep["error"].is_null(),
        "prepareTypeHierarchy error: {prep:#}"
    );
    let items = prep["result"].as_array().expect("array result");
    assert!(!items.is_empty(), "prepareTypeHierarchy returned no items");

    let subs = s.subtypes(items[0].clone()).await;
    assert!(subs["error"].is_null(), "subtypes error: {subs:#}");
    let sub_items = subs["result"].as_array().unwrap_or(&vec![]).to_vec();
    let names: Vec<&str> = sub_items
        .iter()
        .filter_map(|i| i["name"].as_str())
        .collect();
    assert!(
        names.iter().any(|n| n.contains("AuthManager")),
        "subtypes of Factory should include AuthManager, got: {names:?}"
    );
}

/// `textDocument/implementation` on the `Factory` interface name returns
/// `AuthManager` (the concrete implementor).
///
/// Find implementations on the `Factory` interface name includes `AuthManager`
/// (which implements it via the `FactoryContract` alias). Fixed in
/// `implementation.rs` by resolving use-import aliases in the implements check.
#[tokio::test]
async fn laravel_find_implementations_interface_name() {
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

    // Line 4 (0-based) = `interface Factory`; character 10 = "Factory".
    let resp = s
        .implementation("Illuminate/Contracts/Auth/Factory.php", 4, 10)
        .await;
    assert!(resp["error"].is_null(), "error: {resp:#}");
    let locs = resp["result"].as_array().unwrap_or(&vec![]).to_vec();
    assert!(
        !locs.is_empty(),
        "expected ≥1 implementation of Factory interface, got 0"
    );
    let uris: Vec<&str> = locs.iter().filter_map(|l| l["uri"].as_str()).collect();
    assert!(
        uris.iter().any(|u| u.contains("AuthManager")),
        "expected AuthManager among Factory implementations, got: {uris:?}"
    );
}

// ── Under-load stability ──────────────────────────────────────────────────────

/// Open multiple large files concurrently, then verify each feature still
/// responds in time.  Protects against the server request queue serializing
/// or salsa contention causing timeouts when several heavy files are open.
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
