//! Generic-diagnostics fixtures (WP2, plan T1.8 / m1).
//!
//! mir 0.30 already emits the PHPStan-style generic diagnostics
//! `InvalidTemplateParam` (MIR0900) and `ShadowedTemplateParam` (MIR0901), and
//! they already pass the LSP's `issue_passes_filter`. These tests pin that they
//! surface through the LSP `publishDiagnostics` path (no net-new analysis code).

use super::*;

/// MIR0900 — passing a type argument that violates a `@template T of Bound`
/// upper bound. Here `@template TKey of array-key` (i.e. `int|string`) is
/// inferred as `object` from the `array<object, int>` argument, which violates
/// the bound. Modeled on mir's own `invalid_template_param` fixture.
#[tokio::test]
async fn invalid_template_param_surfaces_mir0900() {
    let mut server = TestServer::new().await;
    let notif = server
        .open(
            "invalid_template_param.php",
            r#"<?php
/**
 * @template TKey of array-key
 * @template TValue
 * @param array<TKey, TValue> $array
 * @return list<TKey>
 */
function array_key_list(array $array): array { return []; }

/** @param array<object, int> $arr */
function test(array $arr): void {
    array_key_list($arr);
}
"#,
        )
        .await;
    let diags = notif["params"]["diagnostics"].as_array().unwrap();
    let hit = diags
        .iter()
        .find(|d| d["code"].as_str() == Some("InvalidTemplateParam"));
    assert!(
        hit.is_some(),
        "expected an InvalidTemplateParam (MIR0900) diagnostic, got: {diags:#?}"
    );
}

/// MIR0901 — a method-level `@template T` that shadows a class-level template
/// parameter of the same name. Modeled on mir's own `shadowed_template_param`
/// fixture.
#[tokio::test]
async fn shadowed_template_param_surfaces_mir0901() {
    let mut server = TestServer::new().await;
    let notif = server
        .open(
            "shadowed_template_param.php",
            r#"<?php
/** @template T */
class Box {
    /**
     * @template T
     * @param T $value
     * @return T
     */
    public function transform($value) { return $value; }
}

function test(): void {
    /** @var Box<string> $box */
    $box = new Box();
    $box->transform('hello');
}
"#,
        )
        .await;
    let diags = notif["params"]["diagnostics"].as_array().unwrap();
    let hit = diags
        .iter()
        .find(|d| d["code"].as_str() == Some("ShadowedTemplateParam"));
    assert!(
        hit.is_some(),
        "expected a ShadowedTemplateParam (MIR0901) diagnostic, got: {diags:#?}"
    );
}

/// VF13 (negative): valid generics — a bound satisfied by the type argument and
/// distinct class/method template names — produce NEITHER MIR0900
/// (`InvalidTemplateParam`) NOR MIR0901 (`ShadowedTemplateParam`). Guards against
/// a future over-reporting regression in mir or `issue_passes_filter`.
#[tokio::test]
async fn valid_generics_emit_no_generic_diagnostics() {
    let mut server = TestServer::new().await;
    let notif = server
        .open(
            "valid_generics.php",
            r#"<?php
class Base {}
class Derived extends Base {}

/**
 * @template T of Base
 */
class Box {
    /**
     * @template U
     * @param U $value
     * @return U
     */
    public function transform($value) { return $value; }
}

function test(): void {
    /** @var Box<Derived> $box */
    $box = new Box();
    $box->transform('hello');
}
"#,
        )
        .await;
    let diags = notif["params"]["diagnostics"].as_array().unwrap();
    let bad: Vec<&str> = diags
        .iter()
        .filter_map(|d| d["code"].as_str())
        .filter(|c| *c == "InvalidTemplateParam" || *c == "ShadowedTemplateParam")
        .collect();
    assert!(
        bad.is_empty(),
        "valid generics must not produce generic diagnostics, got: {diags:#?}"
    );
}
