//! Diagnostic coverage matrix using the caret annotation DSL.
//! Each test names the expectation inline with `// ^^^ severity: message`.

use super::*;

use expect_test::expect;
use serde_json::json;

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
async fn undefined_function_detected_in_closure() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
$fn = function(): void {
    nonexistent_function();
//  ^^^^^^^^^^^^^^^^^^^^^^ error: nonexistent_function
};
"#,
        )
        .await;
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

// ── Audit report §5: false UndefinedMethod on trait-aliased methods ─────────

/// CONTROL: a plain (un-aliased) trait method call produces no diagnostics.
#[tokio::test]
async fn trait_method_call_no_false_undefined_method() {
    let mut s = TestServer::new().await;
    s.check_no_diagnostics(
        r#"<?php
trait BaseInit {
    public function init(int $x): void {}
}
class Query {
    use BaseInit;
    public function __construct() {
        $this->init(1);
    }
}
"#,
    )
    .await;
}

/// BUG (report §5): a trait-aliased method call must not be flagged
/// `UndefinedMethod` — the alias introduces a real method.
#[tokio::test]
#[ignore = "KNOWN BUG (audit report §5): trait alias not tracked -> false UndefinedMethod"]
async fn trait_aliased_method_call_no_false_undefined_method_bug() {
    let mut s = TestServer::new().await;
    s.check_no_diagnostics(
        r#"<?php
trait BaseInit {
    public function __construct(int $x) {}
}
class Query {
    use BaseInit { __construct as __constructBase; }
    public function __construct() {
        $this->__constructBase(1);
    }
}
"#,
    )
    .await;
}
