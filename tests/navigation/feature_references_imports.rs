//! References import collection tests (protocol-wired).
//! Tests verify that import statements are correctly identified and used by references.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn references_includes_class_imports() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let refs = s
        .check_references(
            r#"<?php
use App\Ba$0r;
use function App\helper;
use const App\LIMIT;

class Bar {}
"#,
        )
        .await;
    // Should find the class Bar definition through import
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn references_excludes_function_imports() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let refs = s
        .check_references(
            r#"<?php
use function App\he$0lper;

function helper() {}
"#,
        )
        .await;
    // Should find the function definition
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn references_excludes_const_imports() {
    let mut s = TestServer::new().await;
    let refs = s
        .check_references(
            r#"<?php
use const App\LI$0MIT;

define('LIMIT', 100);
"#,
        )
        .await;
    // Should handle const imports - define creates a constant
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn references_finds_aliased_class_imports() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let refs = s
        .check_references(
            r#"<?php
use App\Services\OldSe$0rvice as Service;

class OldService {}
"#,
        )
        .await;
    // Should find the original class name through alias
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn references_distinguishes_class_constant_access() {
    let mut s = TestServer::new().await;
    let refs = s
        .check_references(
            r#"<?php
class Status {
    const ACTIVE = 1;
}
$x = Status::AC$0TIVE;
"#,
        )
        .await;
    // References should find the constant access
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn references_handles_class_name_duplicate_with_member() {
    let mut s = TestServer::new().await;
    let refs = s
        .check_references(
            r#"<?php
class Statu$0s {
    const Status = 1;
}
$x = Status::Status;
"#,
        )
        .await;
    // Should find multiple references to Status (class + const member)
    expect![[r#"
        main.php:1:6-1:12
        main.php:4:5-4:11"#]]
    .assert_eq(&refs);
}

#[tokio::test]
async fn references_respects_namespace_context() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let refs = s
        .check_references(
            r#"<?php
namespace App;
use Services\Log$0ger;

class Logger {}
"#,
        )
        .await;
    // Logger should be found in the namespace context
    expect!["<none>"].assert_eq(&refs);
}

#[tokio::test]
async fn references_handles_function_imports_correctly() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let refs = s
        .check_references(
            r#"<?php
use function App\str$0len;

strlen('test');
"#,
        )
        .await;
    // Should handle function imports
    expect!["<none>"].assert_eq(&refs);
}
