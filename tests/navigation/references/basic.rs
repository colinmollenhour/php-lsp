//! Basic single-file reference tests: functions, methods, classes, unused symbols.

use super::*;

#[tokio::test]
async fn references_function_same_file() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
function gr$0eet(): void {}
//       ^^^^^ def
  greet();
//^^^^^ ref
  greet();
//^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_same_file() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Greeter {
    public function he$0llo(): string { return 'hi'; }
    //              ^^^^^ def
}
$g = new Greeter();
$g->hello();
//  ^^^^^ ref
$g->hello();
//  ^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_static_method() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Reg {
    public static function ge$0t(): void {}
    //                     ^^^ def
}
Reg::get();
//   ^^^ ref
Reg::get();
//   ^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_no_usages_for_unused_function() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
function un$0used(): void {}
//       ^^^^^^ def
"#,
    )
    .await;
}

#[tokio::test]
async fn references_class_used_in_new() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Wi$0dget {}
//    ^^^^^^ def
$a = new Widget();
//       ^^^^^^ ref
$b = new Widget();
//       ^^^^^^ ref
"#,
    )
    .await;
}
