//! Rename coverage: prepareRename bounds + actual rename across files.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn prepare_rename_on_identifier() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_prepare_rename(
            r#"<?php
function gre$0et(): void {}
"#,
        )
        .await;
    expect!["1:9-1:14"].assert_eq(&out);
}

#[tokio::test]
async fn rename_function_same_file() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function gre$0et(): void {}
greet();
greet();
"#,
            "salute",
        )
        .await;
    expect![[r#"
        // main.php
        1:9-1:14 → "salute"
        2:0-2:5 → "salute"
        3:0-3:5 → "salute""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn rename_method_across_file() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
class Greeter {
    public function he$0llo(): string { return 'hi'; }
}
$g = new Greeter();
$g->hello();
"#,
            "salute",
        )
        .await;
    expect![[r#"
        // main.php
        2:20-2:25 → "salute"
        5:4-5:9 → "salute""#]]
    .assert_eq(&out);
}

/// Regression: renaming a variable inside an enum method previously produced
/// zero edits because collect_in_fn_at had no arm for StmtKind::Enum.
#[tokio::test]
async fn rename_variable_inside_enum_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
enum Status {
    public function label($a$0rg) { return $arg + 1; }
}
"#,
            "value",
        )
        .await;
    expect![[r#"
        // main.php
        2:26-2:30 → "$value"
        2:41-2:45 → "$value""#]]
    .assert_eq(&out);
}

/// Regression: renaming a variable parameter in an interface method previously
/// produced zero edits because collect_in_fn_at gated param collection inside
/// `if let Some(body)`, but interface methods have no body.
#[tokio::test]
async fn rename_variable_interface_method_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
interface Logger {
    public function log($mes$0sage): void;
}
"#,
            "$msg",
        )
        .await;
    expect![[r#"
        // main.php
        2:24-2:32 → "$msg""#]]
    .assert_eq(&out);
}

/// Regression: same bug as above but for abstract class methods.
#[tokio::test]
async fn rename_variable_abstract_class_method_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
abstract class Processor {
    abstract public function process($in$0put): string;
}
"#,
            "$data",
        )
        .await;
    expect![[r#"
        // main.php
        2:37-2:43 → "$data""#]]
    .assert_eq(&out);
}

/// Regression: same bug as above but for abstract trait methods.
#[tokio::test]
async fn rename_variable_abstract_trait_method_param() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
trait Formattable {
    abstract public function format($da$0ta): string;
}
"#,
            "$input",
        )
        .await;
    expect![[r#"
        // main.php
        2:36-2:41 → "$input""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn rename_class_updates_new_sites() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
class Wid$0get {}
$a = new Widget();
$b = new Widget();
"#,
            "Gadget",
        )
        .await;
    expect![[r#"
        // main.php
        1:6-1:12 → "Gadget"
        2:9-2:15 → "Gadget"
        3:9-3:15 → "Gadget""#]]
    .assert_eq(&out);
}

/// `prepareRename` on a PHP keyword must return null so the editor greys out
/// the rename action rather than presenting an empty rename dialog.
#[tokio::test]
async fn prepare_rename_on_keyword_returns_nothing() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_prepare_rename(
            r#"<?php
func$0tion greet(): void {}
"#,
        )
        .await;
    expect!["<not renameable>"].assert_eq(&out);
}

/// `prepareRename` on a variable should return the range covering the
/// variable name (without `$`) so editors highlight the right text.
#[tokio::test]
async fn prepare_rename_on_variable() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_prepare_rename(
            r#"<?php
function f(): void {
    $cou$0nt = 0;
}
"#,
        )
        .await;
    expect!["2:5-2:10"].assert_eq(&out);
}

/// Renaming a property via a `->access` site must update the declaration and
/// all other access sites. The cursor must be on the bare name after `->`,
/// not on the `$prop` declaration (which is treated as a variable rename).
#[tokio::test]
async fn rename_property_updates_all_access_sites() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
class Counter {
    public int $count = 0;
    public function inc(): void { $this->coun$0t++; }
    public function get(): int  { return $this->count; }
}
"#,
            "total",
        )
        .await;
    expect![[r#"
        // main.php
        2:16-2:21 → "total"
        3:41-3:46 → "total"
        4:48-4:53 → "total""#]]
    .assert_eq(&out);
}

/// Regression for #141: rename must rewrite the matching segment of a `use`
/// import in addition to call sites. Pinned via snapshot so a future change to
/// the single-pass walker cannot silently drop the `use`-line edit.
#[tokio::test]
async fn rename_class_rewrites_use_import_in_same_file() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
use Vendor\Lib\Widget;
$a = new Wid$0get();
$b = new Widget();
"#,
            "Gadget",
        )
        .await;
    expect![[r#"
        // main.php
        1:15-1:21 → "Gadget"
        2:9-2:15 → "Gadget"
        3:9-3:15 → "Gadget""#]]
    .assert_eq(&out);
}

/// Cross-file companion to `rename_class_rewrites_use_import_in_same_file`:
/// renaming the class in one file must rewrite both the `use` import segment
/// and short-name expression sites in dependents. Snapshot pinned so the
/// merged AST walker can't silently drop either category.
///
/// Note: type hints and inline fully-qualified `\App\Widget` references are
/// intentionally omitted — the general rename walker only emits spans for
/// `ExprKind::Identifier` whose text equals the short name, so neither form
/// participates in the cross-file rename surface today.
#[tokio::test]
async fn rename_class_rewrites_use_imports_across_files() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"//- /src/Widget.php
<?php
namespace App;
class Wid$0get {}

//- /src/a.php
<?php
use App\Widget;
$x = new Widget();
$is = $x instanceof Widget;

//- /src/b.php
<?php
use App\Widget;
$y = new Widget();
"#,
            "Gadget",
        )
        .await;
    expect![[r#"
        // src/Widget.php
        2:6-2:12 → "Gadget"

        // src/a.php
        1:8-1:14 → "Gadget"
        2:9-2:15 → "Gadget"
        3:20-3:26 → "Gadget"

        // src/b.php
        1:8-1:14 → "Gadget"
        2:9-2:15 → "Gadget""#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn rename_on_nonexistent_symbol_does_not_error() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    s.open("rn.php", "<?php\n// nothing to rename\n").await;
    let resp = s.rename("rn.php", 1, 5, "NewName").await;
    assert!(resp["error"].is_null(), "rename errored: {resp:?}");
}

// --- psr4-mini fixture: cross-file rename + PSR4-aware file rename ---

/// Set up psr4-mini with all three files open in the document store.
/// Both the in-file rename and willRenameFiles handlers require open documents.
async fn psr4_bring_up() -> TestServer {
    let mut server = TestServer::with_fixture("psr4-mini").await;
    server.wait_for_index_ready().await;
    let (user, _, _) = server.locate("src/Model/User.php", "<?php", 0);
    server.open("src/Model/User.php", &user).await;
    let (reg, _, _) = server.locate("src/Service/Registry.php", "<?php", 0);
    server.open("src/Service/Registry.php", &reg).await;
    let (greet, _, _) = server.locate("src/Service/Greeter.php", "<?php", 0);
    server.open("src/Service/Greeter.php", &greet).await;
    server
}

/// Renaming `class User` to `Account` must rewrite every `use App\Model\User`
/// import in dependent files. Snapshot-pinned so byte-offset regressions are
/// caught immediately.
#[tokio::test]
async fn rename_class_edits_all_dependents() {
    let mut server = psr4_bring_up().await;
    let (_, line, ch) = server.locate("src/Model/User.php", "class User", 0);

    let resp = server
        .rename("src/Model/User.php", line, ch + 6, "Account")
        .await;

    assert!(resp["error"].is_null(), "rename error: {resp:?}");
    let root = server.uri("");
    let snap = canonicalize_workspace_edit(&resp["result"], &root);
    expect![[r#"
        // src/Model/User.php
        4:6-4:10 → "Account"

        // src/Service/Greeter.php
        4:14-4:18 → "Account"

        // src/Service/Registry.php
        4:14-4:18 → "Account""#]]
    .assert_eq(&snap);
}

/// Moving `src/Model/User.php` to `src/Entity/User.php` changes the FQN from
/// `App\Model\User` to `App\Entity\User`; every `use App\Model\User` must be
/// rewritten.
#[tokio::test]
async fn will_rename_file_rewrites_use_imports_in_dependents() {
    let mut server = psr4_bring_up().await;
    let old_uri = server.uri("src/Model/User.php");
    let new_uri = server.uri("src/Entity/User.php");

    let resp = server.will_rename_files(vec![(old_uri, new_uri)]).await;

    assert!(resp["error"].is_null(), "willRenameFiles error: {resp:?}");
    let root = server.uri("");
    let snap = canonicalize_workspace_edit(&resp["result"], &root);
    expect![[r#"
        // src/Service/Greeter.php
        4:4-4:18 → "App\\Entity\\User"

        // src/Service/Registry.php
        4:4-4:18 → "App\\Entity\\User""#]]
    .assert_eq(&snap);
}

/// Renaming a file to the same PSR4-derived FQN must be a no-op.
#[tokio::test]
async fn will_rename_file_same_psr4_fqn_produces_no_edits() {
    let mut server = psr4_bring_up().await;
    let old_uri = server.uri("src/Model/User.php");
    let new_uri = old_uri.clone();

    let resp = server.will_rename_files(vec![(old_uri, new_uri)]).await;
    assert!(resp["error"].is_null(), "willRenameFiles error: {resp:?}");
    let changes = resp["result"]["changes"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    assert!(
        changes.is_empty(),
        "rename-to-self must not produce edits, got: {changes:?}"
    );
}

/// Deleting the file that defines `App\Model\User` must strip the `use` line
/// from every dependent.
#[tokio::test]
async fn will_delete_file_strips_use_imports_from_dependents() {
    let mut server = psr4_bring_up().await;
    let uri = server.uri("src/Model/User.php");

    let resp = server.will_delete_files(vec![uri]).await;

    assert!(resp["error"].is_null(), "willDeleteFiles error: {resp:?}");
    let root = server.uri("");
    let snap = canonicalize_workspace_edit(&resp["result"], &root);
    expect![[r#"
        // src/Service/Greeter.php
        4:0-5:0 → ""

        // src/Service/Registry.php
        4:0-5:0 → """#]]
    .assert_eq(&snap);
}

/// Rename must match exact word boundaries and not rename partial matches.
#[tokio::test]
async fn rename_does_not_match_partial_words() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function foo$0() {}
function foobar() {}
function barfoo() {}
foo();
foobar();
barfoo();
"#,
            "baz",
        )
        .await;
    expect![[r#"
        // main.php
        1:9-1:12 → "baz"
        4:0-4:3 → "baz""#]]
    .assert_eq(&out);
}

/// Rename a variable should only affect the same scope, not variables with the
/// same name in other function scopes.
#[tokio::test]
async fn rename_variable_does_not_cross_function_boundary() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function foo() { $x$0 = 1; }
function bar() { $x = 2; }
"#,
            "$y",
        )
        .await;
    expect![[r#"
        // main.php
        1:17-1:19 → "$y""#]]
    .assert_eq(&out);
}

/// Rename a property across multiple files should update declaration and all uses.
/// When renaming from access site ($obj->prop), all occurrences are updated.
#[tokio::test]
async fn rename_property_works_across_files() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"//- /a.php
<?php
class Foo {
    public int $count;
}

//- /b.php
<?php
$foo = new Foo();
echo $foo->coun$0t;
"#,
            "total",
        )
        .await;
    expect![[r#"
        // a.php
        2:16-2:21 → "total"

        // b.php
        2:11-2:16 → "total""#]]
    .assert_eq(&out);
}

/// Property rename from declaration site is not supported - must rename from access site.
/// This documents a current limitation: property_refs_in_stmts only finds access sites.
#[tokio::test]
async fn rename_property_from_declaration_site_not_supported() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
class Foo {
    public int $coun$0t;
}
$foo = new Foo();
$foo->count++;
echo $foo->count;
"#,
            "total",
        )
        .await;
    // Current implementation limitation: must rename from access site, not declaration
    expect!["<no `changes` map in {}>"].assert_eq(&out);
}

/// Renaming must respect static properties and not confuse them with instance properties.
#[tokio::test]
async fn rename_distinguishes_static_from_instance_properties() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
class Config {
    public static $instance;
    public $count;
    public function test(): void {
        $this->coun$0t++;
    }
}
"#,
            "total",
        )
        .await;
    expect![[r#"
        // main.php
        3:12-3:17 → "total"
        5:15-5:20 → "total""#]]
    .assert_eq(&out);
}

/// Rename must be case-sensitive and not match names that differ only in case.
#[tokio::test]
async fn rename_is_case_sensitive() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function test() {}
function Test() {}
tes$0t();
"#,
            "verify",
        )
        .await;
    expect![[r#"
        // main.php
        1:9-1:13 → "verify"
        3:0-3:4 → "verify""#]]
    .assert_eq(&out);
}

/// Rename multiple occurrences of the same function in different scopes.
#[tokio::test]
async fn rename_function_multiple_scopes() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function process$0() { process(); }
if (true) { process(); }
while (true) { process(); break; }
"#,
            "handle",
        )
        .await;
    expect![[r#"
        // main.php
        1:9-1:16 → "handle"
        1:21-1:28 → "handle"
        2:12-2:19 → "handle"
        3:15-3:22 → "handle""#]]
    .assert_eq(&out);
}

/// Rename variable across multiple functions (comprehensive coverage).
/// Verifies that rename works correctly with deeply nested scopes.
#[tokio::test]
async fn rename_variable_deep_scopes() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function outer() {
    $x$0 = 1;
    function inner() {
        $x = 2;
    }
    echo $x;
}
"#,
            "$y",
        )
        .await;
    // Rename should only affect $x in outer scope, not the inner $x
    expect![[r#"
        // main.php
        2:4-2:6 → "$y"
        6:9-6:11 → "$y""#]]
    .assert_eq(&out);
}

// --- Documented Limitations ---

/// **LIMITATION**: Property rename only works from access sites (->prop, ?->prop),
/// NOT from property declarations. This is by design - the `property_refs_in_stmts`
/// function only finds property access expressions, not declarations.
/// Workaround: Position cursor on a property access site, not the declaration.
#[tokio::test]
async fn rename_limitation_property_from_declaration_site() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
class Foo {
    public int $coun$0t;
}
$foo = new Foo();
$foo->count++;
echo $foo->count;
"#,
            "total",
        )
        .await;
    // Property rename from declaration site is not supported
    // Expected: no changes because the implementation can't find the property via declaration
    expect!["<no `changes` map in {}>"].assert_eq(&out);
}

/// **LIMITATION**: Property rename from declaration fails, but rename from
/// access site succeeds. This test demonstrates the workaround.
#[tokio::test]
async fn rename_property_from_access_site_works() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
class Foo {
    public int $count;
}
$foo = new Foo();
$foo->coun$0t++;
echo $foo->count;
"#,
            "total",
        )
        .await;
    // Property rename FROM access site works correctly
    expect![[r#"
        // main.php
        2:16-2:21 → "total"
        5:6-5:11 → "total"
        6:11-6:16 → "total""#]]
    .assert_eq(&out);
}

/// **LIMITATION**: Callable/closure parameter types are not fully supported.
/// Type hints like `callable`, `Closure`, etc. don't resolve to actual type definitions.
#[tokio::test]
async fn rename_limitation_callable_types() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function process(callable $callback$0): void {
    $callback();
}
"#,
            "$handler",
        )
        .await;
    // Rename the parameter itself works
    expect![[r#"
        // main.php
        1:17-1:35 → "$handler"
        2:4-2:13 → "$handler""#]]
    .assert_eq(&out);
}

/// **LIMITATION**: Superglobals ($_GET, $_POST, etc.) can technically be renamed,
/// but doing so breaks PHP functionality. This test documents that the feature
/// doesn't prevent renaming superglobals (unlike some IDEs that protect them).
#[tokio::test]
async fn rename_allows_superglobal_rename() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
if (isset($_GET$0['id'])) {
    echo $_GET['id'];
}
"#,
            "$params",
        )
        .await;
    // Superglobals CAN be renamed by this implementation (not recommended!)
    expect![[r#"
        // main.php
        1:10-1:15 → "$params"
        2:9-2:14 → "$params""#]]
    .assert_eq(&out);
}

// --- Regression tests for bugs fixed in walk.rs and rename.rs ---

/// Regression: arrow functions auto-capture outer-scope variables.
/// Previously, VarRefsVisitor treated arrow functions as hard scope boundaries
/// and did not recurse into their body, leaving arrow function references unrenamed.
/// Bug #2 from ROADMAP: arrow functions now properly auto-capture.
#[tokio::test]
async fn rename_variable_in_arrow_function() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function process(): void {
    $value$0 = 42;
    $fn = fn() => $value + 1;
    echo $value;
}
"#,
            "$result",
        )
        .await;
    expect![[r#"
        // main.php
        2:4-2:10 → "$result"
        3:18-3:24 → "$result"
        4:9-4:15 → "$result""#]]
    .assert_eq(&out);
}

/// Edge case: arrow function with multiple captures and nested operations.
#[tokio::test]
async fn rename_variable_in_nested_arrow_function() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function compute(): void {
    $base$0 = 10;
    $offset = 5;
    $calc = fn() => fn() => $base + $offset;
    echo $base;
}
"#,
            "$initial",
        )
        .await;
    expect![[r#"
        // main.php
        2:4-2:9 → "$initial"
        4:28-4:33 → "$initial"
        5:9-5:14 → "$initial""#]]
    .assert_eq(&out);
}

/// Edge case: arrow function in array passed as argument.
#[tokio::test]
async fn rename_variable_in_arrow_in_array() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function process(): void {
    $multiplier$0 = 2;
    $mappers = [
        fn($x) => $x * $multiplier,
        fn($y) => $y + $multiplier,
    ];
    echo $multiplier;
}
"#,
            "$factor",
        )
        .await;
    // Arrow functions in array should capture references
    expect![[r#"
        // main.php
        2:4-2:15 → "$factor"
        4:23-4:34 → "$factor"
        5:23-5:34 → "$factor"
        7:9-7:20 → "$factor""#]]
    .assert_eq(&out);
}

/// Regression: closure use() clause variables were not being collected during rename.
/// Previously, VarRefsVisitor returned early on closures without checking use_vars,
/// leaving the use() clause pointing to the old undefined name.
/// Bug #3 from ROADMAP: closures now collect use() references before stopping.
#[tokio::test]
async fn rename_variable_in_closure_use_clause() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function greet(): void {
    $name$0 = "Alice";
    $greeting = function() use ($name) {
        echo "Hello " . $name;
    };
    echo $name;
}
"#,
            "$person",
        )
        .await;
    expect![[r#"
        // main.php
        2:4-2:9 → "$person"
        3:32-3:37 → "$person"
        6:9-6:14 → "$person""#]]
    .assert_eq(&out);
}

/// Edge case: closure use() clause with reference binding.
#[tokio::test]
async fn rename_variable_in_closure_use_by_reference() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function counter(): void {
    $count$0 = 0;
    $increment = function() use (&$count) {
        $count++;
    };
    $increment();
    echo $count;
}
"#,
            "$total",
        )
        .await;
    expect![[r#"
        // main.php
        2:4-2:10 → "$total"
        3:33-3:40 → "$total"
        7:9-7:15 → "$total""#]]
    .assert_eq(&out);
}

/// Edge case: closure with multiple use() variables.
/// All variables in the use clause should be collected and renamed.
#[tokio::test]
async fn rename_variable_in_closure_multiple_use_vars() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
function process(): void {
    $input$0 = "data";
    $output = "";
    $debug = false;
    $handler = function() use ($input, $output, $debug) {
        if ($debug) {
            echo $input . $output;
        }
    };
    $handler();
}
"#,
            "$data",
        )
        .await;
    // Should rename declaration and the use clause reference
    expect![[r#"
        // main.php
        2:4-2:10 → "$data"
        5:31-5:37 → "$data""#]]
    .assert_eq(&out);
}

/// Regression: rename with same-named symbols in different namespaces was not FQN-aware.
/// Previously, rename() called find_references_with_use without FQN context,
/// causing cross-namespace false matches.
/// Bug #4 from ROADMAP: namespace-aware rename now resolves target FQN.
/// This test demonstrates that renaming a class resolves its FQN correctly
/// within a single namespace scope.
#[tokio::test]
async fn rename_within_namespace_scope() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
namespace App;
class Logger$0 {}
function create() {
    $l = new Logger();
}
"#,
            "Reporter",
        )
        .await;
    // Rename resolves FQN and applies throughout the namespace
    expect![[r#"
        // main.php
        2:6-2:12 → "Reporter"
        4:13-4:19 → "Reporter""#]]
    .assert_eq(&out);
}

/// Edge case: aliased use imports must rename the alias, not the original class name.
/// This is critical: renaming by alias should only affect that alias, not the class itself.
#[tokio::test]
async fn rename_aliased_use_import() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
use App\Logger as Log;
$l = new Log$0();
"#,
            "Reporter",
        )
        .await;
    // Should rename the alias "Log" to "Reporter" in the use statement
    expect![[r#"
        // main.php
        1:18-1:21 → "Reporter"
        2:9-2:12 → "Reporter""#]]
    .assert_eq(&out);
}

/// Edge case: multiple imports in a single use statement must all be updated.
/// Test: use A\Foo, B\Bar; when renaming Foo
#[tokio::test]
async fn rename_with_multiple_use_imports() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_rename(
            r#"<?php
use App\Logger, App\Parser;
$l = new Logger$0();
$p = new Parser();
"#,
            "Reporter",
        )
        .await;
    // Only Logger is renamed, Parser stays the same
    expect![[r#"
        // main.php
        1:8-1:14 → "Reporter"
        2:9-2:15 → "Reporter""#]]
    .assert_eq(&out);
}
