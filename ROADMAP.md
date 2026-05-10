# php-lsp Roadmap

**Last updated**: 2026-05-09 — Fixed issues #6 & #7 (interface extends, switch/match semantic tokens)

---

## Known Limitations

### `textDocument/declaration` — Unopened Files
- **Phase 1 ✅**: All symbol types supported for unopened files via FileIndex fallback (interfaces, abstract classes, traits, enums, methods, constants, trait abstract methods, enum cases)
- **Phase 2 ✅**: Precise character positions for unopened files (extended FileIndex with `name_char: u32`; 18 E2E tests)

### Global Constants
Top-level `const` declarations not matched (only class/enum constants). Rationale: global constants uncommon in modern PHP; namespacing preferred.

### Case Sensitivity
Declaration matching is case-sensitive for all symbols (following editor conventions, not PHP runtime behavior).

---

## Bug Fixes

Bugs found in source-code audit of feature implementations. Each needs a wire-protocol E2E test added under `tests/`.

### Critical — Silently broken

**1. `src/type_definition.rs` — Nullable, union, and intersection parameter types always return no result**

`param_type_for` returns `format_type_hint(type_hint)` verbatim (e.g. `"?Foo"`, `"Foo|Bar"`). `find_class_range` then does an exact name comparison: no class is ever declared as `"?Foo"`, so goto-type-definition silently returns `None` for every nullable, union, or intersection-typed parameter.

Fix: strip the leading `?` and split on `|`/`&` before passing to `find_class_range`; return the first hit.

Test file: `tests/navigation/feature_type_definition.rs`

---

### High — Produces broken PHP or wrong results

**2. `src/walk.rs:190` — Arrow functions treated as hard scope boundaries**

`VarRefsVisitor::visit_expr` returns `ControlFlow::Continue(())` on `ExprKind::ArrowFunction(_)`. PHP arrow functions auto-capture outer-scope variables; `$x` inside `fn() => $x + 1` is the outer `$x`. Renaming the outer `$x` leaves the arrow function body unrenamed, producing broken PHP.

Fix: recurse into the arrow function body (unlike closures, arrow functions are not scope boundaries for reads).

Test file: `tests/navigation/feature_rename.rs`

**3. `src/walk.rs` — Closure `use ($x)` clause skipped during variable rename**

When `VarRefsVisitor` stops at `ExprKind::Closure(_)`, it does not inspect the closure's `use_vars` list. Renaming outer `$x` leaves `use ($x)` pointing at the old (now-undefined) name.

Fix: before returning at a closure boundary, collect `use_vars` entries matching the target name and add their spans to the output.

Test file: `tests/navigation/feature_rename.rs`

**4. `src/rename.rs` — Rename bleeds across unrelated namespaces**

`rename()` calls `find_references_with_use(word, all_docs, true)` with no FQN context. Two unrelated classes both named `Foo` in different namespaces will both be renamed. `find_references_with_target` exists for this purpose but is not called.

Fix: resolve the FQN of the symbol under the cursor before collecting edits, then pass it to `find_references_with_target`.

Test file: `tests/navigation/feature_rename.rs`

**5. `src/call_hierarchy.rs` — Constructor calls missing from outgoing calls**

`CallCollector::visit_expr` has no arm for `ExprKind::New`. Constructor calls (`new Foo(...)`) never appear as outgoing calls.

Fix: add an `ExprKind::New(n)` arm that extracts the class name and emits it as an outgoing call.

Test file: `tests/navigation/feature_hierarchy.rs`

---

### Medium — Wrong results in common PHP patterns

**6. `src/semantic_tokens.rs` — `foreach` key/value variables not tokenized**

`collect_stmt` for `StmtKind::Foreach` collects `f.expr` and `f.body` but not `f.key` or `f.value`. The `$k` and `$v` in `foreach ($a as $k => $v)` get no `TT_VARIABLE` token.

Fix: call `collect_expr` on `f.key` (if present) and `f.value` in the Foreach arm.

Test file: `tests/analysis/feature_semantic_tokens.rs`

**7. `src/hover/hover.rs:475–479` — `parent::` resolution only searches current document**

`find_parent_class_name` is called only on `&doc.program().stmts`. For cross-file inheritance the parent class is not found, and hover on `parent::method()` silently returns nothing.

Fix: after failing in the current doc, iterate `all_docs` to resolve the parent class name.

Test file: `tests/editing/feature_hover.rs`

**8. `src/inlay_hints.rs:82–108` — Method hints keyed by short name; wrong hints for same-named methods**

`collect_defs_from_workspace` inserts methods into the `defs` map keyed by bare `method_name`. When two classes both define `process()` with different signatures, whichever is inserted last wins.

Fix: key as `"ClassName::methodName"` and resolve the receiver class at each call site before lookup.

Test file: `tests/analysis/feature_inlay_hints.rs`

---

### Low — Missing data / minor edge cases

**9. `src/promote_action.rs` — Property type hint not propagated to promoted parameter**

`prop_info` stores visibility and `is_readonly` but not the type hint. Promoting `private string $name` where the constructor param has no explicit type emits `private $name`, losing `string`.

Fix: store the type hint string in `prop_info` and emit it when the parameter lacks an explicit type.

Test file: `tests/editing/feature_code_actions.rs`

---

## Code Quality

Practices diverging from CLAUDE.md. Address before next release.

**1. Unit tests in `src/` violate LSP testing rule**

`src/promote_action.rs`, `src/type_definition.rs`, `src/rename.rs` contain `#[test]` that call internal functions directly. Move to `tests/` and rewrite to use `TestServer` wire protocol.

**2. Excessive explanatory comments**

`backend.rs` (61 comments) and `references.rs` (22 comments) explain what the code does. Keep only non-obvious WHY comments (hidden constraint, subtle invariant, workaround for a specific bug).

**3. Clippy suppressions with unresolved root causes**

- `ast.rs` suppresses `vec_box`
- `hover.rs` suppresses `only_used_in_recursion`

Investigate and fix rather than suppress.

---

## Code Actions

Current implementation: 10 actions (6 deferred, 4 eager). Analysis completed 2026-05-09.

### Current Actions

**Deferred** (resolved on-demand via `code_action_resolve`):
1. Generate constructor
2. Generate getters/setters
3. Implement missing methods (quick-fix)
4. Add return type to functions
5. Generate PHPDoc
6. Promote constructor parameters

**Eager** (computed immediately):
1. Extract variable
2. Extract method
3. Extract constant
4. Inline variable
5. Organize imports / Remove unused use statements
6. Add use import (for undefined classes)

### High-Priority Additions

**Phase 1 — Diagnostic-driven quick-fixes** (2–3 hours each)

Quick-fixes leverage existing semantic diagnostics; high user visibility.

1. **Add use import for undefined functions** — Extends current `UndefinedClass` handler to `UndefinedFunction`
2. **Missing method parameter arity** — From `InvalidArgument` diagnostics
3. **Fix nullable property access** — From `NullPropertyFetch` diagnostics; suggest `??` operator

**Phase 2 — Refactoring actions** (4–6 hours each)

Lower urgency but valuable for code modernization.

1. **Inline variable** — Inverse of extract variable (remove explicit assignment, substitute usage)
2. **Inline method** — Remove trivial methods and inline their bodies at call sites
3. **Introduce parameter object** — Extract 3+ related parameters into a value object
4. **Improve return type inference** — Replace `: mixed` with inferred union types (`: int|string`)

**Phase 3 — Code generation** (3–4 hours each)

Extend existing generators.

1. **Generate static factory methods** — `create()`, `fromArray()` for classes
2. **Generate magic methods** — `__toString()`, `__isset()`, `__get()`, `__set()`
3. **Fill union types** — Convert `mixed` properties to `int|string` based on analysis

### Known Limitations

**Extract constant** — Currently limited to simple literals; could expand to:
- Repeated array structures  
- Repeated SQL queries / API endpoints  
- Class properties with immutable values

**Generate methods** — Only constructors and getters/setters; comparison methods (`equals()`, `compareTo()`) not yet generated.

**Return type inference** — Only detects missing types and suggests `: mixed`; doesn't infer actual return type from statements.

---

## Performance

**Baseline profile** (samply + dhat on 1609-file Laravel fixture):

| Component | % of total |
|-----------|-----------|
| I/O (sequential reads in benchmark, parallel in production) | 37.6% |
| Salsa `file_index` query + parsing | 12.2% |
| Analyzer (`DefinitionCollector`, `DocblockParser`) | 17.6% |
| Parser + lexer | ~8% |
| Other (string ops, arenas, misc) | ~24% |

### Priority 1 — Per-keystroke MirDb rebuild cascade [BLOCKED on mir-codebase]

`file_definitions` returns a new `Arc::new(StubSlice)` pointer on every text edit (even whitespace-only), triggering `cached_mir_db` cache invalidation via `Arc::ptr_eq`. Full MirDb rebuild per keystroke: 0.1s for 1609 files, ~3s for 50k-file workspace.

Fix strategy: requires `StubSlice: PartialEq` in `mir-codebase` for structural comparison. Once landed:

1. Update `SliceArc::maybe_update` in `src/db/definitions.rs` to compare content
2. Add element-wise comparison to `CodebaseArc::maybe_update`; remove `no_eq` from `codebase` query

Action: file mir issue. Reference: `src/db/codebase.rs:59-66`, `src/db/analysis.rs:51-67`.

### Priority 2 — Analyzer-side allocations [OUT-OF-TREE, ../mir/]

dhat top 3 allocation sites by bytes:

- `DefinitionCollector::build_method_storage` — 12.1 MB / 7,936 blocks
- `DocblockParser::parse` — 6.0 MB / 7,418 blocks
- `DefinitionCollector::visit_stmt` — 5.9 MB / 10,623 blocks

Action: file mir issue with dhat numbers; request pre-allocation / arena strategy.

### Priority 3 — Incremental-edit latency profile

Build a tool that simulates N keystrokes on an open file and measures ms from edit to response. Needed after Priority 1 lands to confirm the MirDb fix delivers the expected latency win.

---

## Lazy Vendor Indexing [NOT YET IMPLEMENTED]

Design complete (2026-04-29). `scan_workspace` currently parses all `vendor/` files on `initialized`; for symfony/demo (5247 files, ~5200 vendor) this exceeds the 10s test-harness timeout.

### Mechanism

- **At scan time**: classify each `.php` path by vendor-segment check. Workspace paths parse as today; vendor paths are pushed to `DocumentStore::vendor_paths` without reading.
- **On demand**: `lazy_sweep_vendor(needle)` reads each un-mirrored vendor path, applies a substring prefilter (`text.contains(needle)`), and parses only hits. Idempotent.
- **Skip-sweep guard**: if the symbol under the cursor is workspace-declared, the sweep is skipped (Composer dependency direction is one-way).

### Config

```rust
pub lazy_vendor_index: bool,  // default: false; JSON key: "lazyVendorIndex"
```

### API surface

| File | Change |
|---|---|
| `src/backend.rs` | Add `lazy_vendor_index` to `LspConfig`; hook `lazy_sweep_vendor` into references handler |
| `src/backend.rs::scan_workspace` | Classify vendor paths; skip `warm_reference_index` under lazy mode |
| `src/document_store.rs` | `vendor_paths: Arc<RwLock<Vec<PathBuf>>>`, `push_vendor_path`, `lazy_sweep_vendor` |
| `src/autoload.rs` | `is_vendor_path(p: &Path) -> bool` — single source of truth for segment check |
| `tests/fixtures/lazy-vendor/` | Synthetic on-disk fixture (composer.json, src/, vendor/acme/) |
| `tests/feature_lazy_vendor.rs` | 8 wire-protocol snapshot tests (T1–T8) |

### Rollout (4-PR series)

1. **PR 1**: core implementation + 8 tests, default `false`
2. **PR 2**: extend to `findImplementations`, type hierarchy
3. **PR 3**: flip default to `true` (after release cycle on CI)
4. **PR 4** (optional): persistent disk substring index

### Acceptance criteria for v1

- All 8 tests in `feature_lazy_vendor.rs` pass (`cargo test`, debug + release)
- `feature_symfony_demo` flips to `lazyVendorIndex: true` and loses `#[ignore]`
- `cargo test -- --ignored` is empty for the symfony bucket
- Cold-start latency on symfony-demo drops below 500 ms

---

## Lower Priority (Polish)

- **Audit Clone() usage** — 303 calls total; benchmark hot paths before changing
- **Modularize large files** — `backend.rs` (4,361 lines) → split by handler family; `hover.rs` (3,084 lines) → extract type narrowing, `@template` logic
