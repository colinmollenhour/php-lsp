//! Advanced type declaration references.
//!
//! Tests complex type scenarios:
//! - Union types (Type1|Type2|Type3)
//! - Intersection types (Type1&Type2)
//! - Nullable types (?Type, Type|null)
//! - Mixed pseudo-type
//! - Complex combinations of the above

use super::*;

#[ignore]
#[tokio::test]
async fn type_union_two_classes() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Request$0er {
//    ^^^^^^^^^ def
}

class Response {
//     ^^^^^^^^ def
}

function handle(Requester|Response $item): void {
//               ^^^^^^^^^ ref
//                         ^^^^^^^^ ref
    if ($item instanceof Requester) {}
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_union_parameter_and_return() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Succe$0ss {
//    ^^^^^^^ def
}

class Error {
//     ^^^^^ def
}

function execute(): Success|Error {
//                  ^^^^^^^ ref
//                          ^^^^^ ref
    return new Success();
}

function process(Success|Error $result): void {
//                ^^^^^^^ ref
//                        ^^^^^ ref
    if ($result instanceof Success) {}
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_intersection_two_interfaces() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
interface Printab$0le {
//         ^^^^^^^^^ def
}

interface Serializable {
//         ^^^^^^^^^^^^ def
}

function export(Printable&Serializable $obj): string {
//               ^^^^^^^^^ ref
//                         ^^^^^^^^^^^^ ref
    return $obj->serialize();
}
"#,
    )
    .await;
}

#[ignore]
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
//               ^^^^ ref
    if ($user !== null) {}
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_union_with_null() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Token {
//     ^^^^^ def
}

function authenticate(): Token|null {
//                       ^^^^^ ref
    if (random_int(0, 1)) {
        return new Token();
    }
    return null;
}

function validate(Token|null $token): bool {
//                 ^^^^^ ref
    return $token !== null;
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_union_property() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Standa$0rd {
//    ^^^^^^^^ def
}

class Premium {
//     ^^^^^^^ def
}

class Account {
    public Standard|Premium $subscription;
    //     ^^^^^^^^ ref
    //             ^^^^^^^ ref
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_complex_union_three_classes() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Rea$0d {
//    ^^^^ def
}

class Write {
//     ^^^^^ def
}

class Delete {
//     ^^^^^^ def
}

function perform(Read|Write|Delete $action): void {
//                ^^^^ ref
//                     ^^^^^ ref
//                           ^^^^^^ ref
    match ($action) {
        Read => read_data(),
        Write => write_data(),
        Delete => delete_data(),
    }
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_nullable_union() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Data$0base {
//    ^^^^^^^^ def
}

class Cache {
//     ^^^^^ def
}

function getStore(): ?Database|Cache {
//                  ^^^^^^^^ ref
//                           ^^^^^ ref
    if (random_int(0, 1)) {
        return new Database();
    }
    return new Cache();
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_intersection_with_class_and_interface() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Bas$0eModel {
//    ^^^^^^^^^ def
}

interface Timestamped {
//         ^^^^^^^^^^^ def
}

function save(BaseModel&Timestamped $obj): void {
//             ^^^^^^^^^ ref
//                       ^^^^^^^^^^^ ref
    $obj->save();
}
"#,
    )
    .await;
}

#[ignore]
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
    //                   ^^^^^^^^^ ref
        $data->process();
    }
}
"#,
    )
    .await;
}

#[ignore]
#[tokio::test]
async fn type_in_match_expression() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Credi$0t {
//    ^^^^^^ def
}

class Debit {
//     ^^^^^ def
}

function classify(Credit|Debit $txn): string {
//                 ^^^^^^ ref
//                        ^^^^^ ref
    return match($txn::class) {
        Credit::class => 'income',
        Debit::class => 'expense',
    };
}
"#,
    )
    .await;
}
