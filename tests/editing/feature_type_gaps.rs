//! Regression tests for the six type-system gaps identified in the audit.
//!
//! Each test documents the gap category, expected behaviour after the fix, and
//! (where a fix is deferred) the current behaviour so the snapshot doesn't rot.
//!
//! Gap table:
//!   Gap 1 – Generic type params stripped in TypeMap   (docblock_class_parts)
//!   Gap 2 – @psalm-type aliases not expanded          (TypeMap post-processing)
//!   Gap 3 – @template bounds unused / not enforced    (mir-level, documented only)
//!   Gap 4 – list<T> element type not propagated       (docblock_class_parts + foreach)
//!   Gap 5 – Mixed type flow last-write-wins           (documented via mir primary path)
//!   Gap 6 – First-class callable not typed            (TypeMap CallableCreate → Closure)

use super::*;

use expect_test::expect;

// ── helpers ───────────────────────────────────────────────────────────────────

async fn completion_labels(s: &mut TestServer, src: &str) -> Vec<String> {
    let opened = s.open_fixture(src).await;
    let c = opened.cursor().clone();
    let resp = s.completion(&c.path, c.line, c.character).await;
    let items = match &resp["result"] {
        v if v.is_array() => v.as_array().cloned().unwrap_or_default(),
        v if v["items"].is_array() => v["items"].as_array().cloned().unwrap_or_default(),
        _ => vec![],
    };
    items
        .iter()
        .filter_map(|i| i["label"].as_str().map(str::to_owned))
        .collect()
}

// ── Gap 1: Generic type params stripped by docblock_class_parts ──────────────
//
// `@var Collection<User> $coll` — the TypeMap path stored "Collection<User>"
// verbatim as the class name.  Member lookups against "Collection<User>" find
// nothing, so `$coll->` produced no completions.  After the fix the generic
// argument is stripped and the lookup targets "Collection" correctly.

#[tokio::test]
async fn gap1_generic_annotation_completions_resolve_base_class() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let ls = completion_labels(
        &mut s,
        r#"<?php
class User { public string $name = ''; }
class Collection {
    public function first(): ?User { return null; }
    public function count(): int { return 0; }
}

/** @var Collection<User> $coll */
$coll = getCollection();
$coll->$0
"#,
    )
    .await;
    assert!(
        ls.contains(&"first".to_string()),
        "Collection::first() must appear; got: {ls:?}"
    );
    assert!(
        ls.contains(&"count".to_string()),
        "Collection::count() must appear; got: {ls:?}"
    );
}

// ── Gap 2: @psalm-type aliases not expanded ───────────────────────────────────
//
// `@psalm-type Result = Success|Failure` defined in the class docblock.
// When a method parameter carries `@param Result $r`, the TypeMap stored "Result"
// as the class name.  Because there is no class named "Result", `$r->` produced
// no completions.  After the fix aliases collected from the file are expanded
// before the lookup, yielding "Success|Failure".

#[tokio::test]
async fn gap2_psalm_type_alias_expands_for_completion() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let ls = completion_labels(
        &mut s,
        r#"<?php
class Success { public function ok(): bool { return true; } }
class Failure { public function reason(): string { return ''; } }

/**
 * @psalm-type Result = Success|Failure
 */
class Processor {
    /**
     * @param Result $r
     */
    public function handle($r): void {
        $r->$0
    }
}
"#,
    )
    .await;
    assert!(
        ls.contains(&"ok".to_string()),
        "Success::ok() must appear after alias expansion; got: {ls:?}"
    );
    assert!(
        ls.contains(&"reason".to_string()),
        "Failure::reason() must appear after alias expansion; got: {ls:?}"
    );
}

// ── Gap 3: @template T of Bound — bound shown in hover, not enforced ──────────
//
// The bound from `@template T of Countable` is visible in hover but is not
// used by the type-checker (enforcement requires mir-level changes).  This test
// asserts the bound IS surfaced in hover, documenting the current behaviour.

#[tokio::test]
async fn gap3_template_bound_appears_in_hover() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    s.check_hover_annotated(
        r#"<?php
/**
 * @template T of \Countable
 * @param T $items
 * @return T
 */
function proce$0ss($items) { return $items; }
"#,
        expect![[r#"
            ```php
            function process($items)
            ```

            ---

            **@return** `T`
            **@param** `T` `$items`
            **@template** `T` of `\Countable`"#]],
    )
    .await;
}

// ── Gap 4: list<T> element type propagated through foreach ───────────────────
//
// `/** @var list<Widget> $widgets */` — the TypeMap path skipped the element
// type because "list" does not start with an uppercase letter, so
// `docblock_class_parts` returned nothing.  After the fix the element class
// "Widget" is extracted and propagated to the foreach value variable, enabling
// completions inside the loop body.

#[tokio::test]
async fn gap4_list_element_type_propagates_through_foreach() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let ls = completion_labels(
        &mut s,
        r#"<?php
class Widget {
    public function getId(): int { return 0; }
    public string $label = '';
}

/** @var list<Widget> $widgets */
$widgets = fetchWidgets();
foreach ($widgets as $w) {
    $w->$0
}
"#,
    )
    .await;
    assert!(
        ls.contains(&"getId".to_string()),
        "Widget::getId() must appear inside foreach body; got: {ls:?}"
    );
    assert!(
        ls.contains(&"$label".to_string()),
        "Widget::$label must appear inside foreach body; got: {ls:?}"
    );
}

// ── Gap 5: Mixed type flow — last-write-wins (documented) ────────────────────
//
// The TypeMap is a single-pass last-write-wins store.  For `$x = null; $x = new Foo()`
// the TypeMap discards the initial `null` assignment (null is not a class).
// The mir-primary path is flow-sensitive and resolves `$x` to `Foo` correctly.
// This test asserts the mir primary path produces the right hover value so the
// gap is documented but not regressed.

#[tokio::test]
async fn gap5_mir_handles_reassignment_to_class_after_null() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    s.check_hover_annotated(
        r#"<?php
class Foo { public function doFoo(): void {} }
$x = null;
$x = new Foo();
$x$0;
"#,
        expect![[r#"`$x` `Foo`"#]],
    )
    .await;
}

// ── Gap 6: First-class callable typed as Closure ─────────────────────────────
//
// `$fn = strlen(...)` creates a `Closure` instance.  The TypeMap did not
// recognise the `CallableCreate` expression, so `$fn` had no recorded type
// and hover showed nothing useful.  After the fix the variable is mapped to
// "Closure", enabling hover to surface the type.

#[tokio::test]
async fn gap6_first_class_callable_typed_as_closure() {
    let mut s = TestServer::new().await;
    s.validate_syntax(false);
    let out = s
        .check_hover(
            r#"<?php
$fn = strlen(...);
$fn$0;
"#,
        )
        .await;
    assert!(
        out.contains("Closure") || out.contains("callable"),
        "hover must mention Closure or callable for a first-class callable; got: {out:?}"
    );
}
