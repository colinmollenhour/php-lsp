//! # php-lsp — domain vocabulary
//!
//! The terms below have one canonical meaning across the crate. Use them
//! consistently; do not introduce synonyms.
//!
//! ## Core nouns
//! - **`ParsedDoc`** — a fully parsed source file: AST program + source text +
//!   line table, backed by a bumpalo arena. The unit a user has open.
//! - **`FileIndex`** — the compact (~2 KB) serializable per-file *declaration
//!   summary* used for background-indexed (unopened) files.
//! - **`WorkspaceIndex`** (`WorkspaceIndexData`) — the aggregated cross-file
//!   index over every `FileIndex`, with name→location reverse maps.
//! - **`Workspace`** — the salsa input representing the project root / file set.
//!   (There is intentionally no "Codebase" type — it would be a synonym.)
//! - **`SymbolMap` / `SymbolEntry`** — per-file precomputed name→declarations
//!   table for *open* files; richer than `FileIndex`.
//! - **`Declaration`** (`resolve::Declaration`) — the AST node where a symbol is
//!   *introduced*. Shared by both the `goto_definition` and `goto_declaration`
//!   LSP requests (which stay distinct — see `navigation/`).
//! - **`Docblock`** — a PHPDoc `/** … */` comment and the data parsed from it.
//!
//! ## Naming conventions
//! - **verb `index` → `ingest`**: ingesting a file into the store is `ingest*`;
//!   the noun `index` (`FileIndex`, the `file_index` query) is always a data
//!   structure, never an action.
//! - **`doc`** as a value/variable means a `ParsedDoc`. Docblock-derived data
//!   uses the **`Doc`** type prefix (`DocMethod`, `DocProperty`) or a field
//!   named `docblock`. Never name a docblock field `doc`.
//! - **Type suffixes**: `-Def` = an owned declaration record stored in an index
//!   (`FunctionDef`, `ClassDef`); `-Entry` = a row returned from a name-keyed
//!   map (`SymbolEntry`); `-Ref` = an internal back-pointer/handle into an index
//!   (`ClassRef`, `DeclRef`) — **not** an LSP reference. The spelled-out word
//!   `Reference`/`refs` is reserved for a symbol *usage* in code (see
//!   `navigation/references.rs`, `walk.rs`).
//! - **`sv`** is the established local idiom for a borrowed `SourceView`.

// Private items in the modules below are only reachable through main.rs/backend.rs,
// not through this lib entry point, so rustc would flag them as dead. Pub items in
// a lib crate are never subject to dead_code, so only genuinely-internal items are
// suppressed — real dead code within public API surfaces will still be caught.
#![allow(dead_code)]

// Public modules exposed for benchmark crates.
pub mod cache;
pub mod completion;
pub mod db;
pub mod hover;
pub mod resolve;
pub mod type_map;
pub mod type_query;
pub mod walk;

// Document lifecycle (parsed AST, salsa document store, open-file state).
mod document;

// PHP-language model (config, autoload, phpstorm_meta, docblock, php_names)
// and generic text mechanics (offset/word/fuzzy/range).
mod lang;
mod text;

// Public module: compact symbol index for background-indexed files.
pub mod file_index;

// Public module: per-file memoized symbol map (name → Vec<SymbolEntry>).
pub mod symbol_map;

// Feature groups.
pub mod actions;
pub mod analysis;
pub mod editing;
pub mod navigation;

// Infrastructure modules.
pub mod backend;
mod file_rename;
pub mod panic_guard;
mod stub_members;
pub mod symbols;
#[cfg(test)]
mod test_utils;
mod use_import;
mod workspace_scan;

// Re-exports for benchmark crates that use the flat `php_lsp::X` paths.
pub use analysis::semantic_diagnostics;
pub use document::ast;
pub use document::document_store;
pub use editing::rename;
pub use lang::config;
pub use navigation::call_hierarchy;
pub use navigation::definition;
pub use navigation::implementation;
pub use navigation::references;
