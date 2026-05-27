//! Advanced type declaration references.
//!
//! Tests complex type scenarios:
//! - Union types (Type1|Type2|Type3)
//! - Intersection types (Type1&Type2)
//! - Nullable types (?Type, Type|null)
//! - Mixed pseudo-type
//! - Complex combinations of the above

use super::*;

#[tokio::test]
async fn type_union_two_classes() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Request$0er {
//    ^^^^^^^^^ def
}

class Response {}

function handle(Requester|Response $item): void {
//              ^^^^^^^^^ ref
    if ($item instanceof Requester) {
    //                   ^^^^^^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn type_union_parameter_and_return() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Succe$0ss {
//    ^^^^^^^ def
}

function f1(): Success|Error {}
//             ^^^^^^^ ref
function f2(Success|Error $x) {}
//          ^^^^^^^ ref
function f3(): Success {}
//             ^^^^^^^ ref
function f4(Success $x): void {}
//          ^^^^^^^ ref
function f5(): Success {}
//             ^^^^^^^ ref
function f6(): void { new Success(); }
//                        ^^^^^^^ ref
function f7($x): void { if ($x instanceof Success) {} }
//                                        ^^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn type_intersection_two_interfaces() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
interface Printab$0le {
//        ^^^^^^^^^ def
}

interface Serializable {}

function export(Printable&Serializable $obj): string {
//              ^^^^^^^^^ ref
    return $obj->serialize();
}
"#,
    )
    .await;
}

#[tokio::test]
async fn type_nullable_in_parameter() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Use$0r {
//    ^^^^ def
}

function getUser(int $id): ?User {
//                          ^^^^ ref
    return null;
}

function setUser(?User $user): void {
//                ^^^^ ref
    if ($user !== null) {}
}
"#,
    )
    .await;
}

#[tokio::test]
async fn type_union_with_null() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Tok$0en {
//    ^^^^^ def
}

function authenticate(): Token|null {
//                       ^^^^^ ref
    if (random_int(0, 1)) {
        return new Token();
        //         ^^^^^ ref
    }
    return null;
}

function validate(Token|null $token): bool {
//                ^^^^^ ref
    return $token !== null;
}
"#,
    )
    .await;
}

#[tokio::test]
async fn type_union_property() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Standa$0rd {
//    ^^^^^^^^ def
}

class Premium {}

class Account {
    public Standard|Premium $subscription;
    //     ^^^^^^^^ ref
}
"#,
    )
    .await;
}

#[tokio::test]
async fn type_complex_union_three_classes() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Rea$0d {
//    ^^^^ def
}

class Write {}
class Delete {}

function f(Read|Write|Delete $x): void {}
//         ^^^^ ref
function g(): Read {}
//            ^^^^ ref
function h($x) { if ($x instanceof Read) {} }
//                                 ^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn type_nullable_union() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Data$0base {
//    ^^^^^^^^ def
}

function f(): ?Database {}
//             ^^^^^^^^ ref
function g(): Database|null {}
//            ^^^^^^^^ ref
function h(): void { new Database(); }
//                       ^^^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn type_intersection_with_class_and_interface() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Bas$0eModel {
//    ^^^^^^^^^ def
}

interface Timestamped {}

function save(BaseModel&Timestamped $obj): void {
//            ^^^^^^^^^ ref
    $obj->save();
}
"#,
    )
    .await;
}

#[tokio::test]
async fn type_mixed_pseudo_type() {
    // mixed is a pseudo-type that doesn't need references
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Process$0or {
//    ^^^^^^^^^ def
}

function handle(mixed $data): void {
    if ($data instanceof Processor) {
        //               ^^^^^^^^^ ref
        $data->process();
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn type_in_match_expression() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Credi$0t {
//    ^^^^^^ def
}

function f(Credit|null $x): void {}
//         ^^^^^^ ref
function g(): Credit {}
//            ^^^^^^ ref
function h(): void { new Credit(); }
//                       ^^^^^^ ref
"#,
    )
    .await;
}
