//! Type definition (`textDocument/typeDefinition`) coverage.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn type_definition_variable_to_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Foo {}
$obj = new Foo();
$obj$0->bar();
"#,
        )
        .await;
    expect![[r#"
        main.php:1:6-1:9"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_cross_file() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /a.php
<?php
$obj = new Mailer();
$obj$0->send();

//- /mailer.php
<?php
class Mailer {}
"#,
        )
        .await;
    expect![[r#"
        mailer.php:1:6-1:12"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_unknown_variable() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
$unknown$0->foo();
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

#[tokio::test]
async fn type_definition_interface_type() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
interface Countable {}
$obj = new MyList();
$obj$0->count();
class MyList implements Countable {}
"#,
        )
        .await;
    expect![[r#"
        main.php:4:6-4:12"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_enum_typed_param() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
enum Status { case Active; }
function process(Status $s): void { $s$0-> }
"#,
        )
        .await;
    expect![[r#"
        main.php:1:5-1:11"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_trait_typed_param() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
trait Logger {}
function process(Logger $l): void { $l$0-> }
"#,
        )
        .await;
    expect![[r#"
        main.php:1:6-1:12"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_variable_from_new_expr() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Widget {}
$w = new Widget();
echo $w$0;
"#,
        )
        .await;
    expect![[r#"
        main.php:1:6-1:12"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_non_variable_without_type() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function greet() {}
gree$0t();
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

#[tokio::test]
async fn type_definition_with_use_import() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Mailer.php
<?php
namespace Vendor;
class Mailer {}

//- /src/main.php
<?php
use Vendor\Mailer;
$m = new Mailer();
$m$0->send();
"#,
        )
        .await;
    expect![[r#"
        src/Mailer.php:2:6-2:12"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_nullable_type() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class User {}
function process(?User $u$0): void {}
"#,
        )
        .await;
    expect![[r#"
        main.php:1:6-1:10"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_union_type_not_supported() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Admin {}
class User {}
function process(Admin|User $u$0): void {}
"#,
        )
        .await;
    // Union types return the first matching type in the union
    expect![[r#"
        main.php:1:6-1:11"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_fully_qualified_parameter() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
namespace App;
class Service {}
function process(\App\Service $s$0): void {}
"#,
        )
        .await;
    // Should resolve FQN type hints
    expect![[r#"
        main.php:2:6-2:13"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_cursor_on_param_name() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Logger {}
function log(Logger $l$0): void {}
"#,
        )
        .await;
    // When cursor is on param name, should resolve to param's type
    expect![[r#"
        main.php:1:6-1:12"#]]
    .assert_eq(&out);
}

// ── Tests for indexed type definition (background files) ────

/// Type definition should resolve types from background-indexed files
/// This tests the goto_type_definition_from_index code path.
/// Note: The indexed version returns the class keyword location from the index.
#[tokio::test]
async fn type_definition_from_background_indexed_file() {
    let mut s = TestServer::with_fixture("psr4-mini").await;
    // Wait for background indexing to complete so files are in the index
    s.wait_for_index_ready().await;

    // Now test type resolution from an indexed file
    let out = s
        .check_type_definition(
            r#"<?php
namespace App;
use App\Model\User;
$u = new User();
$u$0->getName();
"#,
        )
        .await;

    // Should resolve to User class from index (indexed version returns class keyword location)
    expect![[r#"
        src/Model/User.php:4:0-4:0"#]]
    .assert_eq(&out);
}

/// Aliased type hints in `use X as Y` are resolved via `collect_file_imports`.
/// This covers both open-docs and background-index paths.
#[tokio::test]
async fn type_definition_alias_resolved_from_index() {
    let mut s = TestServer::with_fixture("psr4-mini").await;
    s.wait_for_index_ready().await;

    let out = s
        .check_type_definition(
            r#"<?php
namespace App\Service;
use App\Model\User as UserModel;
function create(UserModel $u$0): void {}
"#,
        )
        .await;

    // Alias is resolved to the real FQN App\Model\User → finds the class in index
    expect![[r#"
        src/Model/User.php:4:0-4:0"#]]
    .assert_eq(&out);
}

/// Unqualified type names in non-global namespaces are resolved with namespace context.
/// `Logger $l` in `namespace App\Service` resolves to `App\Service\Logger` via resolve_fqn.
#[tokio::test]
async fn type_definition_unqualified_param_in_namespace_resolves_correctly() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Logger.php
<?php
namespace App\Service;
class Logger {}

//- /src/Service.php
<?php
namespace App\Service;
class Service {
    public function log(Logger $l$0): void {}
}
"#,
        )
        .await;
    // param_type_for returns "Logger", resolve_fqn qualifies it to "App\Service\Logger",
    // and the FQN-scoped search finds the correct file.
    expect![[r#"
        src/Logger.php:2:6-2:12"#]]
    .assert_eq(&out);
}

/// Union types (PHP 8.0+) return the first matching type in the union.
/// TODO: Support returning all matching types in union, or at least document clearly.
#[tokio::test]
#[ignore]
async fn type_definition_limitation_union_types_not_supported() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Admin {}
class User {}
function authenticate(Admin|User $a$0): void {}
"#,
        )
        .await;
    // Union types return the first matching type
    expect![[r#"
        main.php:1:6-1:11"#]]
    .assert_eq(&out);
}

/// **LIMITATION**: Intersection types (PHP 8.1+) are not currently supported.
/// `type_hint_to_class_string` returns `None` for intersection type hints, so TypeMap
/// has no entry for the variable and type_definition returns nothing.
/// TODO: Implement intersection type support (PHP 8.1+).
#[tokio::test]
#[ignore]
async fn type_definition_limitation_intersection_types_not_supported() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
interface Readable {}
interface Writable {}
function process(Readable&Writable $rw$0): void {}
"#,
        )
        .await;
    // Intersection types are not yet supported
    expect!["<none>"].assert_eq(&out);
}

/// Aliased types in use imports are resolved via `collect_file_imports` which
/// tracks `use X as Y` mappings. Jumping to type definition works correctly.
#[tokio::test]
async fn type_definition_alias_with_use_import() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/Account.php
<?php
namespace App\Model;
class Account {}

//- /src/Service.php
<?php
namespace App\Service;
use App\Model\Account as UserAccount;
function create(UserAccount $acc$0): void {}
"#,
        )
        .await;
    expect![[r#"
        src/Model/Account.php:2:6-2:13"#]]
    .assert_eq(&out);
}

/// **LIMITATION**: Generic-like syntax (e.g., Collection<User>) is not supported.
/// The type hint parser doesn't understand generic syntax.
/// TODO: Parse and handle generic-like type syntax.
#[tokio::test]
#[ignore]
async fn type_definition_limitation_generic_types_not_supported() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Collection {}
class User {}
function process(Collection<User> $items$0): void {}
"#,
        )
        .await;
    // Generic syntax isn't recognized - Collection<User> is parsed as something unexpected
    expect!["<none>"].assert_eq(&out);
}

/// Enum method parameters should have type definitions resolved.
/// Regression: param_type_for previously did not check StmtKind::Enum.
#[tokio::test]
async fn type_definition_enum_method_parameter() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Logger {}
enum Status {
    case Active;
    public function log(Logger $l$0): void {}
}
"#,
        )
        .await;
    expect![[r#"
        main.php:1:6-1:12"#]]
    .assert_eq(&out);
}

/// When multiple classes share a short name, exact FQN match should be preferred.
/// Regression: goto_type_definition_from_index previously returned first short name match.
#[tokio::test]
async fn type_definition_prefers_exact_fqn_over_short_name() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/User.php
<?php
namespace App\Model;
class User {}

//- /src/Service/User.php
<?php
namespace App\Service;
class User {}

//- /src/main.php
<?php
namespace App\Service;
function create(User $u$0): void {}
"#,
        )
        .await;
    // Should resolve to App\Service\User (exact FQN match), not App\Model\User
    expect![[r#"
        src/Service/User.php:2:6-2:10"#]]
    .assert_eq(&out);
}

/// Unqualified type names in non-global namespaces should be resolved with namespace context.
/// Regression: param_type_for previously didn't qualify unqualified names.
#[tokio::test]
async fn type_definition_unqualified_name_in_namespace() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/User.php
<?php
namespace App\Model;
class User {}

//- /src/Service/UserService.php
<?php
namespace App\Service;
use App\Model\User;
class UserService {
    public function getUser(User $user$0): void {}
}
"#,
        )
        .await;
    // Should resolve to App\Model\User despite being in App\Service namespace
    expect![[r#"
        src/Model/User.php:2:6-2:10"#]]
    .assert_eq(&out);
}

// ── Regression tests for $var FQN resolution ─────────────────────────────────

/// `$var = new Class()` in a namespace: TypeMap stores only the short class name,
/// but resolve_fqn must qualify it to the file's namespace so the FQN-scoped
/// search picks the right file when two classes share the same short name.
#[tokio::test]
async fn type_definition_var_new_in_namespace_prefers_same_namespace() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/Order.php
<?php
namespace App\Model;
class Order {}

//- /src/Service/Order.php
<?php
namespace App\Service;
class Order {}

//- /src/Service/Processor.php
<?php
namespace App\Service;
$order = new Order();
$order$0->process();
"#,
        )
        .await;
    // $order is `new Order()` in namespace App\Service, so it should resolve to
    // App\Service\Order, not App\Model\Order
    expect![[r#"
        src/Service/Order.php:2:6-2:11"#]]
    .assert_eq(&out);
}

/// `$var = new Class()` in a namespace with a `use` import: the import overrides
/// the namespace prefix, so $var should resolve to the imported class.
#[tokio::test]
async fn type_definition_var_new_with_use_import_overrides_namespace() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/Invoice.php
<?php
namespace App\Model;
class Invoice {}

//- /src/Service/Invoice.php
<?php
namespace App\Service;
class Invoice {}

//- /src/Billing/Creator.php
<?php
namespace App\Billing;
use App\Model\Invoice;
$inv = new Invoice();
$inv$0->total();
"#,
        )
        .await;
    // use App\Model\Invoice is explicit, so $inv resolves to App\Model\Invoice
    expect![[r#"
        src/Model/Invoice.php:2:6-2:13"#]]
    .assert_eq(&out);
}

/// Typed parameter in a class method (not a top-level function) in a namespace.
/// Regression: param_type_for must recurse into class members.
#[tokio::test]
async fn type_definition_method_param_in_namespaced_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/Product.php
<?php
namespace App\Model;
class Product {}

//- /src/Service/Cart.php
<?php
namespace App\Service;
use App\Model\Product;
class Cart {
    public function addItem(Product $item$0): void {}
}
"#,
        )
        .await;
    expect![[r#"
        src/Model/Product.php:2:6-2:13"#]]
    .assert_eq(&out);
}

/// Nullable type `?ClassName` in a namespace resolves the inner class by FQN.
#[tokio::test]
async fn type_definition_nullable_type_in_namespace() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/Address.php
<?php
namespace App\Model;
class Address {}

//- /src/Service/Address.php
<?php
namespace App\Service;
class Address {}

//- /src/Handler.php
<?php
namespace App\Service;
function deliver(?Address $addr$0): void {}
"#,
        )
        .await;
    // ?Address in App\Service namespace resolves to App\Service\Address
    expect![[r#"
        src/Service/Address.php:2:6-2:13"#]]
    .assert_eq(&out);
}

/// Braced namespace form: both the calling file and the target class use
/// `namespace Foo { ... }` syntax.
#[tokio::test]
async fn type_definition_braced_namespace() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Model/Report.php
<?php
namespace App\Model {
    class Report {}
}

//- /src/Service/Report.php
<?php
namespace App\Service {
    class Report {}
}

//- /src/Runner.php
<?php
namespace App\Service {
    function run(Report $r$0): void {}
}
"#,
        )
        .await;
    // Report in braced App\Service namespace → App\Service\Report (indented class, col 10)
    expect![[r#"
        src/Service/Report.php:2:10-2:16"#]]
    .assert_eq(&out);
}

/// Deeply nested namespace (A\B\C) — resolve_fqn must handle multi-segment prefix.
#[tokio::test]
async fn type_definition_deeply_nested_namespace() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Cmd.php
<?php
namespace App\Console\Command;
class Cmd {}

//- /src/Other/Cmd.php
<?php
namespace App\Http\Controller;
class Cmd {}

//- /src/Dispatch.php
<?php
namespace App\Console\Command;
function dispatch(Cmd $c$0): void {}
"#,
        )
        .await;
    // Cmd in App\Console\Command → App\Console\Command\Cmd, not App\Http\Controller\Cmd
    expect![[r#"
        src/Cmd.php:2:6-2:9"#]]
    .assert_eq(&out);
}

// ── Regression tests for index (background file) FQN resolution ──────────────

/// Background-indexed class: `$var` in a namespace resolves without an explicit
/// `use` import — the namespace itself qualifies the short class name to an FQN.
#[tokio::test]
async fn type_definition_index_var_namespace_resolves_without_import() {
    let mut s = TestServer::with_fixture("psr4-mini").await;
    s.wait_for_index_ready().await;

    let out = s
        .check_type_definition(
            r#"<?php
namespace App\Model;
$u = new User();
$u$0->greet();
"#,
        )
        .await;
    // No explicit `use` — namespace App\Model qualifies User to App\Model\User,
    // which the index finds directly via FQN match.
    expect![[r#"
        src/Model/User.php:4:0-4:0"#]]
    .assert_eq(&out);
}

/// Background-indexed class: typed parameter with `use` alias, index path.
/// Tests that goto_type_definition_from_index also resolves aliases.
#[tokio::test]
async fn type_definition_index_param_alias_resolved() {
    let mut s = TestServer::with_fixture("psr4-mini").await;
    s.wait_for_index_ready().await;

    let out = s
        .check_type_definition(
            r#"<?php
namespace App\Service;
use App\Model\User as UserModel;
function greet(UserModel $u$0): void {}
"#,
        )
        .await;
    // Alias UserModel resolved to App\Model\User via imports; index finds it
    expect![[r#"
        src/Model/User.php:4:0-4:0"#]]
    .assert_eq(&out);
}

/// Unqualified type hints resolve within the same namespace.
#[tokio::test]
async fn type_definition_unqualified_name_same_namespace() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Logger.php
<?php
namespace App;
class Logger {}

//- /src/Service.php
<?php
namespace App;
class Service {
    public function log(Logger $l$0): void {}
}
"#,
        )
        .await;
    // Unqualified Logger in App namespace should resolve to App\Logger
    expect![[r#"
        src/Logger.php:2:6-2:12"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn type_definition_not_confused_by_use_function_import() {
    // `use function` imports must not pollute the class-import map: a type hint
    // `format $x` where `format` also appears in `use function Lib\format` should
    // resolve to the same-namespace class `App\format`, not to `Lib\format`.
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /main.php
<?php
namespace App;
use function Lib\format;

function go(format $x$0): void {}

//- /format.php
<?php
namespace App;
class format {}
"#,
        )
        .await;
    expect![[r#"
        format.php:2:6-2:12"#]]
    .assert_eq(&out);
}

// ── Built-in and Special Types ────────────────────────────────────────

/// Built-in scalar types (int, string, bool, etc.) have no type definition.
#[tokio::test]
async fn type_definition_builtin_int_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function count(int $n$0): void {}
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

#[tokio::test]
async fn type_definition_builtin_string_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function message(string $msg$0): void {}
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

#[tokio::test]
async fn type_definition_builtin_bool_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function check(bool $flag$0): void {}
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

#[tokio::test]
async fn type_definition_builtin_mixed_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function handle(mixed $data$0): void {}
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

#[tokio::test]
async fn type_definition_builtin_never_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function crash(never $x$0): void {}
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

/// stdClass is a built-in class and should be resolvable.
#[tokio::test]
async fn type_definition_stdclass_builtin() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function object_param(stdClass $obj$0): void {}
"#,
        )
        .await;
    // stdClass is a built-in class; type definition returns None (not in workspace)
    expect!["<none>"].assert_eq(&out);
}

// ── Array and Collection Types ────────────────────────────────────────

/// Array type hint with generic-like documentation syntax (PHPDoc style).
/// Note: `User[]` is only valid in PHPDoc, not as actual parameter type hint.
/// Using generic-like syntax in actual type hints is not standard PHP.
#[tokio::test]
async fn type_definition_array_of_class_via_generic_syntax() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class User {}
/** @param User[] $users */
function batch(array $users$0): void {}
"#,
        )
        .await;
    // Type hint is `array` (built-in), not a class type
    expect!["<none>"].assert_eq(&out);
}

/// Built-in `array` type returns None (it's not a class).
#[tokio::test]
async fn type_definition_array_builtin_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
function items(array $data$0): void {}
"#,
        )
        .await;
    expect!["<none>"].assert_eq(&out);
}

// ── Variable Assignment and Factory Methods ────────────────────────────

/// Variable assigned from another variable's type.
/// TypeMap only tracks direct `new ClassName()` assignments, not variable-to-variable.
/// TODO: Enhance TypeMap to track variable-to-variable assignment chains.
#[tokio::test]
#[ignore]
async fn type_definition_variable_assigned_from_other() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Result {}
$value = new Result();
$copy = $value;
$copy$0->process();
"#,
        )
        .await;
    // Variable assignment chains are not tracked - TypeMap only tracks direct `new`
    expect!["<none>"].assert_eq(&out);
}

/// Nullable union type resolution.
/// Note: `?Success|Error` is parsed as nullable Success or Error, both nullable.
/// TODO: Support union types - currently returns only first match.
#[tokio::test]
#[ignore]
async fn type_definition_nullable_union_type() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Success {}
class Error {}
function handle(Success|Error $result$0): void {}
"#,
        )
        .await;
    // Union types return first match
    expect![[r#"
        main.php:1:6-1:13"#]]
    .assert_eq(&out);
}

// ── Self, Parent, Static Keywords ────────────────────────────────────────

/// `self` keyword in class parameter resolves to the containing class.
#[tokio::test]
async fn type_definition_self_keyword_in_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class User {
    public function duplicate(self $other$0): self {}
}
"#,
        )
        .await;
    // `self` is resolved to the containing class (User)
    expect![[r#"
        main.php:1:6-1:10"#]]
    .assert_eq(&out);
}

/// `parent` keyword in class parameter resolves to the enclosing class, not the parent.
/// This is a known limitation - `parent` should resolve to the parent class, not the child.
/// TODO: Implement proper `parent` keyword resolution using class inheritance info.
#[tokio::test]
#[ignore]
async fn type_definition_parent_keyword_limitation() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Base {}
class Child extends Base {
    public function get_parent(parent $p$0): void {}
}
"#,
        )
        .await;
    // `parent` is parsed as the enclosing class (Child) - limitation
    // In PHP, this should resolve to Base, not Child
    expect![[r#"
        main.php:2:6-2:11"#]]
    .assert_eq(&out);
}

/// `static` keyword cannot be used as a parameter type hint (only valid for return types).
/// This test verifies that attempting to resolve a parameter named `static` fails.
#[tokio::test]
#[ignore]
async fn type_definition_static_return_type() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Factory {
    public function create(): static { return new static(); }
    public function use_it(Factory $f$0): void {}
}
"#,
        )
        .await;
    // Cursor is on $f parameter of type Factory, should resolve to Factory
    expect![[r#"
        main.php:1:6-1:13"#]]
    .assert_eq(&out);
}

// ── Trait-Specific Cases ───────────────────────────────────────────────

/// Type hints in trait methods should resolve correctly.
#[tokio::test]
async fn type_definition_trait_with_class_param() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Config {}
trait Settings {
    public function load(Config $cfg$0): void {}
}
"#,
        )
        .await;
    expect![[r#"
        main.php:1:6-1:12"#]]
    .assert_eq(&out);
}

/// Trait with cross-file type hint.
#[tokio::test]
async fn type_definition_trait_cross_file_param() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/db.php
<?php
class Connection {}

//- /src/main.php
<?php
trait Database {
    public function query(Connection $conn$0): void {}
}
"#,
        )
        .await;
    expect![[r#"
        src/db.php:1:6-1:16"#]]
    .assert_eq(&out);
}

// ── Enum Backed Types ─────────────────────────────────────────────────

/// Backed enum (int-backed) with method parameter.
#[tokio::test]
async fn type_definition_backed_enum_int_param() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Logger {}
enum Priority: int {
    case HIGH = 1;
    case LOW = 0;
    public function log(Logger $logger$0): void {}
}
"#,
        )
        .await;
    expect![[r#"
        main.php:1:6-1:12"#]]
    .assert_eq(&out);
}

/// Backed enum (string-backed) typed as parameter.
#[tokio::test]
async fn type_definition_backed_enum_as_parameter() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
enum Status: string {
    case ACTIVE = 'active';
    case INACTIVE = 'inactive';
}
function process(Status $status$0): void {}
"#,
        )
        .await;
    expect![[r#"
        main.php:1:5-1:11"#]]
    .assert_eq(&out);
}

// ── Interface Inheritance ──────────────────────────────────────────────

/// Parameter typed as interface that extends another.
#[tokio::test]
async fn type_definition_extended_interface() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
interface Animal {}
interface Pet extends Animal {}
function adopt(Pet $pet$0): void {}
"#,
        )
        .await;
    expect![[r#"
        main.php:2:10-2:13"#]]
    .assert_eq(&out);
}

/// Multiple interface inheritance (one class implements two).
#[tokio::test]
async fn type_definition_multi_interface_param() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
interface Logger {}
interface Config {}
class App implements Logger, Config {}
function bootstrap(App $app$0): void {}
"#,
        )
        .await;
    expect![[r#"
        main.php:3:6-3:9"#]]
    .assert_eq(&out);
}

// ── Import and Namespace Conflicts ────────────────────────────────────

/// When both a use import and a local class have the same short name.
/// Current behavior: resolves to local class in same namespace (not respecting import).
/// This is a known limitation - imports are not fully respected in resolution.
/// TODO: Fix import precedence - `use` imports should override same-namespace classes.
#[tokio::test]
#[ignore]
async fn type_definition_import_with_local_class_same_name() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Logger.php
<?php
namespace App;
class Logger {}

//- /src/Service.php
<?php
namespace App;
use Different\Logger;

function log(Logger $l$0): void {}
"#,
        )
        .await;
    // When both import and local class exist, local class is found first
    // This is the current behavior; imports don't fully override same-namespace classes
    expect![[r#"
        src/Logger.php:2:6-2:12"#]]
    .assert_eq(&out);
}

/// Aliased import with conflict.
/// TODO: Fix aliased import resolution - should respect `use ... as ...` aliases.
#[tokio::test]
#[ignore]
async fn type_definition_aliased_import_with_local_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"//- /src/Logger.php
<?php
namespace App;
class Logger {}

//- /src/Service/Logger.php
<?php
namespace App\Service;
class Logger {}

//- /src/Processor.php
<?php
namespace App\Service;
use App\Logger as AppLogger;

function log(AppLogger $l$0): void {}  // Explicitly uses alias
"#,
        )
        .await;
    // Alias resolves to App\Logger despite local Service\Logger existing
    expect![[r#"
        src/Logger.php:2:6-2:12"#]]
    .assert_eq(&out);
}

// ── Cursor Position Variants ───────────────────────────────────────────

/// Cursor on parameter name (not type).
#[tokio::test]
async fn type_definition_cursor_on_param_name_value() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
class Handler {}
function process(Handler $h$0andler): void {}
"#,
        )
        .await;
    // Cursor is on $handler, not just $h
    expect![[r#"
        main.php:1:6-1:13"#]]
    .assert_eq(&out);
}

/// Cursor on variable without type hint.
#[tokio::test]
async fn type_definition_untyped_variable_in_function() {
    let mut s = TestServer::new().await;
    let out = s
        .check_type_definition(
            r#"<?php
$untyped$0 = 123;
"#,
        )
        .await;
    // Variable with no type hint or assignment has no type
    expect!["<none>"].assert_eq(&out);
}

// ── Edge Cases with Index Resolution ───────────────────────────────────

/// When same class name exists in both index and open file, prefer exact FQN match.
#[tokio::test]
async fn type_definition_index_prefers_exact_fqn() {
    let mut s = TestServer::with_fixture("psr4-mini").await;
    s.wait_for_index_ready().await;

    let out = s
        .check_type_definition(
            r#"<?php
namespace App\Model;
function test(User $u$0): void {}
"#,
        )
        .await;
    // Should resolve to App\Model\User from index (exact FQN)
    expect![[r#"
        src/Model/User.php:4:0-4:0"#]]
    .assert_eq(&out);
}
