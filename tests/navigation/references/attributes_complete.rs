//! Comprehensive attribute references across all declaration types.
//!
//! Tests attribute usage on:
//! - Classes, abstract classes, final classes
//! - Methods, static methods
//! - Properties, static properties
//! - Functions
//! - Parameters (function, method, property promotion)
//! - Enums and enum cases
//! - Multiple attributes on same element

use super::*;

#[ignore]
#[tokio::test]
async fn attribute_on_class() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Entity$0Attribute {}
//    ^^^^^^^^^^^^^^^^^ def

#[EntityAttribute]
// ^^^^^^^^^^^^^^^^ ref
class User {}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_method() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Route$0Attribute {}
//    ^^^^^^^^^^^^^^ def

class Controller {
    #[RouteAttribute('/users', 'GET')]
    // ^^^^^^^^^^^^^^ ref
    public function getUsers(): array {
        return [];
    }
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_property() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Serialize$0Attribute {}
//    ^^^^^^^^^^^^^^^^^ def

class Document {
    #[SerializeAttribute]
    // ^^^^^^^^^^^^^^^^^ ref
    public string $content;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_function() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Depreca$0ted {}
//    ^^^^^^^^^^ def

#[Deprecated]
// ^^^^^^^^^^ ref
function oldFunction(): void {
    echo 'This is deprecated';
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_parameter() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Validate$0Attribute {}
//    ^^^^^^^^^^^^^^^^^^ def

function process(
    #[ValidateAttribute('email')]
    // ^^^^^^^^^^^^^^^^^^ ref
    string $email
): void {
    // validate
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_promoted_property() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Immuta$0ble {}
//    ^^^^^^^^^ def

class User {
    public function __construct(
        #[Immutable]
        // ^^^^^^^^^ ref
        public readonly string $id,
    ) {}
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_enum() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Backable$0Enum {}
//    ^^^^^^^^^^^^^^^ def

#[BackableEnum]
// ^^^^^^^^^^^^^^ ref
enum Status: int {
    case Active = 1;
    case Inactive = 0;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_enum_case() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Default$0Case {}
//    ^^^^^^^^^ def

enum Priority {
    #[DefaultCase]
    // ^^^^^^^^^^^ ref
    case Low;
    case Medium;
    case High;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_multiple_on_method() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Async {}
class Cach$0ed {}

class Service {
    #[Async]
    // ^^^^^ ref
    #[Cached]
    // ^^^^^^ ref
    public function fetchData(): array {
        return [];
    }
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_with_constructor_arguments() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Required$0Arg {}
//    ^^^^^^^^^^^^^ def

class Form {
    #[RequiredArg('email', 'string')]
    // ^^^^^^^^^^^^^ ref
    public string $email;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_repeatable() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Permiss$0ion {}
//    ^^^^^^^^^^ def

class AdminResource {
    #[Permission('read')]
    // ^^^^^^^^^ ref
    #[Permission('admin')]
    // ^^^^^^^^^ ref
    public function manage(): void {}
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_static_property() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Singleton$0Marker {}
//    ^^^^^^^^^^^^^^^^^ def

class Cache {
    #[SingletonMarker]
    // ^^^^^^^^^^^^^^^^ ref
    public static Cache $instance;

    public static function getInstance(): self {
        return self::$instance;
    }
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_cross_file() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /src/Attributes.php
<?php
class API$0Endpoint {}
//    ^^^^^^^^^^^^ def

//- /src/Controllers/UserController.php
<?php
class UserController {
    #[ApiEndpoint('/users', 'GET')]
    // ^^^^^^^^^^^^ ref
    public function list(): array {}

    #[ApiEndpoint('/users/{id}', 'GET')]
    // ^^^^^^^^^^^^ ref
    public function show(int $id): array {}
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_abstract_class() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Abstract$0Base {}
//    ^^^^^^^^^^^^^ def

#[AbstractBase]
// ^^^^^^^^^^^^^ ref
abstract class Handler {
    abstract public function handle(): void;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_final_class() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Immutab$0le {}
//    ^^^^^^^^^^ def

#[Immutable]
// ^^^^^^^^^^ ref
final class Config {
    public readonly string $path;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_interface_method() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Required$0Impl {}
//    ^^^^^^^^^^^^^^^ def

interface Repository {
    #[RequiredImpl]
    // ^^^^^^^^^^^^^^ ref
    public function find(int $id): mixed;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn attribute_on_static_method() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Singleton$0Factory {}
//    ^^^^^^^^^^^^^^^^^ def

class Database {
    #[SingletonFactory]
    // ^^^^^^^^^^^^^^^^^ ref
    public static function getInstance(): self {
        static $instance = null;
        if ($instance === null) {
            $instance = new self();
        }
        return $instance;
    }
}
"#,
    )
    .await;
}
