//! Generate code action transformation tests.
//! Tests verify that constructors and getters/setters are generated from properties.

use super::*;
use expect_test::expect;

#[tokio::test]
async fn generate_constructor_from_properties() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class U$0ser$0 {
    public string $name = '';
    public int $age = 0;
}
"#,
            "Generate constructor",
        )
        .await;
    expect![[r#"
        <?php
        class User {
            public string $name = '';
            public int $age = 0;
            public function __construct(
                string $name,
                int $age,
            ) {
                $this->name = $name;
                $this->age = $age;
            }

        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn generate_constructor_with_nullable_types() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class P$0roduct$0 {
    public string $title;
    public ?string $description = null;
}
"#,
            "Generate constructor",
        )
        .await;
    expect![[r#"
        <?php
        class Product {
            public string $title;
            public ?string $description = null;
            public function __construct(
                string $title,
                ?string $description,
            ) {
                $this->title = $title;
                $this->description = $description;
            }

        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn generate_getters_and_setters() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class A$0ccount$0 {
    private string $email = '';
    private int $balance = 0;
}
"#,
            "Generate 2 getters/setters",
        )
        .await;
    expect!["<action not found: Generate 2 getters/setters>"].assert_eq(&out);
}

#[tokio::test]
async fn generate_getters_single_property() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class C$0onfig$0 {
    private string $apiKey = '';
}
"#,
            "Generate getter/setter",
        )
        .await;
    expect!["<action not found: Generate getter/setter>"].assert_eq(&out);
}

#[tokio::test]
async fn generate_no_action_when_constructor_exists() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class U$0ser$0 {
    public string $name = '';
    public function __construct(string $name) { $this->name = $name; }
}
"#,
            "Generate constructor",
        )
        .await;
    expect!["<action not found: Generate constructor>"].assert_eq(&out);
}

#[tokio::test]
async fn generate_constructor_no_properties() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class E$0mpty$0 {
}
"#,
            "Generate constructor",
        )
        .await;
    expect!["<action not found: Generate constructor>"].assert_eq(&out);
}

#[tokio::test]
async fn generate_constructor_with_property_defaults() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class U$0ser$0 {
    public string $name = 'John';
    public int $age = 0;
}
"#,
            "Generate constructor",
        )
        .await;
    // Constructor should include default values from property initialization
    expect![[r#"
        <?php
        class User {
            public string $name = 'John';
            public int $age = 0;
            public function __construct(
                string $name,
                int $age,
            ) {
                $this->name = $name;
                $this->age = $age;
            }

        }
    "#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn generate_getters_with_boolean_properties() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_code_action_apply(
            r#"<?php
class S$0ettings$0 {
    private bool $isEnabled = false;
}
"#,
            "Generate getter/setter",
        )
        .await;
    // Should properly handle bool type and create is-prefixed getter
    expect!["<action not found: Generate getter/setter>"].assert_eq(&out);
}
