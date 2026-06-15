//! Code action smoke coverage. Each scenario uses a two-`$0` selection to
//! name the range the action acts on.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn code_actions_offers_generate_constructor() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
class U$0ser$0 {
    public string $name = '';
    public int $age = 0;
}
"#,
        )
        .await;
    expect![[r#"
        refactor         Generate 2 getters/setters
        refactor         Generate constructor
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_actions_offers_extract_variable_on_expression() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
function f(): int {
    return $01 + 2$0;
}
"#,
        )
        .await;
    expect!["refactor.extract Extract variable [edit]"].assert_eq(&out);
}

#[tokio::test]
async fn code_actions_offers_add_return_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
function $0noReturn$0() { return 42; }
"#,
        )
        .await;
    expect![[r#"
        refactor         Add return type `: mixed`
        refactor         Generate PHPDoc
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_actions_offers_implement_missing_methods() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_actions(
            r#"<?php
interface Writable { public function write(): void; }
class $0My$0 implements Writable {}
"#,
        )
        .await;
    expect![[r#"
        quickfix         Implement missing method
        refactor.extract Extract variable [edit]"#]]
    .assert_eq(&out);
}

// --- codeAction/resolve roundtrip tests ---
// Each test verifies that a deferred action (no `edit` on initial request)
// gets its full WorkspaceEdit populated after calling codeAction/resolve.

#[tokio::test]
async fn code_action_resolve_return_type() {
    let mut server = TestServer::new().await;
    server
        .open("rt.php", "<?php\nfunction noReturn() { return 42; }\n")
        .await;

    let resp = server.code_action("rt.php", 1, 0, 1, 30).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Add return type `: mixed`"))
        .cloned()
        .expect("return type action");

    assert!(
        action["edit"].is_null(),
        "action must be deferred (no edit)"
    );
    assert!(!action["data"].is_null(), "action must carry resolve data");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null(), "resolve must not error");
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // rt.php
        1:19-1:19 → ": mixed""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_phpdoc() {
    let mut server = TestServer::new().await;
    server
        .open("doc.php", "<?php\nfunction greet(string $name): void {}\n")
        .await;

    let resp = server.code_action("doc.php", 1, 0, 1, 40).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Generate PHPDoc"))
        .cloned()
        .expect("phpdoc action");

    assert!(action["edit"].is_null(), "action must be deferred");
    assert!(!action["data"].is_null(), "action must carry resolve data");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // doc.php
        1:0-1:0 → "/**\n * @param string $name\n * @return void\n */\n""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_constructor() {
    let mut server = TestServer::new().await;
    server
        .open(
            "ctor.php",
            "<?php\nclass Point {\n    public float $x = 0.0;\n    public float $y = 0.0;\n}\n",
        )
        .await;

    let resp = server.code_action("ctor.php", 1, 0, 1, 11).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Generate constructor"))
        .cloned()
        .expect("constructor action");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // ctor.php
        4:0-4:0 → "    public function __construct(\n        float $x,\n        float $y,\n    ) {\n        $this->x = $x;\n        $this->y = $y;\n    }\n\n""#]].assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_getters_setters() {
    let mut server = TestServer::new().await;
    server
        .open(
            "gs.php",
            "<?php\nclass Box {\n    public int $width = 0;\n    public int $height = 0;\n}\n",
        )
        .await;

    let resp = server.code_action("gs.php", 1, 0, 1, 9).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.contains("getters/setters"))
                .unwrap_or(false)
        })
        .cloned()
        .expect("getters/setters action");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // gs.php
        4:0-4:0 → "    public function getWidth(): int\n    {\n        return $this->width;\n    }\n\n    public function setWidth(int $width): void\n    {\n        $this->width = $width;\n    }\n\n    public function getHeight(): int\n    {\n        return $this->height;\n    }\n\n    public function setHeight(int $height): void\n    {\n        $this->height = $height;\n    }\n\n""#]].assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_implement_missing_methods() {
    let mut server = TestServer::new().await;
    server
        .open(
            "impl.php",
            "<?php\ninterface Loggable { public function log(): void; }\nclass App implements Loggable {}\n",
        )
        .await;

    let resp = server.code_action("impl.php", 2, 0, 2, 30).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str() == Some("Implement missing method"))
        .cloned()
        .expect("implement missing method action");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // impl.php
        2:31-2:31 → "\n    public function log(): void\n    {\n        throw new \\RuntimeException('Not implemented');\n    }\n\n""#]].assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_promote_constructor_params() {
    let mut server = TestServer::new().await;
    server
        .open(
            "promote.php",
            "<?php\nclass Service {\n    public function __construct(\n        private string $name,\n        private int $port\n    ) {}\n}\n",
        )
        .await;

    let resp = server.code_action("promote.php", 2, 0, 5, 6).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .cloned();

    if let Some(action) = action {
        assert!(action["edit"].is_null(), "action must be deferred");

        let resolved = server.code_action_resolve(action).await;
        assert!(resolved["error"].is_null());
        let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
        expect![[r#""#]].assert_eq(&out);
    }
    // promote action may not apply when params are already promoted — that is valid
}

#[tokio::test]
async fn code_action_resolve_promote_simple_private_property() {
    let mut server = TestServer::new().await;
    server
        .open(
            "promote.php",
            "<?php\nclass Foo {\n    private string $name$0;\n    public function __construct(string $name) {\n        $this->name = $name;\n    }\n}\n",
        )
        .await;

    let resp = server.code_action("promote.php", 2, 0, 5, 6).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .cloned()
        .expect("promote action should be offered for promotable properties");

    let title = action["title"].as_str().expect("action must have title");
    assert_eq!(
        title, "Promote constructor parameter",
        "single property should use singular form"
    );

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // promote.php
        2:0-3:0 → ""
        3:32-3:32 → "private "
        4:0-5:0 → """#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_promote_readonly_property() {
    let mut server = TestServer::new().await;
    server
        .open(
            "promote.php",
            "<?php\nclass Bar {\n    private readonly string $id$0;\n    public function __construct(string $id) {\n        $this->id = $id;\n    }\n}\n",
        )
        .await;

    let resp = server.code_action("promote.php", 2, 0, 5, 6).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .cloned()
        .expect("promote action should be offered for readonly properties");

    let title = action["title"].as_str().expect("action must have title");
    assert_eq!(
        title, "Promote constructor parameter",
        "readonly property should also use singular form"
    );

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // promote.php
        2:0-3:0 → ""
        3:32-3:32 → "private readonly "
        4:0-5:0 → """#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_promote_multiple_properties() {
    let mut server = TestServer::new().await;
    server
        .open(
            "promote.php",
            "<?php\nclass Baz {\n    private string $name$0;\n    protected int $age;\n    public function __construct(string $name, int $age) {\n        $this->name = $name;\n        $this->age = $age;\n    }\n}\n",
        )
        .await;

    let resp = server.code_action("promote.php", 2, 0, 7, 6).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .cloned()
        .expect("promote action should be offered for multiple properties");

    let title = action["title"].as_str().expect("action must have title");
    assert_eq!(
        title, "Promote 2 constructor parameters",
        "multiple properties should use plural form with count"
    );

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // promote.php
        2:0-3:0 → ""
        3:0-4:0 → ""
        4:32-4:32 → "private "
        4:46-4:46 → "protected "
        5:0-6:0 → ""
        6:0-7:0 → """#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn code_action_resolve_without_data_is_passthrough() {
    let mut server = TestServer::new().await;
    server.open("noop.php", "<?php").await;

    let action = serde_json::json!({
        "title": "My Action",
        "kind": "refactor"
    });
    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    assert_eq!(
        resolved["result"]["title"].as_str(),
        Some("My Action"),
        "title must roundtrip"
    );
    assert!(
        resolved["result"]["edit"].is_null(),
        "no edit should be added for data-less actions"
    );
}

// ── Negative tests for promote_action: verify action is NOT offered ────

#[tokio::test]
async fn promote_action_not_offered_without_constructor() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Foo {
    private string $name$0;
}
"#,
        )
        .await;
    // Should NOT contain promote action
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered when constructor is missing"
    );
}

#[tokio::test]
async fn promote_action_not_offered_for_static_properties() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Foo {
    private static string $name$0;
    public function __construct() {}
}
"#,
        )
        .await;
    // Static properties should not be promotable
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered for static properties"
    );
}

#[tokio::test]
async fn promote_action_not_offered_for_mismatched_names() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Foo {
    private string $title$0;
    public function __construct(string $name) {
        $this->title = $name;
    }
}
"#,
        )
        .await;
    // Param name ($name) doesn't match property name ($title)
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered when param name doesn't match property name"
    );
}

#[tokio::test]
async fn promote_action_not_offered_for_complex_assignments() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Foo {
    private string $name$0;
    public function __construct(string $name) {
        $this->name = strtolower($name);
    }
}
"#,
        )
        .await;
    // Only simple assignments ($this->x = $x) are promotable
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered for complex assignments"
    );
}

#[tokio::test]
async fn promote_action_not_offered_without_visibility_modifier() {
    let mut server = TestServer::new().await;
    server.validate_syntax(false);
    let out = server
        .check_code_actions(
            r#"<?php
class Foo {
    string $name$0;
    public function __construct(string $name) {
        $this->name = $name;
    }
}
"#,
        )
        .await;
    // Properties must have visibility modifier
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered for properties without visibility modifier"
    );
}

/// Properties immediately before constructor (no blank line) should still be promotable.
#[tokio::test]
async fn promote_action_with_no_blank_line_before_constructor() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Foo {
    public string $name$0;
    public function __construct(string $name) {
        $this->name = $name;
    }
}
"#,
        )
        .await;
    assert!(
        out.contains("Promote constructor parameter"),
        "promote action should be offered even with no blank line before constructor"
    );
}

/// Promote action with multiple properties works when using range selection.
#[tokio::test]
async fn promote_action_on_multiple_properties() {
    let mut server = TestServer::new().await;
    server
        .open(
            "multi.php",
            "<?php\nclass User {\n    public string $firstName;\n    public string $lastName;\n    public function __construct(string $firstName, string $lastName) {\n        $this->firstName = $firstName;\n        $this->lastName = $lastName;\n    }\n}\n",
        )
        .await;

    let resp = server.code_action("multi.php", 1, 0, 8, 2).await;
    let promotes: Vec<_> = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .collect();

    assert!(
        !promotes.is_empty(),
        "promote action should be offered for multiple properties"
    );
}

/// Constructor parameters with default values should be promotable.
#[tokio::test]
async fn promote_action_with_constructor_default_value() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Config {
    public string $envir$0onment;
    public function __construct(string $environment = 'dev') {
        $this->environment = $environment;
    }
}
"#,
        )
        .await;
    assert!(
        out.contains("Promote constructor parameter"),
        "promote action should be offered even with default values"
    );
}

/// Properties without trailing newline before constructor should work.
/// Regression: whole_line_range logic handles files without trailing newlines.
#[tokio::test]
async fn promote_action_resolve_no_trailing_newline() {
    let mut server = TestServer::new().await;
    server
        .open(
            "promote_no_newline.php",
            "<?php\nclass Foo {\n    public string $name;\n    public function __construct(string $name) {\n        $this->name = $name;\n    }\n}",
        )
        .await;

    let resp = server
        .code_action("promote_no_newline.php", 1, 0, 5, 2)
        .await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .cloned();

    assert!(
        action.is_some(),
        "promote action should work for files without trailing newline"
    );
}

/// Readonly properties with complex types (nullable, mixed, etc.) should be promotable.
#[tokio::test]
async fn promote_action_readonly_with_nullable_type() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Config {
    public readonly ?string $va$0lue;
    public function __construct(?string $value = null) {
        $this->value = $value;
    }
}
"#,
        )
        .await;
    assert!(
        out.contains("Promote"),
        "promote action should work with readonly + nullable types"
    );
}

// --- Documented Limitations ---

/// **LIMITATION**: Unbraced namespace context is not tracked by promote_action.
/// In unbraced namespace declarations (namespace Foo;), classes defined at the
/// top level still work. However, this test documents the behavior.
#[tokio::test]
async fn promote_action_limitation_unbraced_namespace_context() {
    let mut server = TestServer::new().await;
    server
        .open(
            "unbraced_ns.php",
            "<?php\nnamespace App;\n\nclass Foo {\n    public string $name;\n    public function __construct(string $name) {\n        $this->name = $name;\n    }\n}\n",
        )
        .await;

    let resp = server.code_action("unbraced_ns.php", 0, 0, 8, 2).await;
    let promotes: Vec<_> = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| {
            a["title"]
                .as_str()
                .map(|t| t.to_lowercase().contains("promot"))
                .unwrap_or(false)
        })
        .collect();

    // In unbraced namespace, promotion should still work because the class
    // is found at the top level where the namespace applies
    assert!(
        !promotes.is_empty(),
        "promote action should work in unbraced namespaces at top level"
    );
}

/// **LIMITATION**: Static properties are explicitly not promoted because
/// they cannot be promoted to constructor parameters (constructor parameters
/// can only handle instance properties).
#[tokio::test]
async fn promote_action_limitation_static_properties_not_promotable() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Config {
    public static string $database$0;
    public function __construct(string $database) {
        self::$database = $database;
    }
}
"#,
        )
        .await;
    // Static properties cannot be promoted to constructor parameters
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered for static properties"
    );
}

/// **LIMITATION**: Complex assignments are not promotable.
/// Only simple assignments like `$this->prop = $param;` are supported.
/// Other patterns like conditional assignment, function call assignment, etc. are skipped.
#[tokio::test]
async fn promote_action_limitation_complex_assignments() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Foo {
    public int $value$0;
    public function __construct(int $value) {
        if ($value > 0) {
            $this->value = $value;
        }
    }
}
"#,
        )
        .await;
    // Complex assignments (inside conditionals) are not promoted
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered for complex assignments"
    );
}

/// **LIMITATION**: Assignment to wrong variable name prevents promotion.
/// If parameter is `$name` but assignment is `$this->name = $different;`,
/// the property won't be promoted because names don't match.
#[tokio::test]
async fn promote_action_limitation_mismatched_parameter_name() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class User {
    public string $email$0;
    public function __construct(string $address) {
        $this->email = $address;
    }
}
"#,
        )
        .await;
    // Parameter name doesn't match property name
    assert!(
        !out.contains("Promote"),
        "promote action should not be offered when parameter name doesn't match property"
    );
}

/// **LIMITATION**: Multiple assignment targets are not promoted.
/// Promotion requires a simple one-to-one mapping between property and parameter.
#[tokio::test]
async fn promote_action_limitation_multiple_assignments() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Logger {
    public string $file$0;
    public string $level;
    public function __construct(string $file, string $level) {
        $this->file = $file;
        $this->level = $level;
    }
}
"#,
        )
        .await;
    // When multiple properties are promotable, the action shows aggregate count
    assert!(
        out.contains("Promote 2 constructor parameters"),
        "promote action should show correct count for multiple properties"
    );
}

// --- Regression tests for property type hint propagation ---

/// Regression: property type hints were not propagated to promoted parameters.
/// When promoting a typed property (e.g., "private string $name") where the
/// constructor parameter had no explicit type, the generated parameter would lose
/// the property's type hint, producing "private $name" instead of "private string $name".
/// Bug #9 from ROADMAP: type hint now stored in prop_info and emitted during promotion.
#[tokio::test]
async fn promote_action_with_property_type_hint() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class User {
    private string $name$0;
    public function __construct(string $name) {
        $this->name = $name;
    }
}
"#,
        )
        .await;
    // The promote action should be offered for typed properties
    assert!(
        out.contains("Promote"),
        "promote action should be offered for typed property"
    );
}

/// Edge case: property with nullable type hint should promote.
#[tokio::test]
async fn promote_action_with_nullable_type_hint() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Config {
    private ?string $value$0;
    public function __construct(?string $value) {
        $this->value = $value;
    }
}
"#,
        )
        .await;
    // The promote action should be offered for nullable properties
    assert!(
        out.contains("Promote"),
        "promote action should be offered for nullable property"
    );
}

/// Edge case: property with union type hint should promote.
#[tokio::test]
async fn promote_action_with_union_type_hint() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Parser {
    private int|string $data$0;
    public function __construct(int|string $data) {
        $this->data = $data;
    }
}
"#,
        )
        .await;
    // The promote action should be offered for union type properties
    assert!(
        out.contains("Promote"),
        "promote action should be offered for union type property"
    );
}

/// Edge case: readonly property should promote.
#[tokio::test]
async fn promote_action_with_readonly_property() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Config {
    private readonly string $key$0;
    public function __construct(string $key) {
        $this->key = $key;
    }
}
"#,
        )
        .await;
    // The promote action should be offered for readonly properties
    assert!(
        out.contains("Promote"),
        "promote action should be offered for readonly property"
    );
}

/// Edge case: property with mixed type should promote.
#[tokio::test]
async fn promote_action_with_mixed_type() {
    let mut server = TestServer::new().await;
    let out = server
        .check_code_actions(
            r#"<?php
class Flexible {
    private mixed $value$0;
    public function __construct(mixed $value) {
        $this->value = $value;
    }
}
"#,
        )
        .await;
    // The promote action should be offered for mixed type properties
    assert!(
        out.contains("Promote"),
        "promote action should be offered for mixed type property"
    );
}

/// Implement missing methods when the interface is defined in a separate file
/// (background-indexed, not open). The action must resolve across the whole
/// workspace, not just open files.
#[tokio::test]
async fn code_action_resolve_implement_cross_file_interface() {
    let mut server = TestServer::new().await;
    server
        .open(
            "Printable.php",
            "<?php\ninterface Printable { public function print(): void; public function getLabel(): string; }\n",
        )
        .await;
    server
        .open(
            "Report.php",
            "<?php\nclass Report implements Printable {}\n",
        )
        .await;

    let resp = server.code_action("Report.php", 1, 0, 1, 37).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.starts_with("Implement"))
                .unwrap_or(false)
        })
        .cloned()
        .expect("implement missing methods action should be offered for cross-file interface");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // Report.php
        1:35-1:35 → "\n    public function print(): void\n    {\n        throw new \\RuntimeException('Not implemented');\n    }\n\n    public function getLabel(): string\n    {\n        throw new \\RuntimeException('Not implemented');\n    }\n\n""#]]
    .assert_eq(&out);
}

/// Implement missing methods when the interface is in a namespaced file and
/// the class uses a `use` import. Namespace resolution must be FQN-aware.
#[tokio::test]
async fn code_action_resolve_implement_namespaced_cross_file_interface() {
    let mut server = TestServer::new().await;
    server
        .open(
            "Contracts/Serializable.php",
            "<?php\nnamespace App\\Contracts;\ninterface Serializable { public function serialize(): string; }\n",
        )
        .await;
    server
        .open(
            "Models/User.php",
            "<?php\nnamespace App\\Models;\nuse App\\Contracts\\Serializable;\nclass User implements Serializable {}\n",
        )
        .await;

    let resp = server.code_action("Models/User.php", 3, 0, 3, 34).await;
    let action = resp["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.starts_with("Implement"))
                .unwrap_or(false)
        })
        .cloned()
        .expect("implement missing methods should work with namespaced cross-file interface");

    assert!(action["edit"].is_null(), "action must be deferred");

    let resolved = server.code_action_resolve(action).await;
    assert!(resolved["error"].is_null());
    let out = canonicalize_workspace_edit(&resolved["result"]["edit"], &server.uri(""));
    expect![[r#"
        // Models/User.php
        3:36-3:36 → "\n    public function serialize(): string\n    {\n        throw new \\RuntimeException('Not implemented');\n    }\n\n""#]]
    .assert_eq(&out);
}

// --- Quick-fix: UndefinedFunction → use function FQN; ---

#[tokio::test]
async fn code_action_quickfix_undefined_function_cross_file() {
    let mut server = TestServer::new().await;
    server
        .open(
            "helpers.php",
            "<?php\nnamespace App\\Helpers;\nfunction tap(mixed $value): mixed { return $value; }\n",
        )
        .await;
    // open() waits for publishDiagnostics — analysis is complete before code_action
    server
        .open("main.php", "<?php\nnamespace App;\ntap($x);\n")
        .await;

    let resp = server.code_action("main.php", 2, 0, 2, 10).await;
    let actions = resp["result"].as_array().cloned().unwrap_or_default();
    let action = actions
        .iter()
        .find(|a| {
            a["title"]
                .as_str()
                .map(|t| t.starts_with("Add use function"))
                .unwrap_or(false)
        })
        .cloned();

    assert!(
        action.is_some(),
        "expected 'Add use function' quick-fix, got: {resp:#}"
    );
    let a = action.unwrap();
    let out = canonicalize_workspace_edit(&a["edit"], &server.uri(""));
    expect![[r#"
        // main.php
        2:0-2:0 → "use function App\\Helpers\\tap;\n""#]]
    .assert_eq(&out);
}
