//! Class constants and global constant references.
//!
//! Tests constant declarations and usages:
//! - Class-level constants (const declarations)
//! - Global constants (const declarations and define())
//! - Constants accessed via ClassName::, self::, parent::, \Namespace\
//! - Constant in various contexts (expressions, type hints, default values)

use super::*;

#[tokio::test]
async fn constant_class_basic() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Status {
    const ACT$0IVE = 1;
    //    ^^^^^^ def
    const INACTIVE = 0;
}
$s = Status::ACTIVE;
//          ^^^^^^ ref
if ($val === Status::ACTIVE) {}
//                  ^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_class_self_reference() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Config {
    const DEBU$0G = true;
    //    ^^^^^ def

    public static function isDebug(): bool {
        return self::DEBUG;
        //          ^^^^^ ref
    }

    public function check(): void {
        echo self::DEBUG ? 'debug' : 'prod';
        //         ^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_class_parent_reference() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Base {
    const VERS$0ION = '1.0';
    //    ^^^^^^^ def
}

class Extended extends Base {
    public function getVersion(): string {
        return parent::VERSION;
        //            ^^^^^^^ ref
    }
}

echo Extended::VERSION;
//            ^^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_class_cross_file() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /Config.php
<?php
class AppConfig {
    const TIMEO$0UT = 30;
    //    ^^^^^^^ def
    const MAX_RETRIES = 5;
}

//- /Client.php
<?php
$timeout = AppConfig::TIMEOUT;
//                    ^^^^^^^ ref

function retry() {
    for ($i = 0; $i < AppConfig::TIMEOUT; $i++) {
    //                             ^^^^^^^ ref
        try_request();
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_global_define_style() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
define('APP_VE$0RSION', '2.0.0');
//      ^^^^^^^^^^^ def

echo APP_VERSION;
//   ^^^^^^^^^^^ ref

if (defined('APP_VERSION')) {
    echo APP_VERSION;
    //   ^^^^^^^^^^^ ref
}
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_global_namespace_const() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
namespace App;

const MAX_S$0IZE = 1000;
//    ^^^^^^^ def

function validate($input) {
    if (strlen($input) > MAX_SIZE) {
    //                       ^^^^^^^^ ref
        throw new Exception('too large');
    }
}

class Validator {
    public function check(string $s): bool {
        return strlen($s) <= MAX_SIZE;
        //                   ^^^^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_global_cross_namespace() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /config.php
<?php
namespace Config;

const DB_H$0OST = 'localhost';
//    ^^^^^^^ def

//- /database.php
<?php
namespace App\Database;

function connect() {
    $host = \Config\DB_HOST;
    //              ^^^^^^^ ref
}

class Connection {
    private string $host = \Config\DB_HOST;
    //                             ^^^^^^^ ref
}
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_in_default_parameter() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Limits {
    const DEF$0AULT_SIZE = 100;
    //    ^^^^^^^^^^^^^ def
}

function process(int $size = Limits::DEFAULT_SIZE): void {
//                                     ^^^^^^^^^^^^ ref
    echo $size;
}
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_in_array_initializer() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class HttpCode {
    const O$0K = 200;
    //    ^^ def
    const NOT_FOUND = 404;
}

$responses = [
    HttpCode::OK => 'success',
    //       ^^ ref
    HttpCode::NOT_FOUND => 'not found',
];

function status(): int {
    return HttpCode::OK;
    //               ^^ ref
}
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_multiple_same_name_different_class() {
    // Same constant name in different classes must not interfere
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class DatabaseConfig {
    const PO$0RT = 5432;
    //    ^^^^ def
}

class CacheConfig {
    const PORT = 6379;
}

$db_port = DatabaseConfig::PORT;
//                         ^^^^ ref

$cache_port = CacheConfig::PORT;
// Should not match DatabaseConfig::PORT
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_interface_usage() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
interface HttpMethods {
    const GET$0_TIMEOUT = 30;
    //    ^^^^^^^^^^^ def
    const POST_TIMEOUT = 60;
}

class Client implements HttpMethods {
    public function request(): void {
        $timeout = self::GET_TIMEOUT;
        //              ^^^^^^^^^^^ ref
    }
}

echo HttpMethods::GET_TIMEOUT;
//               ^^^^^^^^^^^ ref
"#,
    )
    .await;
}
