//! `textDocument/declaration` — jump to abstract or interface declaration of a symbol.
//!
//! Comprehensive E2E tests covering:
//! - Interface method declarations (declaration ≠ definition)
//! - Abstract class and trait method declarations
//! - Concrete fallback cases (declaration == definition)
//! - Cross-file scenarios
//! - Edge cases (unknown symbol, empty file)

use super::*;

use expect_test::expect;

// ── Interface method declarations ──────────────────────────────────────────

#[tokio::test]
async fn interface_method_from_concrete_impl() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
interface Logger {
    public function log(string $msg): void;
}
class FileLogger implements Logger {
    public function log$0(string $msg): void {}
}
"#,
        )
        .await;
    expect![[r#"main.php:2:20-2:23"#]].assert_eq(&out);
}

#[tokio::test]
async fn interface_method_from_call_site() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
interface Logger {
    public function log(string $msg): void;
}
class FileLogger implements Logger {
    public function log(string $msg): void {}
}
$logger = new FileLogger();
$logger->log$0('hello');
"#,
        )
        .await;
    expect![[r#"main.php:2:20-2:23"#]].assert_eq(&out);
}

#[tokio::test]
async fn interface_method_on_declaration_site() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
interface Logger {
    public function log$0(string $msg): void;
}
class FileLogger implements Logger {
    public function log(string $msg): void {}
}
"#,
        )
        .await;
    expect![[r#"main.php:2:20-2:23"#]].assert_eq(&out);
}

#[tokio::test]
async fn cross_file_interface_method() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"//- /Logger.php
<?php
interface Logger {
    public function log(string $msg): void;
}

//- /FileLogger.php
<?php
class FileLogger implements Logger {
    public function log$0(string $msg): void {}
}
"#,
        )
        .await;
    expect![[r#"Logger.php:2:20-2:23"#]].assert_eq(&out);
}

#[tokio::test]
async fn two_interfaces_same_method() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
interface A {
    public function handle(): void;
}
interface B {
    public function handle(): void;
}
class Handler implements A, B {
    public function handle$0(): void {}
}
"#,
        )
        .await;
    expect![[r#"main.php:2:20-2:26"#]].assert_eq(&out);
}

#[tokio::test]
async fn interface_name_itself() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
interface Logger$0 {
    public function log(): void;
}
"#,
        )
        .await;
    expect![[r#"main.php:1:10-1:16"#]].assert_eq(&out);
}

// ── Abstract class method declarations ─────────────────────────────────────

#[tokio::test]
async fn abstract_method_from_concrete_subclass() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
abstract class Base {
    abstract public function build(): void;
}
class Impl extends Base {
    public function build$0(): void {}
}
"#,
        )
        .await;
    expect![[r#"main.php:2:29-2:34"#]].assert_eq(&out);
}

#[tokio::test]
async fn abstract_method_on_declaration_site() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
abstract class Base {
    abstract public function build$0(): void;
}
class Impl extends Base {
    public function build(): void {}
}
"#,
        )
        .await;
    expect![[r#"main.php:2:29-2:34"#]].assert_eq(&out);
}

#[tokio::test]
async fn cross_file_abstract_method() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"//- /Base.php
<?php
abstract class Base {
    abstract public function build(): void;
}

//- /Impl.php
<?php
class Impl extends Base {
    public function build$0(): void {}
}
"#,
        )
        .await;
    expect![[r#"Base.php:2:29-2:34"#]].assert_eq(&out);
}

// ── Abstract trait methods (bug case) ───────────────────────────────────────

#[tokio::test]
async fn abstract_trait_method_from_using_class() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
trait Renderable {
    abstract public function render(): string;
}
class Page {
    use Renderable;
    public function render$0(): string { return ''; }
}
"#,
        )
        .await;
    expect![[r#"main.php:2:29-2:35"#]].assert_eq(&out);
}

// ── Concrete fallback (declaration == definition) ──────────────────────────

#[tokio::test]
async fn concrete_function_falls_back_to_definition() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
function greet(): string { return 'hi'; }
greet$0();
"#,
        )
        .await;
    expect![[r#"main.php:1:9-1:14"#]].assert_eq(&out);
}

#[tokio::test]
async fn concrete_class_name() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
class Widget {
    public function show(): void {}
}
$w = new Widget$0();
"#,
        )
        .await;
    expect![[r#"main.php:1:6-1:12"#]].assert_eq(&out);
}

#[tokio::test]
async fn trait_name_falls_back() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
trait Loggable$0 {
    public function log(): void {}
}
class Page {
    use Loggable;
}
"#,
        )
        .await;
    expect![[r#"main.php:1:6-1:14"#]].assert_eq(&out);
}

#[tokio::test]
async fn enum_name_falls_back() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
enum Suit$0 {
    case Hearts;
    public function label(): string { return 'H'; }
}
"#,
        )
        .await;
    expect![[r#"main.php:1:5-1:9"#]].assert_eq(&out);
}

// ── Cross-file fallback ────────────────────────────────────────────────────

#[tokio::test]
async fn cross_file_function_fallback() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"//- /helpers.php
<?php
function validate(string $x): bool { return true; }

//- /main.php
<?php
$ok = validate$0('test');
"#,
        )
        .await;
    expect![[r#"helpers.php:1:9-1:17"#]].assert_eq(&out);
}

// ── Constants and enum members ────────────────────────────────────────────

#[tokio::test]
async fn class_constant_declaration() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
class Config {
    const DEBUG$0 = true;
}
"#,
        )
        .await;
    expect![[r#"main.php:2:10-2:15"#]].assert_eq(&out);
}

#[tokio::test]
async fn enum_case_declaration() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
enum Status {
    case Active$0;
    case Inactive;
}
"#,
        )
        .await;
    expect![[r#"main.php:2:9-2:15"#]].assert_eq(&out);
}

#[tokio::test]
async fn enum_constant_declaration() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
enum Suit {
    case Hearts;
    const MAX_VALUE$0 = 100;
}
"#,
        )
        .await;
    expect![[r#"main.php:3:10-3:19"#]].assert_eq(&out);
}

#[tokio::test]
async fn class_property_declaration() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
class User {
    public string $name$0;
    public function getName(): string { return $this->name; }
}
"#,
        )
        .await;
    expect![[r#"main.php:2:19-2:23"#]].assert_eq(&out);
}

#[tokio::test]
async fn trait_property_declaration() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
trait Timestampable {
    protected string $created$0;
}
class Post {
    use Timestampable;
}
"#,
        )
        .await;
    expect![[r#"main.php:2:22-2:29"#]].assert_eq(&out);
}

#[tokio::test]
async fn property_cursor_on_usage_finds_declaration() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
class User {
    public string $email;
}
$u = new User();
$u->email$0 = 'test@example.com';
"#,
        )
        .await;
    expect![[r#"main.php:2:19-2:24"#]].assert_eq(&out);
}

// ── Edge cases ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn unknown_word_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
class Foo {}
$x = new Undefined$0Class();
"#,
        )
        .await;
    expect![[r#"<none>"#]].assert_eq(&out);
}

#[tokio::test]
async fn variable_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
$x$0 = 42;
"#,
        )
        .await;
    expect![[r#"<none>"#]].assert_eq(&out);
}

#[tokio::test]
async fn empty_file_returns_none() {
    let mut s = TestServer::new().await;
    let out = s
        .check_declaration(
            r#"<?php
$0"#,
        )
        .await;
    expect![[r#"<none>"#]].assert_eq(&out);
}
