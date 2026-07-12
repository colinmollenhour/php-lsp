//! References import collection tests (protocol-wired).
//! Cursor placed on a `use` import segment currently resolves to nothing —
//! these tests pin that known limitation so a future change to it is visible.

use super::*;

#[tokio::test]
async fn references_excludes_class_import_alias() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    // Cursor on an import alias — no definitions or usages resolve through it.
    s.check_references_annotated(
        r#"<?php
use App\Ba$0r;
use function App\helper;
use const App\LIMIT;

class Bar {}
"#,
    )
    .await;
}

#[tokio::test]
async fn references_excludes_function_imports() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    // Cursor on a `use function` import segment — no cross-file refs resolve.
    s.check_references_annotated(
        r#"<?php
use function App\he$0lper;

function helper() {}
"#,
    )
    .await;
}

#[tokio::test]
async fn references_excludes_const_imports() {
    let mut s = TestServer::new().await;
    // Cursor on a `use const` import segment — no refs resolve.
    s.check_references_annotated(
        r#"<?php
use const App\LI$0MIT;

define('LIMIT', 100);
"#,
    )
    .await;
}

#[tokio::test]
async fn references_excludes_aliased_class_import() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    // Cursor on the original class name in an aliased import — no refs resolve.
    s.check_references_annotated(
        r#"<?php
use App\Services\OldSe$0rvice as Service;

class OldService {}
"#,
    )
    .await;
}

#[tokio::test]
async fn references_distinguishes_class_constant_access() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Status {
    const ACTIVE = 1;
    //    ^^^^^^ def
}
$x = Status::AC$0TIVE;
//           ^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_handles_class_name_duplicate_with_member() {
    let mut s = TestServer::new().await;
    // Should find multiple references to Status (class + const member)
    s.check_references_annotated(
        r#"<?php
class Statu$0s {
//    ^^^^^^ def
    const Status = 1;
}
$x = Status::Status;
//   ^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_excludes_namespaced_import() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    // Cursor on an import inside a namespace — no cross-file refs resolve.
    s.check_references_annotated(
        r#"<?php
namespace App;
use Services\Log$0ger;

class Logger {}
"#,
    )
    .await;
}

#[tokio::test]
async fn references_excludes_builtin_function_import() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    // Cursor on a built-in function import segment — no refs resolve.
    s.check_references_annotated(
        r#"<?php
use function App\str$0len;

strlen('test');
"#,
    )
    .await;
}
