//! Comprehensive hover coverage.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn hover_abstract_class_shows_keyword() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
abstract class Bas$0eHandler {}
"#,
        )
        .await;
    expect![[r#"
        ```php
        abstract class BaseHandler
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_abstract_method_shows_modifiers() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
abstract class Base {
    abstract protected function pro$0cess(string $input): string;
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        protected abstract function process(string $input): string
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_arrow_function_keyword() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(r#"<?php $f = f$0n(int $a): string => 'hello';"#)
        .await;
    expect![[r#"
        ```php
        fn(int $a): string
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_attribute_class_name() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class MyAttribute {}

#[MyAttri$0bute]
class Foo {}
"#,
        )
        .await;
    expect![[r#"
        ```php
        class MyAttribute
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_attribute_via_use_alias() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Route {}
use Route as HttpRoute;

#[HttpRou$0te]
class Api {}
"#,
        )
        .await;
    // Resolves alias → Route
    expect![[r#"
        ```php
        class Route
        ```"#]]
    .assert_eq(&v);
}

// ── 2.2 Named argument hover ──────────────────────────────────────────────────

#[tokio::test]
async fn hover_attribute_with_args() {
    // Cursor on attribute class name when the attribute has constructor arguments.
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Route {
    public function __construct(string $path) {}
}

#[Rou$0te('/api')]
class Controller {}
"#,
        )
        .await;
    expect![[r#"
        ```php
        class Route
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_attribute_with_docblock() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
/** Marks a class as a service container. */
class Service {}

#[Servi$0ce]
class Mailer {}
"#,
        )
        .await;
    expect![[r#"
        ```php
        class Service
        ```

        ---

        Marks a class as a service container."#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_backed_int_enum_shows_backing_type() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
enum Priorit$0y: int { case Low = 1; case High = 2; }
"#,
        )
        .await;
    expect![[r#"
        ```php
        enum Priority: int
        ```"#]]
    .assert_eq(&v);
}

// ── Class modifiers ───────────────────────────────────────────────────────────

#[tokio::test]
async fn hover_class_const_in_static_access() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Config {
    const DEBUG = true;
}
if (Config::DEB$0UG) { }
"#,
        )
        .await;
    expect![[r#"
        ```php
        const bool DEBUG = true
        ```"#]]
    .assert_eq(&v);
}

/// Hovering on backed enum case in a match arm should show the value.
#[tokio::test]
async fn hover_closure_as_argument() {
    // Cursor on `function` keyword passed as a callback argument.
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
function apply(callable $fn): void {}
apply(fun$0ction(int $n): int { return $n * 2; });
"#,
        )
        .await;
    expect![[r#"
        ```php
        function(int $n): int
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_closure_inside_if_body() {
    // Closure nested inside an if body — the walker must recurse into if branches.
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
if (true) {
    $fn = fun$0ction(int $x): string { return (string) $x; };
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        function(int $x): string
        ```"#]]
    .assert_eq(&v);
}

/// Hovering on `new Foo()` (the constructor call) must resolve to the class definition.
#[tokio::test]
async fn hover_closure_keyword() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(r#"<?php $fn = fun$0ction(int $x, string $y): bool { return true; };"#)
        .await;
    expect![[r#"
        ```php
        function(int $x, string $y): bool
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_closure_no_params_no_return() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(r#"<?php $fn = fun$0ction() { return 1; };"#)
        .await;
    expect![[r#"
        ```php
        function()
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_deprecated_function_shows_banner() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
/** @deprecated Use newGreet() instead */
function ol$0dGreet(): void {}
"#,
        )
        .await;
    expect![[r#"
        ```php
        function oldGreet(): void
        ```

        ---

        > **Deprecated**: Use newGreet() instead"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_docblock_annotated_function() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
/**
 * Greets the user.
 * @param string $name the name
 * @return string
 */
function gr$0eet(string $name): string { return $name; }
"#,
        )
        .await;
    expect![[r#"
        ```php
        function greet(string $name): string
        ```

        ---

        Greets the user.

        **@return** `string`
        **@param** `string` `$name` — the name"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_enum_case_in_match_arm() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
enum Status { case Active; case Inactive; }
$status = Status::Active;
match ($status) {
    Status::Act$0ive => echo 'active',
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        case Status::Active
        ```"#]]
    .assert_eq(&v);
}

/// Hovering on class constant in static access should show the constant.
#[tokio::test]
async fn hover_final_class_shows_keyword() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
final class Concret$0eService {}
"#,
        )
        .await;
    expect![[r#"
        ```php
        final class ConcreteService
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_final_method_shows_modifiers() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Locked {
    final public function sea$0l(): void {}
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        public final function seal(): void
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_first_class_callable_builtin() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s.check_hover(r#"<?php $fn = str$0len(...);"#).await;
    expect![[r#"
        ```php
        function strlen()
        ```

        [php.net documentation](https://www.php.net/function.strlen)"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_first_class_callable_user_function() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(r#"<?php function double(int $n): int {} $fn = dou$0ble(...);"#)
        .await;
    expect![[r#"
        ```php
        function double(int $n): int
        ```"#]]
    .assert_eq(&v);
}

// ── 1.1 @inheritDoc resolution ───────────────────────────────────────────────

#[tokio::test]
async fn hover_keyword_false() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s.check_hover(r#"<?php $x = fal$0se;"#).await;
    expect![["`false` — boolean false"]].assert_eq(&v);
}

#[tokio::test]
async fn hover_keyword_match() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s.check_hover(r#"<?php $x = mat$0ch($y) {};"#).await;
    expect![["`match` — evaluates an expression against a set of arms (PHP 8.0)"]].assert_eq(&v);
}

#[tokio::test]
async fn hover_keyword_never() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(r#"<?php function fail(): nev$0er { throw new \Exception(); }"#)
        .await;
    expect![["`never` — return type indicating the function always throws or exits (PHP 8.1)"]]
        .assert_eq(&v);
}

#[tokio::test]
async fn hover_keyword_null() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s.check_hover(r#"<?php $x = nu$0ll;"#).await;
    expect![["`null` — the null value; a variable has no value"]].assert_eq(&v);
}

#[tokio::test]
async fn hover_keyword_readonly() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(r#"<?php class Foo { readon$0ly string $x; }"#)
        .await;
    expect![["`readonly` — property or class that can only be initialised once (PHP 8.1)"]]
        .assert_eq(&v);
}

#[tokio::test]
async fn hover_keyword_true() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s.check_hover(r#"<?php $x = tr$0ue;"#).await;
    expect![["`true` — boolean true"]].assert_eq(&v);
}

#[tokio::test]
async fn hover_named_arg_builtin_function() {
    // PHP 8.0 named arg on a user-defined function matching a known param name.
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
function greet(string $name, int $count = 1): string { return $name; }
greet(coun$0t: 3);
"#,
        )
        .await;
    expect![[r#"
        ```php
        (parameter) int $count = 1
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_named_arg_method_call() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Mailer {
    public function send(string $to, string $subject): bool { return true; }
}
$m = new Mailer();
$m->send(subje$0ct: 'Hello', to: 'a@b.com');
"#,
        )
        .await;
    expect![[r#"
        ```php
        (parameter) string $subject
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_named_arg_nested_call() {
    // Named arg inside a nested function call — cursor on inner call's arg.
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
function outer(string $a): string { return $a; }
function inner(int $x): int { return $x; }
outer(a: inner(x$0: 1));
"#,
        )
        .await;
    expect![[r#"
        ```php
        (parameter) int $x
        ```"#]]
    .assert_eq(&v);
}

// ── 2.3 Closure / arrow function hover ───────────────────────────────────────

#[tokio::test]
async fn hover_named_arg_static_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class DB {
    public static function query(string $sql, int $limit = 100): array { return []; }
}
DB::query(lim$0it: 10);
"#,
        )
        .await;
    expect![[r#"
        ```php
        (parameter) int $limit = 100
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_named_arg_with_docblock() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
/**
 * @param string $name The user's name.
 * @param int $age  The user's age.
 */
function register(string $name, int $age): void {}
register(na$0me: 'Alice', age: 30);
"#,
        )
        .await;
    expect![[r#"
        ```php
        (parameter) string $name
        ```

        ---

        The user's name."#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_named_function_keyword_not_intercepted() {
    // Hovering the `function` keyword in a named declaration (not a closure)
    // should not trigger the closure hover — returns nothing for the keyword itself.
    // Hover on the function *name* (not keyword) to get the signature.
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s.check_hover(r#"<?php fun$0ction greet(): void {}"#).await;
    expect!["<no hover>"].assert_eq(&v);
}

#[tokio::test]
async fn hover_on_constructor_call() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Service {
    public function __construct(private string $dsn) {}
}
$svc = new Serv$0ice('db://localhost');
"#,
        )
        .await;
    expect![[r#"
        ```php
        class Service
        ```"#]]
    .assert_eq(&v);
}

/// Hovering on a property access with union type should show the union.
#[tokio::test]
async fn hover_readonly_class_shows_keyword() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
readonly class Poi$0nt { public function __construct(public float $x, public float $y) {} }
"#,
        )
        .await;
    expect![[r#"
        ```php
        readonly class Point
        ```"#]]
    .assert_eq(&v);
}

// ── Use-alias resolution ──────────────────────────────────────────────────────

/// Hovering on `Bar` where `use Foo as Bar` is in scope must show the `Foo`
/// class declaration.
#[tokio::test]
async fn hover_readonly_property_shows_modifier() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Point {
    public readonly float $x;
}
$p = new Point();
echo $p->$0x;
"#,
        )
        .await;
    expect![[r#"
        ```php
        (property) public readonly Point::$x: float
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_real_docblock_not_overwritten_by_inheritdoc() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Base {
    /** Parent description. */
    public function run(): void {}
}
class Child extends Base {
    /** Child's own description. */
    public function run(): void {}
}
$c = new Child();
$c->ru$0n();
"#,
        )
        .await;
    expect![[r#"
        ```php
        Child::run(): void
        ```

        ---

        Child's own description."#]]
    .assert_eq(&v);
}

// ── 1.2 Keyword hover ────────────────────────────────────────────────────────

#[tokio::test]
async fn hover_second_method_call_on_same_line_picks_correct_receiver() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class A { public function handle(string $x): bool {} }
class B { public function handle(int $n): void {} }
$a = new A(); $b = new B();
$a->handle('x'); $b->hand$0le(1);
"#,
        )
        .await;
    // Must show B::handle (int $n), not A::handle (string $x).
    expect![[r#"
        ```php
        B::handle(int $n): void
        ```"#]]
    .assert_eq(&v);
}

// ── Trait inheritance correctness ─────────────────────────────────────────────

/// Two classes each define a method named `ping`; only `Server` uses the
/// `Pingable` trait that actually has the implementation.  Hovering on
/// `$server->ping()` must show `Server::ping`, not `Client::ping`.
#[tokio::test]
async fn hover_self_static_call_resolves_enclosing_class() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Builder {
    public static function create(): static { return new static(); }
    public function run(): void { self::crea$0te(); }
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        Builder::create(): static
        ```"#]]
    .assert_eq(&v);
}

// ── Correct receiver on multi-call line ───────────────────────────────────────

/// Two different `->process()` calls on the same line — cursor on the second
/// one must pick the second receiver, not the first.
#[tokio::test]
async fn hover_static_call_resolves_correct_class() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Worker { public static function run(int $jobs): void {} }
class Scheduler { public static function run(string $cron): bool { return true; } }
Worker::ru$0n(4);
"#,
        )
        .await;
    expect![[r#"
        ```php
        Worker::run(int $jobs): void
        ```"#]]
    .assert_eq(&v);
}

/// `self::method()` at a call site resolves to the enclosing class.
#[tokio::test]
async fn hover_static_keyword_in_static_call_not_intercepted() {
    // `static::method()` — hovering `static` should NOT return the keyword doc,
    // it should fall through to the self/static class resolution.
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Base {
    public static function create(): static {}
    public static function build(): static {
        return stat$0ic::create();
    }
}
"#,
        )
        .await;
    // Should resolve to something about Base (static call), not the keyword doc.
    assert!(
        !v.contains("return type") && !v.contains("late static"),
        "static:: should not trigger keyword hover, got: {v}"
    );
}

// ── 2.4 PHP attribute hover ───────────────────────────────────────────────────
