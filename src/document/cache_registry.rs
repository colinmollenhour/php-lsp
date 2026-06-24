use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use tower_lsp::lsp_types::{SemanticToken, Url};

use crate::document::ast::ParsedDoc;
use crate::index::file_index::FileIndex;

/// Upper bound on `parsed_cache` entries. Matched to the `lru = 2048` on
/// `parsed_doc` in `src/db/parse.rs` so the secondary Arc retention can't
/// pin more ASTs alive than salsa's memo already bounds.
pub(crate) const PARSED_CACHE_CAP: usize = 2048;

/// All per-file caches owned by `DocumentStore`, grouped so eviction logic
/// lives in one place. Adding a new cache only requires: add the field here,
/// then add `self.field.remove(uri)` to `evict()`.
pub(crate) struct CacheRegistry {
    /// Cached semantic tokens per document: (result_id, tokens).
    pub(crate) token_cache: DashMap<Url, (String, Arc<Vec<SemanticToken>>)>,
    /// G2: lock-free mirror of each file's last-set text for dedup in mirror_text.
    pub(crate) text_cache: DashMap<Url, Arc<str>>,
    /// G3: cross-revision read-through cache for parsed_doc.
    pub(crate) parsed_cache: DashMap<Url, (Arc<str>, Arc<ParsedDoc>)>,
    /// Per-file mir body analysis cache: (source_arc, decl_ver, analysis).
    pub(crate) analysis_cache: DashMap<Url, (Arc<str>, u64, Arc<mir_analyzer::FileAnalysis>)>,
    /// Monotonically increasing counter bumped on any declaration-level change.
    pub(crate) decl_version: AtomicU64,
    /// Count of real `ParsedDoc` parses served by `get_parsed_cached` (cache
    /// misses only). Read via `$/php-lsp/debugStats` to guard the references
    /// read path against re-introducing whole-workspace parsing.
    pub(crate) parse_count: AtomicU64,
    /// Last-seen FileIndex per URI, used to detect declaration changes.
    pub(crate) decl_fingerprints: DashMap<Url, Arc<FileIndex>>,
    /// Cross-request cache for the whole-doc TypeMap (completion).
    pub(crate) type_map_cache:
        DashMap<Url, (Arc<str>, usize, Arc<crate::types::type_map::TypeMap>)>,
    /// Owned-program cache: (source_arc, owned_program). Avoids repeating the
    /// deep arena clone in `cached_analysis` when `decl_version` bumps due to
    /// a sibling file's declaration change — the file's own source is unchanged,
    /// so the owned AST copy can be reused.
    pub(crate) owned_program_cache: DashMap<Url, (Arc<str>, Arc<php_ast::owned::Program>)>,
}

impl CacheRegistry {
    pub(crate) fn new() -> Self {
        CacheRegistry {
            token_cache: DashMap::new(),
            text_cache: DashMap::new(),
            parsed_cache: DashMap::new(),
            analysis_cache: DashMap::new(),
            decl_version: AtomicU64::new(0),
            parse_count: AtomicU64::new(0),
            decl_fingerprints: DashMap::new(),
            type_map_cache: DashMap::new(),
            owned_program_cache: DashMap::new(),
        }
    }

    /// Evict every per-file cache entry for `uri`. Call this from `DocumentStore::remove`.
    pub(crate) fn evict(&self, uri: &Url) {
        self.token_cache.remove(uri);
        self.text_cache.remove(uri);
        self.parsed_cache.remove(uri);
        self.analysis_cache.remove(uri);
        self.decl_fingerprints.remove(uri);
        self.type_map_cache.remove(uri);
        self.owned_program_cache.remove(uri);
    }

    /// Evict only the mir analysis cache for `uri`. Used on text change so the
    /// next request re-runs Pass 1 + Pass 2 with the new content.
    pub(crate) fn evict_analysis(&self, uri: &Url) {
        self.analysis_cache.remove(uri);
    }

    /// Clear the entire analysis cache. Used when the PHP version or
    /// autoload.files set changes, making all cached FileAnalysis stale.
    pub(crate) fn evict_analysis_all(&self) {
        self.analysis_cache.clear();
    }

    /// Evict only the semantic-tokens cache for `uri`. Used when a file is
    /// closed; delta tokens computed against the old revision are invalid.
    pub(crate) fn evict_tokens(&self, uri: &Url) {
        self.token_cache.remove(uri);
    }

    /// Store a fresh token set for delta requests.
    pub(crate) fn store_token(
        &self,
        uri: &Url,
        result_id: String,
        tokens: Arc<Vec<SemanticToken>>,
    ) {
        self.token_cache.insert(uri.clone(), (result_id, tokens));
    }

    /// Return the cached token set if `result_id` matches.
    pub(crate) fn get_token(&self, uri: &Url, result_id: &str) -> Option<Arc<Vec<SemanticToken>>> {
        self.token_cache
            .get(uri)
            .filter(|e| e.0.as_str() == result_id)
            .map(|e| Arc::clone(&e.1))
    }

    /// Publish a fresh `ParsedDoc` into `parsed_cache`, shedding roughly
    /// half the cache first when it has grown past [`PARSED_CACHE_CAP`].
    pub(crate) fn insert_parsed(&self, uri: Url, text: Arc<str>, doc: Arc<ParsedDoc>) {
        if self.parsed_cache.len() >= PARSED_CACHE_CAP {
            let drop_target = self.parsed_cache.len() / 2;
            let mut dropped = 0usize;
            self.parsed_cache.retain(|_, _| {
                if dropped < drop_target {
                    dropped += 1;
                    false
                } else {
                    true
                }
            });
        }
        self.parsed_cache.insert(uri, (text, doc));
    }

    pub(crate) fn decl_version(&self) -> u64 {
        self.decl_version.load(Ordering::Acquire)
    }

    pub(crate) fn bump_decl_version(&self) {
        self.decl_version.fetch_add(1, Ordering::Release);
    }

    pub(crate) fn bump_parse_count(&self) {
        self.parse_count.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn parse_count(&self) -> u64 {
        self.parse_count.load(Ordering::Relaxed)
    }
}
