use super::*;

use expect_test::expect;
use serde_json::json;

/// The definition file is never opened — it exists only in the workspace index
/// from the background scan. This is the typical production scenario.
#[tokio::test]
async fn inlay_hints_from_workspace_index_only() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("greeter.php"),
        "<?php\nfunction greet(string $name, int $count): void {}\n",
    )
    .unwrap();
    let caller_src = "<?php\ngreet('world', 3);\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();
    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    // Only open the caller — greeter.php is indexed but never opened.
    s.open("caller.php", caller_src).await;
    let resp = s.inlay_hints("caller.php", 0, 0, 3, 0).await;
    expect![[r#"
        1:6 name:
        1:15 count:"#]]
    .assert_eq(&render_inlay_hints(&resp));
}

#[tokio::test]
async fn inlay_hints_cross_file_function_call() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /caller.php
<?php
greet('world', 3);

//- /greeter.php
<?php
function greet(string $name, int $count): void {}
"#,
        )
        .await;
    expect![[r#"
        1:6 name:
        1:15 count:"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hints_cross_file_constructor_call() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /caller.php
<?php
$p = new Point(1, 2);

//- /Point.php
<?php
class Point {
    public function __construct(int $x, int $y) {}
}
"#,
        )
        .await;
    expect![[r#"
        1:15 x:
        1:18 y:"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hints_cross_file_method_call() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /caller.php
<?php
$g = new Greeter();
$g->sayHello('World');

//- /Greeter.php
<?php
class Greeter {
    public function sayHello(string $name): void {}
}
"#,
        )
        .await;
    expect![[r#"
        2:13 name:"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hints_for_parameter_names() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"<?php
function greet(string $name, int $count): void {}
greet('world', 3);
"#,
        )
        .await;
    expect![[r#"
        2:6 name:
        2:15 count:"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hint_resolve_populates_tooltip() {
    let mut s = TestServer::new().await;
    s.open(
        "resolve.php",
        "<?php\nfunction add(int $a, int $b): int { return $a + $b; }\nadd(1, 2);\n",
    )
    .await;
    let hints_resp = s.inlay_hints("resolve.php", 0, 0, 4, 0).await;
    let hints = hints_resp["result"].as_array().cloned().unwrap_or_default();
    assert!(!hints.is_empty(), "expected inlay hints");
    let resp = s.inlay_hint_resolve(hints[0].clone()).await;
    let out = render_resolved_inlay_hint(&resp);
    expect![[r#"
2:4 a:
tooltip: ```php
function add(int $a, int $b): int
```"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hint_resolve_with_docblock_includes_docs() {
    let mut s = TestServer::new().await;
    s.open(
        "resolve.php",
        "<?php\n/** Adds two integers */\nfunction add(int $a, int $b): int { return $a + $b; }\nadd(1, 2);\n",
    )
    .await;
    let hints_resp = s.inlay_hints("resolve.php", 0, 0, 5, 0).await;
    let hints = hints_resp["result"].as_array().cloned().unwrap_or_default();
    assert!(!hints.is_empty(), "expected inlay hints");
    let resp = s.inlay_hint_resolve(hints[0].clone()).await;
    let out = render_resolved_inlay_hint(&resp);
    expect![[r#"
3:4 a:
tooltip: ```php
function add(int $a, int $b): int
```

---

Adds two integers"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hint_resolve_no_data_field_returns_unchanged() {
    let mut s = TestServer::new().await;
    s.open("nohint.php", "<?php").await;

    let hint = json!({
        "position": { "line": 0, "character": 5 },
        "label": "$test:",
    });

    let resp = s.inlay_hint_resolve(hint).await;
    let out = render_resolved_inlay_hint(&resp);
    expect![[r#"
        0:5 $test:
        tooltip: <no tooltip>"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hint_resolve_existing_tooltip_is_noop() {
    let mut s = TestServer::new().await;
    s.open("existing.php", "<?php").await;

    let hint = json!({
        "position": { "line": 1, "character": 10 },
        "label": "param:",
        "tooltip": {
            "kind": "markdown",
            "value": "custom tooltip"
        }
    });

    let resp = s.inlay_hint_resolve(hint).await;
    let out = render_resolved_inlay_hint(&resp);
    expect![[r#"
        1:10 param:
        tooltip: custom tooltip"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hint_resolve_data_without_php_lsp_fn_returns_unchanged() {
    let mut s = TestServer::new().await;
    s.open("nokey.php", "<?php").await;

    let hint = json!({
        "position": { "line": 0, "character": 5 },
        "label": "param:",
        "data": {
            "some_other_key": "value"
        }
    });

    let resp = s.inlay_hint_resolve(hint).await;
    let out = render_resolved_inlay_hint(&resp);
    expect![[r#"
0:5 param:
tooltip: <no tooltip>"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hint_resolve_php_lsp_fn_nonexistent_function_returns_unchanged() {
    let mut s = TestServer::new().await;
    s.open("nofunc.php", "<?php").await;

    let hint = json!({
        "position": { "line": 2, "character": 8 },
        "label": "$x:",
        "data": {
            "php_lsp_fn": "nonExistentFunctionXyz"
        }
    });

    let resp = s.inlay_hint_resolve(hint).await;
    let out = render_resolved_inlay_hint(&resp);
    expect![[r#"
2:8 $x:
tooltip: <no tooltip>"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hint_resolve_is_idempotent() {
    let mut s = TestServer::new().await;
    s.open(
        "idempotent.php",
        "<?php\nfunction add(int $a, int $b): int { return $a + $b; }\nadd(1, 2);\n",
    )
    .await;

    let hints_resp = s.inlay_hints("idempotent.php", 0, 0, 4, 0).await;
    let hints = hints_resp["result"].as_array().cloned().unwrap_or_default();
    assert!(!hints.is_empty());

    let resolved_once = s.inlay_hint_resolve(hints[0].clone()).await;
    let resolved_twice = s.inlay_hint_resolve(resolved_once["result"].clone()).await;

    assert_eq!(
        resolved_once["result"], resolved_twice["result"],
        "calling resolve twice must return identical results (idempotent)"
    );
}

#[tokio::test]
async fn inlay_hints_nullsafe_method_call() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /caller.php
<?php
$g = new Greeter();
$g?->sayHello('World');

//- /Greeter.php
<?php
class Greeter {
    public function sayHello(string $name): void {}
}
"#,
        )
        .await;
    expect![[r#"
        2:14 name:"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hints_static_method_call() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /caller.php
<?php
Greeter::sayHello('world');

//- /Greeter.php
<?php
class Greeter {
    public static function sayHello(string $name): void {}
}
"#,
        )
        .await;
    expect![[r#"
        1:18 name:"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn inlay_hints_empty_for_file_with_no_calls() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"<?php
$x = 1;
$y = 2;
"#,
        )
        .await;
    expect!["<no hints>"].assert_eq(&out);
}

#[tokio::test]
async fn inlay_hints_respects_lsp_half_open_range_semantics() {
    // LSP uses half-open range semantics [start, end) where end is exclusive.
    // This test verifies that hints positioned exactly at range.end are excluded.
    let mut s = TestServer::new().await;
    s.open(
        "range_test.php",
        "<?php\nfunction f(int $x): void {}\nf(1);\n",
    )
    .await;

    // Line 2: "f(1);"
    //         01234
    // Hint is at character 2 (start of argument "1")

    // Range [0, 2) excludes the hint (it's AT the boundary, half-open semantics)
    let resp = s.inlay_hints("range_test.php", 2, 0, 2, 2).await;
    let out = render_inlay_hints(&resp);
    expect!["<no hints>"].assert_eq(&out);

    // Range [0, 3) includes the hint (it's within the range)
    let resp = s.inlay_hints("range_test.php", 2, 0, 2, 3).await;
    let out = render_inlay_hints(&resp);
    expect![[r#"2:2 x:"#]].assert_eq(&out);
}

/// Regression: method hint collisions when multiple classes define methods with same name.
/// Previously, method hints were keyed by bare method_name, so when two classes both
/// defined process() with different signatures, the wrong hint was shown.
/// Bug #8 from ROADMAP: method hints now keyed by "ClassName::methodName".
#[tokio::test]
async fn inlay_hints_method_name_collision() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /caller.php
<?php
$processor = new DataProcessor();
$processor->process(1, 2);

//- /DataProcessor.php
<?php
class DataProcessor {
    public function process(int $x, int $y): int {
        return $x + $y;
    }
}
"#,
        )
        .await;
    // Should show parameter hints for DataProcessor::process
    expect![[r#"
        2:20 x:
        2:23 y:"#]]
    .assert_eq(&out);
}

/// Edge case: two methods with same name but different parameter counts.
#[tokio::test]
async fn inlay_hints_method_different_signatures() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /main.php
<?php
$filter = new TextFilter();
$filter->apply("input", "lowercase");

//- /TextFilter.php
<?php
class TextFilter {
    public function apply(string $text, string $mode): string {
        return $mode === "lowercase" ? strtolower($text) : strtoupper($text);
    }
}
"#,
        )
        .await;
    expect![[r#"
        2:15 text:
        2:24 mode:"#]]
    .assert_eq(&out);
}

/// Edge case: inherited method should show parent's parameter hints, not overridden version.
#[tokio::test]
async fn inlay_hints_inherited_method_parameters() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /main.php
<?php
$child = new ChildClass();
$child->compute(10, 20);

//- /Parent.php
<?php
class ParentClass {
    public function compute(int $a, int $b): int {
        return $a + $b;
    }
}

//- /Child.php
<?php
class ChildClass extends ParentClass {
    // No override, should inherit parent's signature
}
"#,
        )
        .await;
    // Should show parent's parameter names (int $a, int $b)
    expect![[r#"
        2:16 a:
        2:20 b:"#]]
    .assert_eq(&out);
}

/// Edge case: static method with numeric parameters.
#[tokio::test]
async fn inlay_hints_static_method_with_math() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /main.php
<?php
$result = MathHelper::add(5, 3);

//- /MathHelper.php
<?php
class MathHelper {
    public static function add(int $a, int $b): int {
        return $a + $b;
    }
}
"#,
        )
        .await;
    // Static method hints should work the same as instance methods
    expect![[r#"
        1:26 a:
        1:29 b:
        1:31 : int"#]]
    .assert_eq(&out);
}

/// Edge case: abstract method inherited by concrete class.
#[tokio::test]
async fn inlay_hints_abstract_method_implementation() {
    let mut s = TestServer::new().await;
    let out = s
        .check_inlay_hints(
            r#"//- /main.php
<?php
$handler = new ConcreteHandler();
$handler->process("test", 123);

//- /Handler.php
<?php
abstract class AbstractHandler {
    abstract public function process(string $input, int $code): void;
}

//- /ConcreteHandler.php
<?php
class ConcreteHandler extends AbstractHandler {
    public function process(string $input, int $code): void {
        // implementation
    }
}
"#,
        )
        .await;
    // Should show abstract method's parameter names from parent
    expect![[r#"
        2:18 input:
        2:26 code:"#]]
    .assert_eq(&out);
}
