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
#[tokio::test]
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
#[tokio::test]
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
#[tokio::test]
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
