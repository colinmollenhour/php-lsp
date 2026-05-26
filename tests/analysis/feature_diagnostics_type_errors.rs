//! Diagnostic coverage matrix using the caret annotation DSL.
//! Each test names the expectation inline with `// ^^^ severity: message`.

use super::*;

use expect_test::expect;
use serde_json::json;

#[tokio::test]
async fn argument_count_too_few_detected() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
function needs_two(string $a, string $b): void {}
function wrap(): void {
    needs_two('x');
//  ^^^^^^^^^^^^^^ error: needs_two
}
"#,
        )
        .await;
}

#[tokio::test]
async fn argument_count_too_many_detected() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
function takes_one(string $s): void {}
function wrap(): void {
    takes_one('a', 'b', 'c');
//                 ^^^ error: takes_one
}
"#,
        )
        .await;
}

/// Regression: `new ShortName()` where `use A\B\ShortName;` must not emit
/// UndefinedClass when the class is on disk (PSR-4 lazy-loading path).
/// Distinct from `psr4_imported_class_not_flagged_before_workspace_scan` which
/// only tested parameter type hints — this exercises the `new` expression path.
#[tokio::test]
async fn argument_type_mismatch_detected() {
    let mut server = TestServer::new().await;
    server
        .check_diagnostics(
            r#"<?php
function takes_string(string $s): void {}
function wrap(): void {
    takes_string(42);
//               ^^ error: takes_string
}
"#,
        )
        .await;
}

/// PSR-4-resolvable classes must not produce UndefinedClass diagnostics even
/// when the background workspace scan has not yet reached the dependency file.
/// The fix (PSR-4 lazy-loading inside `get_semantic_issues_salsa`) reads the
/// dependency from disk before running semantic analysis, making the result
/// deterministic regardless of scan timing.
#[tokio::test]
async fn duplicate_named_arg_in_constructor() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
class Point {
    public function __construct(public int $x, public int $y) {}
}
new Point(x: 0, y: 1, x: 2);
//                    ^^^^ error: Point::__construct() has no parameter named $x
"#,
    )
    .await;
}

#[tokio::test]
async fn duplicate_named_arg_in_function_call() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function foo(int $a, int $b): void {}
foo(a: 1, b: 2, a: 3);
//              ^^^^ error: foo() has no parameter named $a
"#,
    )
    .await;
}

#[tokio::test]
async fn duplicate_named_arg_in_method_call() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
class C {
    public function run(int $x, int $y): void {}
}
(new C())->run(x: 1, y: 2, x: 99);
//                         ^^^^^ error: run() has no parameter named $x
"#,
    )
    .await;
}

#[tokio::test]
async fn positional_after_named_arg() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    s.check_diagnostics(
        r#"<?php
function bar(int $a, int $b): void {}
bar(a: 1, 2);
//        ^ error: cannot use positional argument after named argument
//        ^ error: bar() has no parameter named $#2
"#,
    )
    .await;
}

#[tokio::test]
async fn valid_named_args_produce_no_diagnostic() {
    let mut s = TestServer::new().await;
    s.check_diagnostics(
        r#"<?php
function greet(string $name, int $times): void {}
greet(name: 'Alice', times: 3);
"#,
    )
    .await;
}

// ── circular inheritance diagnostics ─────────────────────────────────────────

#[tokio::test]
async fn workspace_diagnostic_named_arguments() {
    let mut server = TestServer::new().await;
    server
        .open(
            "ws_named_args.php",
            "<?php\nfunction foo(int $a, int $b): void {}\nfoo(a: 1, b: 2, a: 3);\n",
        )
        .await;

    let resp = server.workspace_diagnostic().await;
    let out = render_workspace_diagnostic(&resp, &server.uri(""));

    expect![[r#"
        ws_named_args.php
          2:16 foo() has no parameter named $a [InvalidNamedArgument] (error)"#]]
    .assert_eq(&out);
}
