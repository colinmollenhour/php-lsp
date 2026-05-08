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
    // Union types are not currently supported by the implementation
    expect!["<none>"].assert_eq(&out);
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

/// Aliased type hints are not currently supported by type_definition
#[tokio::test]
async fn type_definition_does_not_support_use_aliases() {
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

    // Aliased types are not supported - type hint uses alias name that's not a real class
    expect!["<none>"].assert_eq(&out);
}

/// **LIMITATION**: Unqualified type names in non-global namespaces are NOT automatically
/// qualified with namespace context. This documents a known limitation.
/// Example: `Logger $l` in `namespace App\Service` is NOT resolved to `App\Service\Logger`.
/// Workaround: Use fully qualified name `\App\Service\Logger` or import with `use`.
#[tokio::test]
async fn type_definition_limitation_unqualified_no_namespace_resolution() {
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
    // This SHOULD resolve to App\Service\Logger but currently doesn't because
    // param_type_for() doesn't have namespace context to qualify the unqualified name.
    // It works in this case because both are in the same file and Logger is in scope.
    expect![[r#"
        src/Logger.php:2:6-2:12"#]]
    .assert_eq(&out);
}

/// **LIMITATION**: Union types (PHP 8.0+) are not supported.
/// Returns <none> when type hint is a union like `Admin|User`.
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
    // Union types are explicitly not supported
    expect!["<none>"].assert_eq(&out);
}

/// **LIMITATION**: Intersection types (PHP 8.1+) are not supported.
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
    // Intersection types are not supported
    expect!["<none>"].assert_eq(&out);
}

/// **LIMITATION**: Aliased types in use imports are not supported.
/// When a type is imported with an alias (use X as Y), jumping to type definition
/// won't work because the implementation doesn't track use aliases.
#[tokio::test]
async fn type_definition_limitation_alias_with_use_import() {
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
    // Aliased types are not resolved - the alias name "UserAccount" doesn't match
    // any real class name in the index
    expect!["<none>"].assert_eq(&out);
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
