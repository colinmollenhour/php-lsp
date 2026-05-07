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

// ── Stub-index fallback (unopened file) ──────────────────────────────────────
//
// Tests for declaration resolution through FileIndex entries (unopened files).
// These use tempdir to write files to disk, start a rooted server that scans
// them, then only open the caller file so the declaration target is index-only.

/// Abstract method in unopened parent class.
#[tokio::test]
async fn declaration_from_unopened_abstract_method() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Animal.php"),
        "<?php\nabstract class Animal {\n    abstract public function speak(): string;\n}\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction call(Animal $a): string { return $a->speak(); }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "speak()", 0);
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["Animal.php:2:29-2:34"].assert_eq(&out);
}

/// Interface method in unopened interface.
#[tokio::test]
async fn declaration_from_unopened_interface_method() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Logger.php"),
        "<?php\ninterface Logger {\n    public function log(string $msg): void;\n}\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction emit(Logger $l, string $m): void { $l->log($m); }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "log($m)", 0);
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["Logger.php:2:20-2:23"].assert_eq(&out);
}

/// Interface name in unopened interface.
#[tokio::test]
async fn declaration_from_unopened_interface_name() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Logger.php"),
        "<?php\ninterface Logger {\n    public function log(): void;\n}\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction emit(Logger $l): void { $l; }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "Logger $l", 0);
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["Logger.php:1:10-1:16"].assert_eq(&out);
}

/// Free function in unopened file.
#[tokio::test]
async fn declaration_from_unopened_function() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("helpers.php"),
        "<?php\nfunction format_name(string $s): string { return $s; }\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction caller(): string { return format_name('x'); }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "format_name('x')", 0);
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["helpers.php:1:9-1:20"].assert_eq(&out);
}

/// Class name in unopened class.
#[tokio::test]
async fn declaration_from_unopened_class_name() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Widget.php"),
        "<?php\nclass Widget {\n    public function render(): void {}\n}\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction make(): Widget { return new Widget(); }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "new Widget", 0);
    let ch = ch + "new ".len() as u32;
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["Widget.php:1:6-1:12"].assert_eq(&out);
}

/// Method in unopened class.
#[tokio::test]
async fn declaration_from_unopened_method() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Service.php"),
        "<?php\nclass Service {\n    public function execute(): void {}\n}\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction run(Service $s): void { $s->execute(); }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "execute()", 0);
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["Service.php:2:20-2:27"].assert_eq(&out);
}

/// Property in unopened class (previously unimplemented for index path).
#[tokio::test]
async fn declaration_from_unopened_property() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Entity.php"),
        "<?php\nclass Entity {\n    public string $name = '';\n}\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction get(Entity $e): string { return $e->name; }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "->name", 0);
    let ch = ch + "->".len() as u32; // move cursor to start of property name
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["Entity.php:2:19-2:23"].assert_eq(&out);
}

/// Abstract method in unopened trait.
#[tokio::test]
async fn declaration_from_unopened_trait_abstract_method() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Loggable.php"),
        "<?php\ntrait Loggable {\n    abstract public function record(): void;\n}\n",
    )
    .unwrap();
    let caller_src = "<?php\nfunction output(): void { $x->record(); }\n";
    std::fs::write(tmp.path().join("caller.php"), caller_src).unwrap();

    let mut s = TestServer::with_root(tmp.path()).await;
    s.wait_for_index_ready().await;
    s.open("caller.php", caller_src).await;

    let (_, line, ch) = s.locate("caller.php", "record()", 0);
    let resp = s.declaration("caller.php", line, ch).await;
    let out = common::render_locations(&resp, &s.uri(""));
    expect!["Loggable.php:2:29-2:35"].assert_eq(&out);
}
