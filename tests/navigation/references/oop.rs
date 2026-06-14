//! OOP pattern tests: inheritance, traits, interfaces, enums, nullsafe operators.

use super::*;

#[tokio::test]
async fn references_method_via_subclass_receiver_found() {
    // Method defined on a base class must also find calls on subclass receivers.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Base {
    public function wo$0rk(): void {}
    //              ^^^^ def
}
class Child extends Base {}
$c = new Child();
$c->work();
//  ^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_trait_method() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
trait Timestampable {
    public function touc$0hAt(): void {}
    //              ^^^^^^^ def
}
class Post {
    use Timestampable;
}
$p = new Post();
$p->touchAt();
//  ^^^^^^^ ref
$p->touchAt();
//  ^^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_interface_method_finds_call_sites() {
    // Cursor on the interface method declaration: must find both the
    // implementing class's method declaration and call sites.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
interface Renderable {
    public function ren$0der(): string;
    //              ^^^^^^ def
}
class Page implements Renderable {
    public function render(): string { return ''; }
    //              ^^^^^^ def
}
$page = new Page();
echo $page->render();
//          ^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_enum_method() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
enum Status {
    case Active;
    public function lab$0el(): string { return 'active'; }
    //              ^^^^^ def
}
echo Status::Active->label();
//                   ^^^^^ ref
echo Status::Active->label();
//                   ^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_nullsafe_method_call() {
    // `$obj?->method()` must be found as a reference alongside `$obj->method()`.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Mailer {
    public function se$0nd(): void {}
    //              ^^^^ def
}
$m = new Mailer();
$m->send();
//  ^^^^ ref
$m?->send();
//   ^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_two_enums_same_method_name() {
    // Two enums that both define `label()`. Refs on `Status::label` must NOT
    // pick up `Color::label` declarations and must use the span within the
    // correct enum member (not the first occurrence of the name in the file).
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
enum Status {
    case Active;
    public function la$0bel(): string { return 'active'; }
    //              ^^^^^ def
}
enum Color {
    case Red;
    public function label(): string { return 'red'; }
}
echo Status::Active->label();
//                   ^^^^^ ref
"#,
    )
    .await;
}
