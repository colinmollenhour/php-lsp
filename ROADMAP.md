# php-lsp Roadmap

**Last updated**: 2026-05-10 — All 9 bugs fixed; 3 signature help bugs fixed; comprehensive E2E test migration complete

---

## Completed

| Item | Commit | Notes |
|---|---|---|
| Bug #1 — nullable/union type_definition | `67b1495` | `param_type_for` strips `?`, splits `\|`/`&` |
| Bug #2 — arrow function scope in rename | `cca95bf` | ArrowFunction no longer a hard boundary |
| Bug #3 — closure `use` clause in rename | `cca95bf` | `use_vars` entries now collected |
| Bug #4 — rename bleeds across namespaces | `cca95bf` | FQN resolved before collecting edits |
| Bug #5 — constructors missing from call hierarchy | `cca95bf` | `ExprKind::New` arm added |
| Bug #6 — `foreach` key/value not tokenized | `fc16e54` | `collect_expr` called on `f.key`/`f.value` |
| Bug #7 — `parent::` hover only searches current doc | `cca95bf` | Falls through to `all_docs` on miss |
| Bug #8 — inlay hints keyed by bare method name | `cca95bf` | Keyed as `ClassName::methodName` |
| Bug #9 — promote action loses type hint | `cca95bf` | Type hint stored and emitted |
| Regression tests for #2–4, #6, #8–9 | `5d90a78` | Wire-protocol E2E tests added |
| `backend.rs` split into focused modules | `4fc0102` | Handler families in separate files |
| `hover.rs` split into semantic modules | `b3d04d9` | Type narrowing, @template extracted |
| Excessive "what" comments removed | `fc4157c` | `backend.rs`, `references.rs` cleaned |
| Unit tests removed from `src/promote_action.rs`, `src/rename.rs` | — | Moved to wire-protocol tests |

---

## Known Limitations

### Global Constants
Top-level `const` declarations not matched (only class/enum constants). Rationale: global constants uncommon in modern PHP; namespacing preferred.

### Case Sensitivity
Declaration matching is case-sensitive for all symbols (following editor conventions, not PHP runtime behavior).

---

## Open Work

### PR 1 — Lazy Vendor Indexing: Core

Design complete (2026-04-29). `scan_workspace` currently parses all `vendor/` files on `initialized`; for symfony-demo (5,247 files, ~5,200 vendor) this exceeds the 10s test-harness timeout. 11 tests in `tests/frameworks/feature_symfony.rs` are `#[ignore]` until this lands.

**Mechanism**

- **At scan time**: classify each `.php` path via `is_vendor_path`. Workspace files parse as today; vendor paths pushed to `DocumentStore::vendor_paths` without reading.
- **On demand**: `lazy_sweep_vendor(needle)` reads each un-mirrored vendor path, applies a substring prefilter (`text.contains(needle)`), parses only hits. Idempotent.
- **Skip-sweep guard**: if symbol under cursor is workspace-declared, sweep is skipped (Composer dependency direction is one-way).

**Config**

```rust
pub lazy_vendor_index: bool,  // default: false; JSON key: "lazyVendorIndex"
```

**API surface**

| File | Change |
|---|---|
| `src/config.rs` | Add `lazy_vendor_index: bool` to `LspConfig` |
| `src/autoload.rs` | `pub fn is_vendor_path(p: &Path) -> bool` — single source of truth |
| `src/document_store.rs` | `vendor_paths: Arc<RwLock<Vec<PathBuf>>>`, `push_vendor_path`, `lazy_sweep_vendor` |
| `src/backend.rs` (scan_workspace) | Classify vendor paths; skip `warm_reference_index` under lazy mode |
| `src/backend.rs` (references handler) | Call `lazy_sweep_vendor(word)` when `lazy_vendor_index = true` |
| `tests/fixtures/lazy-vendor/` | Synthetic fixture: `composer.json`, `src/`, `vendor/acme/` |
| `tests/frameworks/feature_lazy_vendor.rs` | 8 wire-protocol snapshot tests (T1–T8) |

**Acceptance criteria**

- All 8 T1–T8 tests pass (`cargo test`, debug + release)
- `cargo test -- --ignored` is empty for the symfony bucket

---

### PR 2 — Lazy Vendor Indexing: Extend + Unblock Symfony

- Extend `lazy_sweep_vendor` call into `goto_implementation` and `supertypes`/`subtypes` handlers
- Flip symfony-demo fixture to `lazyVendorIndex: true`
- Remove all 11 `#[ignore]` markers from `tests/frameworks/feature_symfony.rs`

---

### PR 3 — Phase 1 Code Actions: Diagnostic-Driven Quick-Fixes

Leverages diagnostics already emitted by the mir analyzer (`IssueKind::UndefinedFunction`, `IssueKind::NullPropertyFetch`, `IssueKind::InvalidArgument` — all confirmed present in `src/semantic_diagnostics.rs:159–179`).

**Actions to add** (2–3 hours each):

1. **Add `use` import for undefined function** — extends `src/use_import.rs` `UndefinedClass` handler to `UndefinedFunction`; searches `workspace_index` for a matching function, proposes `use FQN;`
2. **Suggest `??` for nullable property access** — detects `NullPropertyFetch`; wraps access in `?? default`; implement in new `src/null_coalesce_action.rs`
3. **Add missing argument** — detects `InvalidArgument` on under-arity calls; adds `null` placeholders or default-value params; implement in new `src/arity_action.rs`

Tests: `tests/editing/feature_code_actions.rs` snapshot tests for each.

---

### PR 4 — Performance: Incremental MirDb Updates [NOW UNBLOCKED]

**Status**: Fully unblocked — `StubSlice: PartialEq` is the only missing piece in mir, and all constituent types already derive it.

**Root cause (two layers)**

Every text edit causes a full MirDb rebuild because of two compounding problems:

1. **`file_definitions` always produces a new pointer** (`#[salsa::tracked(no_eq)]` + `Arc::new(StubSlice)` on every re-run). Even a whitespace-only edit creates a new Arc, breaking the `cached_mir_db` pointer check (`Arc::ptr_eq`).

2. **`cached_mir_db` does a full rebuild on any pointer miss** — `clone(stubs_db) + ingest × N`. It does not use `MirDb`'s incremental API (`remove_file_definitions` + `ingest_stub_slice`) which already exists in mir for exactly this purpose.

Cost: 0.1s for 1,609 files; ~3s for 50k-file workspace, per keystroke.

**Four-part fix**

| Step | Location | Change |
|---|---|---|
| 1 | `../mir/crates/mir-codebase/src/storage.rs:515` | Add `PartialEq` to `StubSlice`'s derive: `#[derive(Debug, Clone, Default, PartialEq, …)]` — one word. All constituent types (`ClassStorage`, `MethodStorage`, `InterfaceStorage`, `TraitStorage`, `EnumStorage`, `FunctionStorage`, `Union`, `Atomic`) already derive it. |
| 2 | `src/db/definitions.rs:25` | Change `SliceArc::maybe_update` from pointer equality to content equality: `*old.0 == new_value.0`. |
| 3 | `src/db/definitions.rs:41` | Remove `no_eq` from `#[salsa::tracked(no_eq)]`. With content equality in `Update`, salsa skips downstream queries when definitions are unchanged (whitespace-only edits, comment edits, etc.). |
| 4 | `src/db/analysis.rs:58` | Replace the full-rebuild in `cached_mir_db` with incremental updates using `MirDb::remove_file_definitions(file) + MirDb::ingest_stub_slice(new_slice)`. Track per-file `Arc<StubSlice>` in the cache keyed by `slice.file`. On each call: diff incoming slices against cached; for removed files call `remove_file_definitions`; for changed-pointer files call `remove + ingest`. |

Steps 1–3 ensure definition-preserving edits reuse the old Arc pointer (O(1) salsa check). Step 4 ensures definition-changing edits update only the affected file in MirDb, not all N files (O(changed_files) instead of O(N)).

**New `MirDbCache` shape** (`src/db/analysis.rs`):

```rust
struct IncrementalMirDb {
    php_version: PhpVersion,
    db: MirDb,                                      // persistent, never rebuilt from scratch
    per_file: HashMap<Arc<str>, Arc<StubSlice>>,    // file_uri → last-ingested slice
}
type MirDbCache = Arc<Mutex<Option<IncrementalMirDb>>>;
```

**Expected outcome**

| Edit type | Before | After |
|---|---|---|
| Whitespace / comment only | Full rebuild (~100 ms) | Salsa cache hit — 0 ms |
| Method body change (no signature change) | Full rebuild (~100 ms) | `remove + ingest` 1 file (~1 ms) |
| New class / method added | Full rebuild (~100 ms) | `remove + ingest` 1 file (~1 ms) |

**Verification**

```bash
cargo test                              # no regressions
cargo bench --bench edit_latency        # before/after comparison
```

Build `benches/edit_latency.rs` before applying the fix as a baseline: open `benches/fixtures/controller.php`, simulate 20 single-char `did_change` events, measure ms from edit to hover response.

---

### PR 5 — Upstream Issues: Blocked Diagnostic Tests

Three tests in `tests/analysis/feature_diagnostics.rs` are `#[ignore]` pending upstream mir-analyzer fixes. File issues at https://github.com/jorgsowa/mir:

| Test line | Reason | Issue to file |
|---|---|---|
| `:892` | FileAnalyzer not in public API; `StatementsAnalyzer` skips traits | Request `FileAnalyzer` public surface or trait support in `StatementsAnalyzer` |
| `:998` | `too-many-arguments` not detected | Request arity checking in the analyzer |
| `:1097` | PSR-4 lazy-loading misses FQN `new` expressions | Request FQN `new` coverage in PSR-4 resolver |

Also file **analyzer allocation issue** with dhat numbers:
- `DefinitionCollector::build_method_storage` — 12.1 MB / 7,936 blocks
- `DocblockParser::parse` — 6.0 MB / 7,418 blocks
- `DefinitionCollector::visit_stmt` — 5.9 MB / 10,623 blocks

Remove each test's `#[ignore]` as the corresponding upstream fix lands.

---

## Code Actions — Planned Additions

### Current (16 total)

**Deferred** (resolved via `code_action_resolve`):
1. Generate constructor
2. Generate getters/setters
3. Implement missing methods
4. Add return type to functions
5. Generate PHPDoc
6. Promote constructor parameters

**Eager** (computed immediately):
1. Extract variable
2. Extract method
3. Extract constant
4. Inline variable
5. Organize imports / remove unused `use`
6. Add `use` import for undefined classes

### Phase 1 — Diagnostic quick-fixes (PR 3 above)
7. Add `use` import for undefined functions
8. Suggest `??` for nullable property access
9. Add missing arguments

### Phase 2 — Refactoring actions (future)
10. Inline variable — inverse of extract variable
11. Inline method — remove trivial methods, inline body at call sites
12. Introduce parameter object — extract 3+ related params into a value object
13. Improve return type — replace `: mixed` with inferred union types

### Phase 3 — Code generation (future)
14. Generate static factory methods — `create()`, `fromArray()`
15. Generate magic methods — `__toString()`, `__isset()`, `__get()`, `__set()`
16. Fill union types — convert `mixed` to `int|string` based on analysis

---

## Performance Baseline

(samply + dhat on 1,609-file Laravel fixture)

| Component | % of total |
|---|---|
| I/O (sequential in bench, parallel in prod) | 37.6% |
| Salsa `file_index` query + parsing | 12.2% |
| Analyzer (`DefinitionCollector`, `DocblockParser`) | 17.6% |
| Parser + lexer | ~8% |
| Other | ~24% |

---

## Feature Gaps Identified (E2E Test Coverage Work)

### Document Highlight
- **DocumentHighlightKind**: Implementation always returns `TEXT`, never `READ`/`WRITE`. LSP spec intends these to distinguish variable reads from writes (for color-coding). Requires data-flow analysis of variable assignments — significant feature addition, not bug fix.
- **Class constants not highlighted**: `Foo::CONST` references found but declarations not matched. Would require extending `refs_in_stmts` to handle class constant declarations.

### Signature Help
- **Cross-file lookup missing**: Functions/classes defined in other files are invisible to `find_signature`. Architecture walks single-document AST only. Fixing requires storing definitions in workspace-wide codebase view.
- **FQN user-defined functions unresolved**: `\App\foo()` calls still fail (only builtin FQN fixed). Would require namespace-aware symbol resolution in `find_signature`.
- **Multi-file closure scope**: Closure `use($var)` parameters not tested; behavior across file boundaries unclear.

### Range Formatting
- **Deterministic content assertions impossible**: All tests accept null (no formatter) or unspecified edits. Cannot assert that malformed code becomes specific formatted code without guaranteed formatter (php-cs-fixer/phpcbf) in CI. Tests remain "happy path only."

### On-Type Formatting
- **Position edge cases**: Cursor position handling when beyond line end not fully tested; current implementation returns empty edits (correct) but no explicit test coverage.

## Lower Priority (Polish)

- **Audit `Clone()` usage** — 303 calls total; benchmark hot paths before changing
- **Global constants** — top-level `const` support (low demand in modern PHP)
