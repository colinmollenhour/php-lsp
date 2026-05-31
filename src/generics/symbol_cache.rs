//! Resolved-symbol cache: retains mir's per-expression `ResolvedSymbol` slice
//! for each open document, keyed by the text `Arc<str>` it was analysed from.
//!
//! mir's body analysis already resolves a generic-aware [`mir_analyzer::Type`]
//! for every expression it sees and returns them as
//! [`mir_analyzer::FileAnalysis::symbols`]. The diagnostics pass
//! ([`crate::document_store::DocumentStore::get_semantic_issues_salsa`]) is the
//! single place that runs `analyze()`, and historically it dropped `.symbols`,
//! keeping only `.issues`. This cache captures that slice as a **side effect** of
//! that existing pass — there is no extra `analyze()` call, and request handlers
//! never analyse on the read path.
//!
//! ## Staleness model
//! Entries are keyed `Url -> (Arc<str>, Arc<[ResolvedSymbol]>)`, mirroring the
//! `parsed_cache` shape. The stored `Arc<str>` is the exact source pointer the
//! symbols were resolved from. A reader validates freshness with
//! `Arc::ptr_eq(&cached_arc, current_text_arc)` (the same pattern as
//! [`crate::document_store::DocumentStore::get_parsed_cached`]): a pointer match
//! guarantees the cached symbols describe the current document text. On a
//! mismatch (the document was edited and a fresh `Arc<str>` allocated) or an
//! absent entry, the reader returns `None` and the caller degrades to the legacy
//! `String`/`TypeMap` path. The cache is **never** refreshed on read.
//!
//! The cache self-evicts on the next diagnostics pass (overwrite) and on
//! [`crate::document_store::DocumentStore::remove`].

use std::sync::Arc;

use dashmap::DashMap;
use mir_analyzer::ResolvedSymbol;
use mir_types::Type;
use tower_lsp::lsp_types::Url;

/// Upper bound on the number of per-URI entries retained in the resolved-symbol
/// cache. Matches `document_store::PARSED_CACHE_CAP` (2048): each entry pins an
/// `Arc<str>` of the full source plus an `Arc<[ResolvedSymbol]>` (one symbol per
/// expression, each holding a full `mir_types::Type`), so without a cap a long
/// session over a large workspace would grow monotonically and keep source text
/// alive. Insertion sheds roughly half the cache once this is exceeded — the same
/// probabilistic strategy as `insert_parsed_cache`.
const RESOLVED_SYMBOLS_CAP: usize = 2048;

/// Per-document cache of mir's resolved symbols, keyed by the source `Arc<str>`.
///
/// Stored as `(Arc<str>, Arc<[ResolvedSymbol]>)` so a read can validate the
/// symbols against the current document text via `Arc::ptr_eq` before trusting
/// them — exactly the staleness check used by `parsed_cache`.
#[derive(Default)]
pub struct ResolvedSymbolCache {
    entries: DashMap<Url, (Arc<str>, Arc<[ResolvedSymbol]>)>,
}

impl ResolvedSymbolCache {
    /// A new, empty cache.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Publish the symbols resolved for `uri` from the source text `text_arc`.
    ///
    /// `text_arc` must be the **same** `Arc<str>` pointer that was analysed
    /// (`doc.source_arc()`), so subsequent reads can validate via `Arc::ptr_eq`.
    /// Overwrites any previous entry, which is the cache's primary invalidation
    /// mechanism (a fresh analysis replaces stale symbols in place).
    ///
    /// Bounded at [`RESOLVED_SYMBOLS_CAP`]: when the cache has grown past the cap
    /// this sheds roughly half of it first (probabilistic — DashMap iteration
    /// order is arbitrary, not LRU), mirroring
    /// `DocumentStore::insert_parsed_cache`. A cache miss is cheap (the reader
    /// just degrades to the legacy `TypeMap`/`String` path), so dropping cold
    /// entries to bound memory is safe.
    pub fn insert(&self, uri: Url, text_arc: Arc<str>, symbols: Arc<[ResolvedSymbol]>) {
        if self.entries.len() >= RESOLVED_SYMBOLS_CAP {
            let drop_target = self.entries.len() / 2;
            let mut dropped = 0usize;
            self.entries.retain(|_, _| {
                if dropped < drop_target {
                    dropped += 1;
                    false
                } else {
                    true
                }
            });
        }
        self.entries.insert(uri, (text_arc, symbols));
    }

    /// Drop the cached symbols for `uri` (called from `DocumentStore::remove`).
    pub fn remove(&self, uri: &Url) {
        self.entries.remove(uri);
    }

    /// Resolve the innermost symbol type at `byte_off`, but only if the cached
    /// symbols were resolved from the exact `text_arc` the caller holds.
    ///
    /// Returns `None` when the entry is absent or stale (`Arc::ptr_eq` mismatch),
    /// or when no recorded symbol span contains `byte_off`. Never analyses, never
    /// blocks, and never refreshes the cache.
    ///
    /// The innermost-span selection replicates mir's
    /// [`mir_analyzer::FileAnalysis::symbol_at`]: among symbols whose span
    /// contains the offset, pick the one with the smallest span width.
    pub fn type_at(&self, uri: &Url, text_arc: &Arc<str>, byte_off: u32) -> Option<Type> {
        let entry = self.entries.get(uri)?;
        let (cached_arc, symbols) = entry.value();
        if !Arc::ptr_eq(cached_arc, text_arc) {
            return None;
        }
        symbols
            .iter()
            .filter(|s| s.span.start <= byte_off && byte_off < s.span.end)
            .min_by_key(|s| s.span.end - s.span.start)
            .map(|s| s.resolved_type.clone())
    }

    /// Whether a (fresh-or-stale) entry exists for `uri`. Test/diagnostic aid.
    #[cfg(test)]
    pub fn contains(&self, uri: &Url) -> bool {
        self.entries.contains_key(uri)
    }
}
