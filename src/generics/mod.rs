//! PHP generics support: bridges mir's generic-aware type system to the LSP.
//!
//! This module hosts the LSP-side consumption of mir 0.30's PHPStan-style
//! generics. It does **not** reimplement generic inference — mir already does
//! that and exposes the results. Our job is rendering, lookup and one targeted
//! completion substitution.
//!
//! Submodules:
//!   * [`render`] — `mir_types::Type` → import-aware SHORT-name display string
//!     ([`render::render_type`] / [`render::ImportCtx`]). Owned by WP1.
//!
//! Later work packages add `symbol_cache` / `resolved` here (the resolved-symbol
//! cache + `resolved_type_at` accessor) — see the work-packages plan.

pub mod render;
pub mod resolved;
pub mod symbol_cache;

// `render_type` / `ImportCtx` are the frozen WP1 contract consumed by docblock
// plumbing (and later WP2/WP3). `render_template_decl` lives in `render` and is
// reachable as `generics::render::render_template_decl` for WP2 hover-decl use.
pub use render::{ImportCtx, is_generic_relevant, render_type};

// WP2 contract consumed by hover/inlay/type-def (and WP3 completion): the
// resolved-symbol cache plus the `resolved_type_at` accessor that gates all
// generic-aware behaviour. `None` ⇒ caller falls back to the legacy path.
pub use resolved::resolved_type_at;
pub use symbol_cache::ResolvedSymbolCache;
