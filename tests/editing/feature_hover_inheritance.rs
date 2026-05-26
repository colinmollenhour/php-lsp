//! Comprehensive hover coverage.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn hover_child_receiver_resolves_parent_method_correctly() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Animal { public function speak(): string { return '...'; } }
class Dog extends Animal {}
class Parrot { public function speak(): string { return 'hello'; } }
$d = new Dog();
$d->spea$0k();
"#,
        )
        .await;
    // Must show Dog::speak (inherited from Animal), NOT Parrot::speak.
    expect![[r#"
        ```php
        Dog::speak(): string
        ```"#]]
    .assert_eq(&v);
}

// ── Declaration-site modifiers ────────────────────────────────────────────────

#[tokio::test]
async fn hover_inheritdoc_at_tag_form() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Base {
    /** Fetches the record. */
    public function fetch(): void {}
}
class Child extends Base {
    /** @inheritDoc */
    public function fetch(): void {}
}
$c = new Child();
$c->fet$0ch();
"#,
        )
        .await;
    expect![[r#"
        ```php
        Child::fetch(): void
        ```

        ---

        Fetches the record."#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_inheritdoc_shows_parent_description() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Base {
    /** Sends the payload to the remote endpoint. */
    public function send(): void {}
}
class Child extends Base {
    /** {@inheritDoc} */
    public function send(): void {}
}
$c = new Child();
$c->sen$0d();
"#,
        )
        .await;
    expect![[r#"
        ```php
        Child::send(): void
        ```

        ---

        Sends the payload to the remote endpoint."#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_inherited_method_shows_child_class_name() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
class Animal { public function spe$0ak(): string { return '...'; } }
class Dog extends Animal {}
$d = new Dog();
$d->speak();
"#,
        )
        .await;
    // Hovering on the declaration itself — should show Animal::speak.
    expect![[r#"
        ```php
        public function speak(): string
        ```"#]]
    .assert_eq(&v);
}

/// `$dog->speak()` where Dog extends Animal must show Dog::speak (via extends
/// walk) when another class also has a method called `speak`.
#[tokio::test]
async fn hover_multi_trait_alpha() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
trait A { public function alpha(): int { return 1; } }
trait B { public function beta(): int { return 2; } }
class Both {
    use A;
    use B;
    public function run(): int { return $this->$0alpha() + $this->beta(); }
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        Both::alpha(): int
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_multi_trait_beta() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
trait A { public function alpha(): int { return 1; } }
trait B { public function beta(): int { return 2; } }
class Both {
    use A;
    use B;
    public function run(): int { return $this->alpha() + $this->$0beta(); }
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        Both::beta(): int
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_trait_identifier() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
trait Logg$0able { public function log(): void {} }
"#,
        )
        .await;
    expect![[r#"
        ```php
        trait Loggable
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_trait_inherited_method() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
trait Greeting {
    public function sayHello(string $name): string {
        return "Hello, {$name}";
    }
}
class Greeter {
    use Greeting;
    public function run(): string {
        return $this->$0sayHello('world');
    }
}
"#,
        )
        .await;
    expect![[r#"
        ```php
        Greeter::sayHello(string $name): string
        ```"#]]
    .assert_eq(&v);
}

#[tokio::test]
async fn hover_trait_method_picks_correct_class_not_unrelated_one() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let v = s
        .check_hover(
            r#"<?php
trait Pingable { public function ping(): string { return 'pong'; } }
class Server { use Pingable; }
class Client { public function ping(): bool { return false; } }
$s = new Server();
$s->pin$0g();
"#,
        )
        .await;
    // Must show Server::ping (from trait), returning string — not Client::ping.
    expect![[r#"
        ```php
        Server::ping(): string
        ```"#]]
    .assert_eq(&v);
}
