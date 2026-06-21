//! Property reference tests: declared properties, promoted properties, access sites, nullsafe.

use super::*;

use expect_test::expect;

/// Two classes in the same file share the same property name. References for one
/// must NOT include the declaration of the other — previously `str_offset` found
/// the first occurrence in the file, mapping both declarations to class A's span.
#[tokio::test]
async fn references_property_same_name_two_classes_no_cross_bleed() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class A {
    public int $va$0lue = 0;
    //          ^^^^^ def
    public function getA(): int { return $this->value; }
    //                                          ^^^^^ ref
}
class B {
    public int $value = 0;
    public function getB(): int { return $this->value; }
}
"#,
    )
    .await;
}

/// Same scenario from the other class — cursor on B's property must only show B's
/// declaration and B's access site, not A's declaration or A's access.
#[tokio::test]
async fn references_property_same_name_two_classes_b_side() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class A {
    public int $value = 0;
    public function getA(): int { return $this->value; }
}
class B {
    public int $va$0lue = 0;
    //          ^^^^^ def
    public function getB(): int { return $this->value; }
    //                                          ^^^^^ ref
}
"#,
    )
    .await;
}

#[tokio::test]
async fn references_promoted_property_this_access() {
    // `$this->prop` inside a method must be returned alongside external `->prop`
    // accesses and the constructor param declaration when cursor is on a promoted
    // constructor property.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Person {
    public function __construct(public readonly string $na$0me) {}
    //                                                  ^^^^ ref
    public function greet(): string {
        return $this->name;
        //            ^^^^ ref
    }
}
$p = new Person('Alice');
echo $p->name;
//       ^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_promoted_property_finds_nullsafe_access() {
    // `$obj?->prop` must be returned alongside `$obj->prop` and the constructor
    // param declaration when searching refs on a promoted constructor property.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Config {
    public function __construct(public readonly string $ke$0y) {}
    //                                                  ^^^ ref
}
$c = new Config('x');
echo $c->key;
//       ^^^ ref
echo $c?->key;
//        ^^^ ref
"#,
    )
    .await;
}

/// Searching references from a property *access* site (`$this->prop`) must
/// behave the same as searching from the constructor param declaration —
/// finding all property accesses, not method calls.
#[tokio::test]
async fn references_promoted_property_from_access_site() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Cart {
    public function __construct(private object $item) {}
    //                                          ^^^^ ref
    public function total(): void { $this->it$0em; }
    //                                     ^^^^ ref
    public function describe(): void { $this->item; }
    //                                        ^^^^ ref
}
"#,
    )
    .await;
}

/// Cursor on a property *access* site — refs must find all `->propName`
/// accesses and the declaration, but not a method with the same name.
#[tokio::test]
async fn references_property_access_site() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Cart {
    public int $total = 0;
    //          ^^^^^ def
    public function total(): int { return $this->total; }
    //                                           ^^^^^ ref
}
$c = new Cart();
$c->to$0tal;
//  ^^^^^ ref
$c->total += 5;
//  ^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_property_declaration() {
    // Cursor on a property *declaration* — refs must include the declaration
    // itself plus every access site, but not a same-named method call.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Order {
    public string $sta$0tus = '';
    //             ^^^^^^ def
    public function status(): string { return $this->status; }
    //                                               ^^^^^^ ref
}
$o = new Order();
$o->status;
//  ^^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_on_promoted_property_cross_file() {
    let dir = tempfile::tempdir().unwrap();
    let entity_src = "<?php\nclass User {\n    public function __construct(public readonly string $email) {}\n}\n";
    std::fs::write(dir.path().join("entity.php"), entity_src).unwrap();
    std::fs::write(
        dir.path().join("service.php"),
        "<?php\nfunction notify(User $u): void {\n    echo $u->email;\n    echo $u?->email;\n}\n",
    )
    .unwrap();

    let mut server = TestServer::with_root(dir.path()).await;
    server.wait_for_index_ready().await;

    server.open("entity.php", entity_src).await;

    let resp = server.references("entity.php", 2, 56, false).await;
    assert!(resp["error"].is_null(), "references error: {resp:?}");
    expect![[r#"
        service.php:2:13-2:18
        service.php:3:14-3:19"#]]
    .assert_eq(&render_locations(&resp, &server.uri("")));
}
