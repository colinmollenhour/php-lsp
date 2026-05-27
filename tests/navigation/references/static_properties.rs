//! Static property references.
//!
//! Tests static property declarations and usages:
//! - ClassName::$property access
//! - self::$property within class scope
//! - parent::$property in inheritance
//! - Static properties with type hints
//! - Static properties in various expressions

use super::*;

#[tokio::test]
async fn static_property_basic_access() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Registry {
    public static string $instan$0ce = '';
    //                    ^^^^^^^^ def
}

$r = Registry::$instance;
//              ^^^^^^^^ ref

Registry::$instance = 'value';
//         ^^^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_self_reference() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Cache {
    public static array $ite$0ms = [];
    //                   ^^^^^ def

    public static function add(string $key): void {
        self::$items[$key] = true;
        //     ^^^^^ ref
    }

    public static function get(string $key): mixed {
        return self::$items[$key] ?? null;
        //            ^^^^^ ref
    }

    public static function clear(): void {
        self::$items = [];
        //     ^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_parent_reference() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class BaseStorage {
    protected static array $coun$0t = [];
    //                      ^^^^^ def
}

class ExtendedStorage extends BaseStorage {
    public static function increment(): void {
        parent::$count[] = 1;
        //       ^^^^^ ref
    }

    public static function reset(): void {
        parent::$count = [];
        //       ^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_cross_file() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /Registry.php
<?php
class ServiceRegistry {
    public static ?\Psr\Container $contai$0ner = null;
    //                             ^^^^^^^^^ def
}

//- /Service.php
<?php
function getService(string $name): mixed {
    return ServiceRegistry::$container?->get($name);
    //                       ^^^^^^^^^ ref
}

class Bootstrap {
    public static function init(\Psr\Container $c): void {
        ServiceRegistry::$container = $c;
        //                ^^^^^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_in_condition() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class AppState {
    public static bool $initia$0lized = false;
    //                  ^^^^^^^^^^^ def
}

function bootstrap(): void {
    if (!AppState::$initialized) {
    //              ^^^^^^^^^^^ ref
        setup();
        AppState::$initialized = true;
        //         ^^^^^^^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_in_foreach() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Logger {
    public static array $message$0s = [];
    //                   ^^^^^^^^ def
}

function logErrors(): void {
    foreach (Logger::$messages as $msg) {
    //                ^^^^^^^^ ref
        echo $msg;
    }

    Logger::$messages = [];
    //       ^^^^^^^^ ref
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_visibility_levels() {
    // Test static properties with different visibility modifiers
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class AccessControl {
    public static string $public$0_val = '';
    //                    ^^^^^^^^^^ def
    protected static string $protected_val = '';
    private static string $private_val = '';
}

class External {
    public function test(): void {
        $val = AccessControl::$public_val;
        //                     ^^^^^^^^^^ ref
    }
}

class Internal extends AccessControl {
    public function test(): void {
        echo self::$protected_val;
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_multiple_assignments() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Counter {
    public static int $valu$0e = 0;
    //                 ^^^^^ def
}

function incrementCounter(): void {
    Counter::$value++;
    //        ^^^^^ ref
}

function resetCounter(): void {
    Counter::$value = 0;
    //        ^^^^^ ref
}

function getCounter(): int {
    return Counter::$value;
    //               ^^^^^ ref
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_in_static_initializer() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Config {
    public static string $app$0_name = '';
    //                    ^^^^^^^^ def
    public static string $version = '1.0';

    public static function init(): void {
        self::$app_name = 'MyApp';
        //     ^^^^^^^^ ref
        self::$version = self::$app_name . ' v' . self::$version;
        //                      ^^^^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn static_property_type_safety() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Database {
    public static ?\PDO $connectio$0n = null;
    //                   ^^^^^^^^^^ def
}

class QueryBuilder {
    public function execute(): void {
        if (Database::$connection === null) {
        //             ^^^^^^^^^^ ref
            throw new Exception('Not connected');
        }

        Database::$connection->query('SELECT 1');
        //         ^^^^^^^^^^ ref
    }
}
"#,
    )
    .await;
}
