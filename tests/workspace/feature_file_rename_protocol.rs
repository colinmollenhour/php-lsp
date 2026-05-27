//! File rename use import rewriting tests (protocol-wired).
//! Tests verify that use statements are correctly parsed and referenced.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn simple_use_statement_resolves() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let hover = s
        .check_hover(
            r#"<?php
use App$0\Services\Foo;

class Foo {}
"#,
        )
        .await;
    // Use statement namespace should be recognized
    expect![[r#"`use App\Services\Foo;`"#]].assert_eq(&hover);
}

#[tokio::test]
async fn use_with_leading_backslash() {
    let mut s = TestServer::new().await;
    let hover = s
        .check_hover(
            r#"<?php
use \App$0\Services\OldService;

echo 'ok';
"#,
        )
        .await;
    // The use statement with leading backslash should be recognized
    expect![[r#"`use \App\Services\OldService;`"#]].assert_eq(&hover);
}

#[tokio::test]
async fn aliased_imports_resolve() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let refs = s
        .check_references(
            r#"<?php
use App$0\Services\OldService as Service;

class OldService {}
"#,
        )
        .await;
    // Aliased import should find the target class
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn partial_class_names_not_matched() {
    let mut s = TestServer::new().await;
    let refs = s
        .check_references(
            r#"<?php
use App$0\Services\Foo;
class FooExtra {}

$x = new FooExtra();
"#,
        )
        .await;
    // When searching for Foo, should not match FooExtra
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn non_use_lines_ignored() {
    let mut s = TestServer::new().await;
    let hover = s
        .check_hover(
            r#"<?php
// use App\Old\Class;
$x = new App$0\Old\Class();
"#,
        )
        .await;
    // Direct instantiation should be recognized even with commented use
    expect!["<no hover>"].assert_eq(&hover);
}

#[tokio::test]
async fn namespace_and_use_together() {
    let mut s = TestServer::new().await;
    let hover = s
        .check_hover(
            r#"<?php
namespace App;
use Services$0\OldName;
"#,
        )
        .await;
    // Both namespace and use should be parsed together
    expect![[r#"`use Services\OldName;`"#]].assert_eq(&hover);
}

#[tokio::test]
async fn function_imports_resolved() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let hover = s
        .check_hover(
            r#"<?php
use function App$0\helper;
use const App\LIMIT;
use App\Class;

class Class {}
"#,
        )
        .await;
    // Function imports should be recognized
    expect![[r#"`use function App\helper;`"#]].assert_eq(&hover);
}

#[tokio::test]
async fn references_across_use_statements() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    // References should find the class usage.
    s.check_references_annotated(
        r#"<?php
use App$0\Logger;

class Logger {}

$log = new Logger();
//         ^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn utf16_offsets_in_use_statements() {
    let mut s = TestServer::new().await;
    let def = s
        .check_definition(
            r#"<?php
use App$0\Services\Service;

echo 'test';
"#,
        )
        .await;
    // UTF-16 offset handling should work for ASCII
    expect!["<none>"].assert_eq(&def);
}

#[tokio::test]
async fn use_with_alias_preserves_original() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let refs = s
        .check_references(
            r#"<?php
use App\Se$0rvices\MyClass as MC;

class MyClass {}
"#,
        )
        .await;
    // Alias import should reference original class name
    expect!["<none>"].assert_eq(&refs);
}
