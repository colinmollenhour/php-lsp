use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwap;

use dashmap::{DashMap, DashSet};
use salsa::Setter;
use tower_lsp::lsp_types::{SemanticToken, Url};

use crate::db::mir_queries::{LspWorkspace, LspWsFile};
use crate::document::ast::ParsedDoc;
use crate::document::cache_registry::CacheRegistry;
use crate::index::file_index::FileIndex;
use crate::lang::autoload::Psr4Map;

pub struct DocumentStore {
    /// Per-file caches with unified eviction logic. See [`CacheRegistry`].
    caches: CacheRegistry,

    // ── Salsa-input storage ────────────────────────────────────────────────
    // Phase E4: `DocumentStore` is now a pure salsa-input wrapper. Open-file
    // state (live text, version token, parse-diagnostics cache) lives on
    // `Backend` in its `open_files` map; the set of files tracked by salsa
    // is exactly `source_files.keys()`.
    /// `Url -> LspWsFile` lookup on the shared mir db. Each `LspWsFile` pairs
    /// mir's `SourceFile` (the shared text input) with the optional warm-start
    /// `cached_index`. Created/updated through the `AnalysisSession`'s
    /// `with_db_mut` under the db write lock; reads run on cheap snapshot clones.
    lsp_ws_files: DashMap<Url, LspWsFile>,
    /// URIs that have been removed. Re-opening a deleted URI un-deletes it here
    /// and reuses the existing `LspWsFile` handle.
    deleted_uris: DashSet<Url>,
    /// Set to `true` when the set of tracked files changes (add or remove).
    /// `sync_workspace_files` skips the collect/sort/compare path when this
    /// is `false`, avoiding a lock acquisition on every LSP request.
    workspace_files_dirty: AtomicBool,
    /// `LspWorkspace` salsa input on the shared mir db: the project-file scoping
    /// set aggregated by `workspace_index`. Created lazily on first sync (the db
    /// is owned by the lazily-built `AnalysisSession`).
    lsp_workspace: Mutex<Option<LspWorkspace>>,
    /// Target PHP version (selects the `AnalysisSession`). Stored here since the
    /// converged db has no php-lsp `Workspace` input to carry it.
    php_version: Mutex<mir_analyzer::PhpVersion>,
    /// Shared PSR-4 namespace-to-path map. Shared with `Backend` via `Arc`
    /// so updates from `initialized` (when composer.json is loaded) are
    /// visible here without any additional wiring. `ArcSwap` makes reads
    /// lock-free — a poisoned guard can no longer crash a request handler.
    psr4: Arc<ArcSwap<Psr4Map>>,
    /// mir-analyzer's `AnalysisSession` — owns the workspace MirDb, runs
    /// Pass-2 analysis, and lazy-loads dependencies via PSR-4. Built lazily
    /// on first use; rebuilt when PHP version changes.
    analysis_session: Mutex<Option<(mir_analyzer::PhpVersion, Arc<mir_analyzer::AnalysisSession>)>>,
    /// Cache directory shared with the workspace file-index cache. When set,
    /// new `AnalysisSession`s are built with `with_cache_dir` so that stub
    /// parsing results survive server restarts.
    session_cache_dir: OnceLock<std::path::PathBuf>,
    /// URIs of autoload.files entries from composer.json. These define global
    /// helper functions (e.g. tap, class_uses_recursive in Laravel) that are
    /// not discoverable by namespace walk. Pre-ingested into the AnalysisSession
    /// before each file analysis so mir doesn't emit false UndefinedFunction.
    autoload_uris: std::sync::RwLock<Vec<Url>>,
    /// On-demand `FileIndex` store for vendor files loaded lazily via PSR-4
    /// navigation. Vendor is excluded from the eager workspace scan, so files
    /// ingested by `psr4_method_goto` are not in the salsa workspace_index;
    /// this map fills that gap for hierarchy traversal. Populated by
    /// `cache_vendor_index`; reads via `get_vendor_index`.
    vendor_index_cache: DashMap<Url, Arc<FileIndex>>,
    /// Set once the workspace scan's reference-index phase finishes, i.e. the
    /// `subtypes_of` map in `workspace_index` is complete. Visibility-derived
    /// scope narrowing that relies on the full subtype set (protected methods)
    /// only applies once this is `true`; before then it falls back to the full
    /// workspace scope so references are never under-reported.
    index_ready: AtomicBool,
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentStore {
    pub fn new() -> Self {
        DocumentStore {
            caches: CacheRegistry::new(),
            lsp_ws_files: DashMap::new(),
            deleted_uris: DashSet::new(),
            workspace_files_dirty: AtomicBool::new(true),
            lsp_workspace: Mutex::new(None),
            php_version: Mutex::new(mir_analyzer::PhpVersion::LATEST),
            psr4: Arc::new(ArcSwap::from_pointee(Psr4Map::empty())),
            analysis_session: Mutex::new(None),
            session_cache_dir: OnceLock::new(),
            autoload_uris: std::sync::RwLock::new(Vec::new()),
            vendor_index_cache: DashMap::new(),
            index_ready: AtomicBool::new(false),
        }
    }

    /// Mark the workspace reference index as fully built. Called by the scan
    /// when its final phase completes (alongside `$/php-lsp/indexReady`).
    pub fn mark_index_ready(&self) {
        self.index_ready.store(true, Ordering::Release);
    }

    /// Whether the workspace reference index has finished building.
    pub fn is_index_ready(&self) -> bool {
        self.index_ready.load(Ordering::Acquire)
    }

    /// Set the directory used to persist stub-parse and analysis results across
    /// server restarts.  Must be called before the first `analysis_session` use;
    /// subsequent calls are silently ignored (`OnceLock` semantics).
    pub fn set_session_cache_dir(&self, dir: std::path::PathBuf) {
        let _ = self.session_cache_dir.set(dir);
    }

    /// Register URIs discovered from composer.json `autoload.files` entries.
    /// These PHP files define global helper functions (e.g. `tap()` in Laravel)
    /// that are not class-resolvable via PSR-4. Clears `analysis_cache` so the
    /// next per-file analysis pre-ingests them into the AnalysisSession before
    /// running mir's FileAnalyzer.
    pub fn set_autoload_uris(&self, uris: Vec<Url>) {
        *self.autoload_uris.write().unwrap() = uris;
        self.caches.evict_analysis_all();
    }

    /// Get or build the `AnalysisSession` for the given PHP version, **without**
    /// loading PHP stubs. Stub loading is heavy (parses every built-in stub
    /// file) and is only needed for semantic analysis, not for parse / index /
    /// symbol-map queries. The parse path uses this; semantic callers use
    /// [`Self::analysis_session`], which loads stubs idempotently on top.
    fn get_or_build_session(
        &self,
        php_version: mir_analyzer::PhpVersion,
    ) -> Arc<mir_analyzer::AnalysisSession> {
        let mut guard = self.analysis_session.lock().unwrap();
        if let Some((cached_ver, session)) = guard.as_ref()
            && *cached_ver == php_version
        {
            return Arc::clone(session);
        }
        // Build a fresh session. Hand it the shared PSR-4 map so it can
        // lazy-resolve `UndefinedClass` candidates without us having to mirror
        // every vendor file upfront.
        let resolver: Arc<dyn mir_analyzer::ClassResolver> = self.psr4.load_full();
        let mut builder =
            mir_analyzer::AnalysisSession::new(php_version).with_class_resolver(resolver);
        if let Some(dir) = self.session_cache_dir.get() {
            builder = builder.with_cache_dir(dir);
        }
        let session = Arc::new(builder);
        *guard = Some((php_version, Arc::clone(&session)));
        session
    }

    /// The `AnalysisSession` for the given PHP version with PHP stubs loaded.
    /// Rebuilds when the version changes (e.g. user flipped config). The
    /// session owns the shared salsa db and AnalysisCache; lazy-loads vendor
    /// files via the shared PSR-4 map. `ensure_all_stubs` is idempotent — cheap
    /// after the first call on a given session.
    pub fn analysis_session(
        &self,
        php_version: mir_analyzer::PhpVersion,
    ) -> Arc<mir_analyzer::AnalysisSession> {
        let session = self.get_or_build_session(php_version);
        session.ensure_all_stubs();
        session
    }

    /// Current PHP version tracked by the workspace input.
    pub fn workspace_php_version(&self) -> mir_analyzer::PhpVersion {
        *self.php_version.lock().unwrap()
    }

    /// File URIs of all direct and transitive subclasses of `class_fqn`,
    /// resolved via mir's inheritance graph. Returns an empty vec when the mir
    /// session hasn't ingested the class yet (cold start, excluded paths).
    ///
    /// Used by `goto_implementation` and `subtypes` to scope their lookups to
    /// the correct files, fixing aliased `extends` and FQN-qualified forms that
    /// the raw-name `subtypes_of` map misses.
    pub fn class_subtype_urls(&self, class_fqn: &str) -> Vec<tower_lsp::lsp_types::Url> {
        let session = self.analysis_session(self.workspace_php_version());
        session
            .subtype_files(class_fqn)
            .into_iter()
            .filter_map(|p| tower_lsp::lsp_types::Url::parse(&p).ok())
            .collect()
    }

    /// Return the `Arc<ArcSwap<Psr4Map>>` so callers can share it.
    /// `Backend` clones this arc at construction time so writes
    /// (e.g. loading composer.json on `initialized`) are immediately visible
    /// to PSR-4 resolution during analysis without extra plumbing.
    pub fn psr4_arc(&self) -> Arc<ArcSwap<Psr4Map>> {
        Arc::clone(&self.psr4)
    }

    /// Durability for a file's salsa input: vendor files never change within a
    /// session, so salsa can skip re-validating their queries on user edits.
    fn input_durability(uri: &Url) -> salsa::Durability {
        if uri.as_str().contains("/vendor/") {
            salsa::Durability::HIGH
        } else {
            salsa::Durability::LOW
        }
    }

    /// The `LspWsFile` handle for `uri`, if it is mirrored and not deleted.
    fn lsp_ws_file(&self, uri: &Url) -> Option<LspWsFile> {
        if self.deleted_uris.contains(uri) {
            return None;
        }
        self.lsp_ws_files.get(uri).map(|e| *e)
    }

    /// Take a cheap snapshot of the shared mir db and run `f` on it, retrying
    /// once on `salsa::Cancelled` (raised when a concurrent writer bumps the
    /// revision). Mirrors [`Self::snapshot_query`] for the converged db.
    fn snapshot_mir_query<R>(&self, f: impl Fn(&mir_analyzer::db::MirDbStorage) -> R) -> R {
        use std::panic::AssertUnwindSafe;
        let session = self.get_or_build_session(self.workspace_php_version());
        // Each iteration's snapshot clone MUST drop before the next snapshot:
        // a concurrent writer's salsa `set` holds the mir write lock and waits
        // for outstanding db handles to drop, while the next `snapshot_db` needs
        // the read lock — keeping the clone alive across the retry deadlocks.
        loop {
            let db = session.snapshot_db();
            match salsa::Cancelled::catch(AssertUnwindSafe(|| f(&db))) {
                Ok(r) => return r,
                Err(_) => drop(db),
            }
        }
    }

    /// Mirror a file's current text into the salsa layer. Creates the
    /// `FileText` input on first sight, otherwise updates `text` on the
    /// existing input (bumping the salsa revision so downstream queries
    /// invalidate).
    pub fn mirror_text(&self, uri: &Url, text: &str) {
        // G2 fast path: compare against the lock-free text cache. When the
        // new text byte-matches what we already mirrored, skip the host
        // mutex entirely. Common during workspace scan + `did_open` for
        // unchanged files, where most threads would otherwise serialise on
        // `host.lock()` just to confirm a no-op.
        if let Some(cached) = self.caches.text_cache.get(uri)
            && **cached == *text
            && !self.deleted_uris.contains(uri)
            && self.lsp_ws_files.contains_key(uri)
        {
            return;
        }
        self.mirror_text_arc(uri, Arc::from(text))
    }

    /// Like [`mirror_text`] but takes an already-allocated `Arc<str>`.
    ///
    /// Callers that already hold an `Arc<str>` (e.g. `ingest_from_doc` reusing
    /// `ParsedDoc::source_arc()`) use this to avoid a second allocation and to
    /// ensure `text_cache` and `parsed_cache` hold the same Arc pointer —
    /// enabling `Arc::ptr_eq` validation in `get_parsed_cached`.
    pub fn mirror_text_arc(&self, uri: &Url, text_arc: Arc<str>) {
        let dur = Self::input_durability(uri);
        let path: Arc<str> = Arc::from(uri.as_str());
        let session = self.get_or_build_session(self.workspace_php_version());
        if let Some(wf) = self.lsp_ws_files.get(uri).map(|e| *e) {
            self.deleted_uris.remove(uri);
            // Fast path: byte-identical text already mirrored — skip the write
            // lock and the revision bump entirely.
            if let Some(cached) = self.caches.text_cache.get(uri)
                && **cached == *text_arc
            {
                return;
            }
            session.with_db_mut(|db| {
                let sf = wf.source(db);
                sf.set_text(db).with_durability(dur).to(text_arc.clone());
                // Any text change invalidates a previously-seeded cached index.
                // Only set when present to avoid a spurious second revision bump.
                if wf.cached_index(db).is_some() {
                    wf.set_cached_index(db).to(None);
                }
            });
            self.caches.text_cache.insert(uri.clone(), text_arc);
            // Evict only this file's analysis; cross-file invalidation is handled
            // lazily in `cached_analysis` via the declaration fingerprint.
            self.caches.evict_analysis(uri);
        } else {
            let wf = session.with_db_mut(|db| {
                let sf = db.upsert_source_file_with_durability(path, text_arc.clone(), dur);
                LspWsFile::new(db, sf, None)
            });
            self.lsp_ws_files.insert(uri.clone(), wf);
            self.caches.text_cache.insert(uri.clone(), text_arc);
            self.workspace_files_dirty.store(true, Ordering::Release);
        }
    }

    /// Return the `LspWsFile` handle for a URL, if active (not deleted).
    #[cfg(test)]
    pub fn source_file(&self, uri: &Url) -> Option<LspWsFile> {
        if self.deleted_uris.contains(uri) {
            return None;
        }
        self.lsp_ws_files.get(uri).map(|e| *e)
    }

    /// Phase K2: pre-seed a `FileIndex` loaded from the on-disk cache onto
    /// the `FileText` input for `uri`. The next `file_index` call for that
    /// file returns the cached index directly, skipping parse + extract.
    ///
    /// Must be called **before** any `file_index(db, sf)` call for this file —
    /// otherwise salsa has already memoized the fresh-parse result and setting
    /// `cached_index` now would only bump the revision without using the cache.
    /// In practice the workspace-scan path seeds immediately after `mirror_text`
    /// and before any query runs.
    ///
    /// Returns `false` when `uri` was not mirrored (caller should mirror
    /// first); returns `true` on success.
    pub fn seed_cached_index(&self, uri: &Url, index: Arc<FileIndex>) -> bool {
        let Some(wf) = self.lsp_ws_file(uri) else {
            return false;
        };
        let session = self.get_or_build_session(self.workspace_php_version());
        session.with_db_mut(|db| wf.set_cached_index(db).to(Some(index)));
        true
    }

    /// Evict the semantic-tokens cache for `uri`. Called by Backend when a
    /// file is closed; diff-based tokens computed against the old revision
    /// are no longer meaningful.
    pub fn evict_token_cache(&self, uri: &Url) {
        self.caches.evict_tokens(uri);
    }

    /// Return the `FileIndex` for `uri` by running `file_index` on a salsa
    /// snapshot.  Returns `None` when `uri` has not been mirrored.
    ///
    /// Test-only — production code uses the salsa query directly via
    /// `snapshot_query`.
    #[cfg(test)]
    pub fn source_files_len(&self) -> usize {
        self.lsp_ws_files.len()
    }

    #[cfg(test)]
    pub fn snapshot_query_file_index(
        &self,
        uri: &Url,
    ) -> Option<crate::index::file_index::FileIndex> {
        let wf = self.lsp_ws_file(uri)?;
        Some(
            self.snapshot_mir_query(move |db| {
                (*crate::db::mir_queries::file_index(db, wf).0).clone()
            }),
        )
    }

    /// Register a file in the salsa layer without marking it open.
    ///
    /// Salsa's `parsed_doc` query parses lazily on first read; diagnostics
    /// are populated by `did_open` when the editor actually opens the file.
    pub fn ingest(&self, uri: Url, text: &str) {
        self.mirror_text(&uri, text);
    }

    /// Index a file using an already-parsed `ParsedDoc`, avoiding a second parse.
    ///
    /// Prefer this over [`ingest`] when the caller already has a `ParsedDoc` (e.g.
    /// after running `DefinitionCollector` during workspace scan). Reuses the
    /// `Arc<str>` already owned by `doc` so that `text_cache` and `SourceFile::text`
    /// share the same pointer — enabling the `Arc::ptr_eq` fast path in
    /// `get_parsed_cached` on the first subsequent salsa query, without an extra
    /// `Arc::from(source)` allocation.
    pub fn ingest_from_doc(&self, uri: Url, doc: &ParsedDoc) {
        self.mirror_text_arc(&uri, doc.source_arc());
    }

    pub fn remove(&self, uri: &Url) {
        self.caches.evict(uri);
        // Mark the URI as deleted but keep the `source_files` entry so the
        // salsa `SourceFile` handle remains alive. Re-opening the file reuses
        // the same handle instead of calling `SourceFile::new()` again, which
        // would create a new orphaned salsa input on every delete-reopen cycle.
        self.deleted_uris.insert(uri.clone());
        self.workspace_files_dirty.store(true, Ordering::Release);
        // Sync workspace files so the deleted file is removed from the salsa
        // `Workspace::files` list and won't appear in workspace symbols etc.
        self.sync_workspace_files();
        // Also evict the file from the `AnalysisSession`'s internal state so
        // workspace symbol queries don't keep returning the deleted file's
        // declarations. Cheap when the session hasn't ingested this file.
        let guard = self.analysis_session.lock().unwrap();
        if let Some((_, session)) = guard.as_ref() {
            session.invalidate_file(uri.as_str());
        }
    }

    // ── Salsa-backed accessors ─────────────────────────────────────────────
    //
    // Reads run the memoized `parsed_doc` / `file_index` queries, parsing
    // only on first access per revision. These are the production accessors
    // used by every handler.

    /// Salsa-backed parsed document.
    ///
    /// Salsa-backed parsed document for any mirrored file (open or
    /// background-indexed). Returns `None` only when the file is not known
    /// to the store. Callers that want "only if open" should gate on
    /// `Backend::open_files` at the call site (see `Backend::get_doc`).
    pub fn get_doc_salsa(&self, uri: &Url) -> Option<Arc<ParsedDoc>> {
        self.get_parsed_cached(uri)
    }

    /// Salsa-backed compact symbol index.
    pub fn get_index_salsa(&self, uri: &Url) -> Option<Arc<FileIndex>> {
        let wf = self.lsp_ws_file(uri)?;
        Some(
            self.snapshot_mir_query(move |db| crate::db::mir_queries::file_index(db, wf).0.clone()),
        )
    }

    /// Salsa-backed pre-computed symbol map (name → Vec<SymbolEntry>).
    /// Memoized per revision: stable files serve from cache in O(1).
    pub fn get_symbol_map_salsa(
        &self,
        uri: &Url,
    ) -> Option<Arc<crate::types::symbol_map::SymbolMap>> {
        // Symbol map runs on the shared mir db, sharing its memoized `parsed_doc`.
        let wf = self.lsp_ws_file(uri)?;
        Some(self.snapshot_mir_query(move |db| {
            let sf = wf.source(db);
            crate::db::mir_queries::symbol_map(db, sf).0.clone()
        }))
    }

    /// Pre-computed symbol maps for every entry in `open_urls` except `uri`.
    pub fn other_symbol_maps(
        &self,
        uri: &Url,
        open_urls: &[Url],
    ) -> Vec<(Url, Arc<crate::types::symbol_map::SymbolMap>)> {
        open_urls
            .iter()
            .filter(|u| *u != uri)
            .filter_map(|u| self.get_symbol_map_salsa(u).map(|m| (u.clone(), m)))
            .collect()
    }

    /// G3: shared implementation for `get_doc_salsa`.
    /// Tries the `parsed_cache` (lock-free) first; validates via
    /// `Arc::ptr_eq` against the G2 `text_cache` so a concurrent writer
    /// that has already committed a new text input cannot be masked by a
    /// stale cache entry. On miss, captures the text Arc and ParsedDoc
    /// together inside a single `snapshot_query`, then publishes both.
    fn get_parsed_cached(&self, uri: &Url) -> Option<Arc<ParsedDoc>> {
        if let Some(current_text) = self.caches.text_cache.get(uri)
            && let Some(entry) = self.caches.parsed_cache.get(uri)
            && Arc::ptr_eq(&*current_text, &entry.0)
        {
            return Some(entry.1.clone());
        }

        // Parse runs on the shared mir db.
        let wf = self.lsp_ws_file(uri)?;
        self.caches.bump_parse_count();
        let (text, doc) = self.snapshot_mir_query(move |db| {
            let sf = wf.source(db);
            let text = sf.text(db);
            let doc = crate::db::mir_queries::parsed_doc(db, sf).0.clone();
            (text, doc)
        });
        self.caches.insert_parsed(uri.clone(), text, doc.clone());
        Some(doc)
    }

    /// Refresh `workspace.files` to mirror the current active file set.
    ///
    /// Skips all work when `workspace_files_dirty` is `false` (the common
    /// case after the workspace scan completes — file-set changes are rare).
    pub fn sync_workspace_files(&self) {
        // Atomically clear the flag.  If it was already false the file set
        // hasn't changed since the last sync; nothing to do.
        if !self.workspace_files_dirty.swap(false, Ordering::AcqRel) {
            return;
        }

        // Collect active (non-deleted) files, sorted by URI for stable ordering.
        let mut entries: Vec<(Arc<str>, LspWsFile)> = self
            .lsp_ws_files
            .iter()
            .filter(|e| !self.deleted_uris.contains(e.key()))
            .map(|e| (Arc::<str>::from(e.key().as_str()), *e.value()))
            .collect();
        entries.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        let files: Arc<[LspWsFile]> = entries.iter().map(|(_, wf)| *wf).collect();

        let session = self.get_or_build_session(self.workspace_php_version());
        let mut guard = self.lsp_workspace.lock().unwrap();
        session.with_db_mut(|db| match *guard {
            Some(ws) => {
                ws.set_files(db).to(files);
            }
            None => *guard = Some(LspWorkspace::new(db, files)),
        });
    }

    /// Mark the workspace file set as dirty so the next `sync_workspace_files`
    /// call re-runs the collect/sort/compare path.  Exposed for benchmarks that
    /// need to measure the dirty-path cost in isolation.
    pub fn mark_workspace_files_dirty(&self) {
        self.workspace_files_dirty.store(true, Ordering::Release);
    }

    /// Update the PHP version tracked by the workspace. Salsa will invalidate
    /// all `semantic_issues` queries so diagnostics are re-evaluated.
    /// Skips the setter when the version hasn't changed to avoid spurious
    /// query invalidation.
    pub fn set_php_version(&self, version: mir_analyzer::PhpVersion) {
        {
            let mut guard = self.php_version.lock().unwrap();
            if *guard == version {
                return;
            }
            *guard = version;
        }
        // Changing the version selects a different `AnalysisSession` (and thus a
        // different db). Drop the inputs created on the old session's db; the
        // workspace scan re-mirrors files onto the new session. In practice the
        // version is set once at init, before any file is mirrored.
        self.lsp_ws_files.clear();
        *self.lsp_workspace.lock().unwrap() = None;
        self.workspace_files_dirty.store(true, Ordering::Release);
        // Stale FileAnalysis from the old version would survive unchanged files.
        self.caches.evict_analysis_all();
    }

    /// Session-backed reference lookup scoped to `files`. Returns `(file, line,
    /// col)` locations for every occurrence of `symbol` originating in those
    /// files, computed from the memoized `analyze_file` query — no ingest, no
    /// shared-index mutation, so repeated lookups don't degrade across a
    /// session. `files` should be the text-pre-filtered candidate set.
    ///
    /// Returns LSP-style 0-based line/column.
    pub fn session_references_to(
        &self,
        symbol: &mir_analyzer::Name,
        files: &[Arc<str>],
    ) -> Vec<(Arc<str>, u32, u32, u32)> {
        let php_version = self.workspace_php_version();
        // Retry: concurrent db writes (background indexing) cancel snapshot
        // queries via resume_unwind; without the loop the panic propagates out
        // of the caller's spawn_blocking and the request silently returns empty.
        let raw = loop {
            let session = self.analysis_session(php_version);
            if let Ok(refs) = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                session.references_to_in_files(symbol, files)
            })) {
                break refs;
            }
        };
        raw.into_iter()
            .map(|(file, range)| {
                // mir uses 1-based lines; 0-based columns (since mir 0.42.0).
                let line = range.start.line.saturating_sub(1);
                let col_start = range.start.column;
                let col_end = range.end.column;
                (file, line, col_start, col_end)
            })
            .collect()
    }

    /// The complete set of files a `private` method's references can occur in,
    /// or `None` when the search must stay workspace-wide.
    ///
    /// `owner_fqn` is the enclosing class at the cursor; narrowing applies only
    /// when that class *directly declares* `method` (so its `MethodDef`
    /// visibility is authoritative — inherited/call-site lookups fall through to
    /// `None`). A reference to `Class::method` is recorded only where
    /// `analyze_file` resolves an expression to that key, and `analyze_file` is
    /// per-source-file, so a `private` member — resolvable only inside the
    /// declaring class body, which lives in one file — is fully scoped to the
    /// declaring file at any indexing stage.
    ///
    /// A `protected` member is reachable from the declaring class and its
    /// transitive subclasses (`$this->m()`, `parent::m()`, `self::`/`static::`),
    /// all of which live in the declaring file or a subtype file — never outside
    /// the hierarchy in valid PHP. Its scope is therefore the declaring file plus
    /// the transitive subtype files. The complete subtype set is only known once
    /// the workspace reference index finishes, so this narrowing applies only
    /// when [`Self::is_index_ready`]; before then it falls back to `None` (full
    /// workspace scope) so references are never under-reported.
    ///
    /// Classes that compose traits or mixins are excluded — those inline
    /// external resolution context, so a reference could surface in another file.
    pub fn method_reference_scope(&self, owner_fqn: &str, method: &str) -> Option<Vec<Url>> {
        use crate::index::file_index::{ClassKind, Visibility};

        let ws = self.get_workspace_index_salsa();
        let owner_fqn = owner_fqn.trim_start_matches('\\');
        let owner_short = crate::text::fqn_short_name(owner_fqn);

        let (decl_uri, decl_class) = ws
            .classes_by_name
            .get(owner_short)?
            .iter()
            .filter_map(|&r| ws.at(r))
            .find(|(_, cls)| cls.fqn.trim_start_matches('\\') == owner_fqn)?;

        // Trait/interface methods, and methods on classes that compose traits or
        // mixins, can be referenced from other files — keep the full scope.
        if !matches!(decl_class.kind, ClassKind::Class | ClassKind::Enum)
            || !decl_class.traits.is_empty()
            || !decl_class.mixins.is_empty()
        {
            return None;
        }

        let vis = decl_class
            .methods
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(method))?
            .visibility;

        match vis {
            Visibility::Private => Some(vec![decl_uri.clone()]),
            Visibility::Protected => {
                if !self.is_index_ready() {
                    return None;
                }
                // Subtype set comes from mir's resolved inheritance graph — it
                // matches subclasses by FQCN, so `extends \Ns\Base` and aliased
                // `use ... as` forms are all found. Falls back to full scope if
                // mir can't resolve the owner.
                let session = self.analysis_session(self.workspace_php_version());
                let mut files: std::collections::HashSet<Url> = session
                    .subtype_files(owner_fqn)
                    .into_iter()
                    .filter_map(|p| Url::parse(&p).ok())
                    .collect();
                files.insert(decl_uri.clone());
                Some(files.into_iter().collect())
            }
            Visibility::Public => None,
        }
    }

    /// Phase J: salsa-memoized aggregate workspace index.
    ///
    /// Returns the shared `Arc<WorkspaceIndexData>` with flat
    /// `(Url, Arc<FileIndex>)` list plus pre-built `classes_by_name` and
    /// `subtypes_of` reverse maps. Used by workspace_symbols,
    /// prepare_type_hierarchy, supertypes_of, subtypes_of, and
    /// find_implementations so they don't each rebuild the aggregate per
    /// request. Invalidates automatically when any file's `file_index`
    /// changes.
    pub fn get_workspace_index_salsa(&self) -> Arc<crate::db::workspace_index::WorkspaceIndexData> {
        self.sync_workspace_files();
        let ws = *self.lsp_workspace.lock().unwrap();
        let Some(ws) = ws else {
            return Arc::new(crate::db::workspace_index::WorkspaceIndexData {
                files: Vec::new(),
                classes_by_name: std::collections::HashMap::new(),
                subtypes_of: std::collections::HashMap::new(),
                decls_by_name: std::collections::HashMap::new(),
            });
        };
        self.snapshot_mir_query(move |db| crate::db::mir_queries::workspace_index(db, ws).0.clone())
    }

    /// Total number of real `ParsedDoc` parses served so far (cache misses).
    /// Surfaced via `$/php-lsp/debugStats` so tests can assert the references
    /// read path doesn't parse the whole workspace.
    pub fn parse_count(&self) -> u64 {
        self.caches.parse_count()
    }

    /// Return the raw source text for `uri` if it has been mirrored into the
    /// salsa workspace. Used by the references handler to pre-filter session
    /// results by checking whether a file mentions the owning class name.
    pub fn source_text(&self, uri: &Url) -> Option<Arc<str>> {
        self.caches.text_cache.get(uri).map(|e| Arc::clone(&e))
    }

    /// Cache the semantic tokens computed for a delta response.
    /// `result_id` is an opaque string (a hash of the token data) returned to the client.
    pub fn store_token_cache(&self, uri: &Url, result_id: String, tokens: Arc<Vec<SemanticToken>>) {
        self.caches.store_token(uri, result_id, tokens);
    }

    /// Return the cached tokens if `result_id` matches the stored one.
    pub fn get_token_cache(&self, uri: &Url, result_id: &str) -> Option<Arc<Vec<SemanticToken>>> {
        self.caches.get_token(uri, result_id)
    }

    /// Raw semantic issues for a file, computed via mir's session-based
    /// `FileAnalyzer`. The session lazy-loads dependencies via PSR-4 so the
    /// LSP no longer needs to mirror vendor up-front. Callers apply their
    /// own `DiagnosticsConfig` filter via
    /// [`crate::semantic_diagnostics::issues_to_diagnostics`].
    #[tracing::instrument(skip_all)]
    pub fn get_semantic_issues_salsa(&self, uri: &Url) -> Option<Arc<[mir_issues::Issue]>> {
        let analysis = self.cached_analysis(uri)?;
        let file: Arc<str> = Arc::from(uri.as_str());
        // Workspace-level class issues for this file (circular inheritance,
        // override violations, abstract-method gaps). These are session-wide
        // (a dependency edit changes them without changing this file's bytes),
        // so they are recomputed live rather than cached alongside the
        // per-file body analysis.
        let class_issues = {
            let _s = tracing::debug_span!("session.class_issues_for").entered();
            // Retry: concurrent db writes cancel snapshot queries via resume_unwind.
            loop {
                let session = self.analysis_session(self.workspace_php_version());
                if let Ok(issues) = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                    session.class_issues(std::slice::from_ref(&file))
                })) {
                    break issues;
                }
            }
        };
        let combined: Vec<mir_issues::Issue> = analysis
            .issues
            .iter()
            .cloned()
            .chain(class_issues)
            .filter(|i| !i.suppressed)
            .collect();
        Some(Arc::from(combined))
    }

    /// Run (or reuse) mir's per-file body analysis, retaining the full
    /// [`mir_analyzer::FileAnalysis`] — issues **and** resolved symbols — across
    /// requests. Diagnostics read `.issues`; position features call
    /// `.symbol_at(offset)` for the resolved type at a cursor.
    ///
    /// Cache hit when the entry's captured source `Arc` is pointer-equal to the
    /// file's current `doc.source_arc()`. A miss recomputes and overwrites, so
    /// the entry self-evicts on any content edit.
    /// Build (or reuse) the whole-doc completion [`crate::types::type_map::TypeMap`]
    /// for `uri`. Cache hit when the entry's captured source `Arc` is
    /// pointer-equal to `doc.source_arc()` and the PHPStorm-meta pointer is
    /// unchanged (meta lives behind `ArcSwap`, so its address is stable until
    /// `.phpstorm.meta.php` is reloaded). A miss rebuilds and overwrites, so
    /// the entry self-evicts on any content edit.
    pub fn cached_type_map(
        &self,
        uri: &Url,
        doc: &crate::document::ast::ParsedDoc,
        meta: Option<&crate::lang::phpstorm_meta::PhpStormMeta>,
    ) -> Arc<crate::types::type_map::TypeMap> {
        let source = doc.source_arc();
        let meta_key = meta.map_or(0usize, |m| std::ptr::from_ref(m) as usize);
        if let Some(entry) = self.caches.type_map_cache.get(uri)
            && Arc::ptr_eq(&entry.0, &source)
            && entry.1 == meta_key
        {
            return Arc::clone(&entry.2);
        }
        let map = Arc::new(crate::types::type_map::TypeMap::from_doc_with_meta(
            doc, meta,
        ));
        self.caches
            .type_map_cache
            .insert(uri.clone(), (source, meta_key, Arc::clone(&map)));
        map
    }

    /// Cache-hit-only variant of [`Self::cached_analysis`]: returns the cached
    /// analysis when the entry is current for the file's text, never computes.
    /// Lets async handlers take the warm path synchronously and reserve
    /// `spawn_blocking` for the cold path (mir Pass 1 + Pass 2 can take
    /// hundreds of ms on large files).
    pub fn cached_analysis_if_fresh(&self, uri: &Url) -> Option<Arc<mir_analyzer::FileAnalysis>> {
        let doc = self.get_doc_salsa(uri)?;
        let source = doc.source_arc();
        let entry = self.caches.analysis_cache.get(uri)?;
        let cur_ver = self.caches.decl_version();
        (Arc::ptr_eq(&entry.0, &source) && entry.1 == cur_ver).then(|| Arc::clone(&entry.2))
    }

    #[tracing::instrument(skip_all)]
    pub fn cached_analysis(&self, uri: &Url) -> Option<Arc<mir_analyzer::FileAnalysis>> {
        // Need the parsed doc both for the analyzer and as the cache key.
        let doc = self.get_doc_salsa(uri)?;
        let source = doc.source_arc();

        let cur_ver = self.caches.decl_version();
        if let Some(entry) = self.caches.analysis_cache.get(uri)
            && Arc::ptr_eq(&entry.0, &source)
            && entry.1 == cur_ver
        {
            return Some(Arc::clone(&entry.2));
        }

        let php_version = self.workspace_php_version();
        let session = self.analysis_session(php_version);
        let file: Arc<str> = Arc::from(uri.as_str());

        let source_map = php_rs_parser::source_map::SourceMap::new(doc.source());
        let owned_program = if let Some(cached) = self.caches.owned_program_cache.get(uri)
            && Arc::ptr_eq(&cached.0, &source)
        {
            Arc::clone(&cached.1)
        } else {
            let prog = Arc::new(php_ast::owned::to_owned_program(doc.program()));
            self.caches
                .owned_program_cache
                .insert(uri.clone(), (Arc::clone(&source), Arc::clone(&prog)));
            prog
        };

        // autoload.files helpers (e.g. Laravel's tap()) must be ingested so mir sees their functions.
        let autoload_texts: Vec<(Arc<str>, Arc<str>)> = {
            let autoload_uris = self.autoload_uris.read().unwrap().clone();
            autoload_uris
                .iter()
                .filter_map(|auri| {
                    self.caches
                        .text_cache
                        .get(auri)
                        .map(|t| (Arc::from(auri.as_str()), Arc::clone(&*t)))
                })
                .collect()
        };
        // Bare same-namespace / use-imported class refs aren't resolved by mir's priority_index_for_ast; preload them.
        let class_fqns = crate::references::collect_referenced_class_fqns(&doc);

        // ingest_file/load_class/analyze take internal salsa snapshots; a concurrent db write cancels them via resume_unwind. Retry the idempotent sequence.
        let analysis = loop {
            let attempt = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
                session.ingest_file(file.clone(), source.clone());
                for (afile, atext) in &autoload_texts {
                    session.ingest_file(afile.clone(), atext.clone());
                }
                for fqcn in &class_fqns {
                    let _ = session.load_class(fqcn);
                }
                let analyzer = mir_analyzer::FileAnalyzer::new(&session);
                analyzer.analyze(file.clone(), doc.source(), &owned_program, &source_map)
            }));
            if let Ok(a) = attempt {
                break Arc::new(a);
            }
        };
        // Compare the new FileIndex against the stored fingerprint. If
        // declarations changed (or this is the first analysis), bump
        // `decl_version` so other files' cache entries become stale. Body-only
        // edits leave the counter unchanged, allowing sibling files to be
        // served from cache on the next request.
        let new_index = self.get_index_salsa(uri);
        let old_fp = self
            .caches
            .decl_fingerprints
            .get(uri)
            .map(|e| Arc::clone(&*e));
        let decl_changed = match (&old_fp, &new_index) {
            (Some(old), Some(new)) => **old != **new,
            (None, Some(_)) => true,
            _ => false,
        };
        if decl_changed {
            if let Some(idx) = new_index {
                self.caches.decl_fingerprints.insert(uri.clone(), idx);
            }
            self.caches.bump_decl_version();
        }
        let ver = self.caches.decl_version();
        self.caches
            .analysis_cache
            .insert(uri.clone(), (source, ver, Arc::clone(&analysis)));
        Some(analysis)
    }

    /// Returns `(uri, doc)` for files currently open in the editor.
    ///
    /// Resolve `open_urls` (from `Backend::open_urls()`) to parsed docs.
    /// Files not mirrored in the salsa layer are filtered out silently.
    pub fn docs_for(&self, open_urls: &[Url]) -> Vec<(Url, Arc<ParsedDoc>)> {
        open_urls
            .iter()
            .filter_map(|u| self.get_doc_salsa(u).map(|d| (u.clone(), d)))
            .collect()
    }

    /// `(primary, doc)` first, then every other open file's parsed doc.
    /// The `open_urls` slice should include `uri` — this helper filters it out.
    pub fn doc_with_others(
        &self,
        uri: &Url,
        doc: Arc<ParsedDoc>,
        open_urls: &[Url],
    ) -> Vec<(Url, Arc<ParsedDoc>)> {
        let mut result = vec![(uri.clone(), doc)];
        result.extend(self.other_docs(uri, open_urls));
        result
    }

    /// Parsed docs for every entry in `open_urls` except `uri`.
    pub fn other_docs(&self, uri: &Url, open_urls: &[Url]) -> Vec<(Url, Arc<ParsedDoc>)> {
        open_urls
            .iter()
            .filter(|u| *u != uri)
            .filter_map(|u| self.get_doc_salsa(u).map(|d| (u.clone(), d)))
            .collect()
    }

    /// Compact symbol index for every mirrored file.
    pub fn all_indexes(&self) -> Vec<(Url, Arc<FileIndex>)> {
        self.get_workspace_index_salsa().files.clone()
    }

    /// Store a lazily-loaded vendor `FileIndex` in the session cache.
    /// Only call this for files that are not part of the normal workspace scan
    /// (i.e. vendor files loaded on-demand by PSR-4 navigation).
    pub fn cache_vendor_index(&self, uri: Url, index: Arc<FileIndex>) {
        self.vendor_index_cache.insert(uri, index);
    }

    /// Retrieve a previously cached vendor `FileIndex`.
    pub fn get_vendor_index(&self, uri: &Url) -> Option<Arc<FileIndex>> {
        self.vendor_index_cache.get(uri).map(|e| Arc::clone(&*e))
    }

    /// Same as `all_indexes` but excludes `uri`.
    pub fn other_indexes(&self, uri: &Url) -> Vec<(Url, Arc<FileIndex>)> {
        self.get_workspace_index_salsa()
            .files
            .iter()
            .filter(|(u, _)| u != uri)
            .cloned()
            .collect()
    }

    /// Parsed documents for every mirrored file (open or background-indexed).
    /// Suitable for full-scan operations: find-references, rename,
    /// call_hierarchy, code_lens.
    pub fn all_docs_for_scan(&self) -> Vec<(Url, Arc<ParsedDoc>)> {
        let urls: Vec<Url> = self
            .lsp_ws_files
            .iter()
            .filter(|e| !self.deleted_uris.contains(e.key()))
            .map(|e| e.key().clone())
            .collect();
        urls.into_iter()
            .filter_map(|u| self.get_doc_salsa(&u).map(|d| (u, d)))
            .collect()
    }

    /// URLs of workspace files whose raw source text mentions `word` as a whole
    /// identifier — **no parsing**. Uses a precompiled `memmem` finder (the same
    /// needle is scanned across the whole workspace) plus a word-boundary check,
    /// so `save` does not match inside `saveAll`/`unsaved`; the scan runs in
    /// parallel over the file set. The references read path narrows this list by
    /// symbol visibility and hands it to mir's per-file `analyze_file`, so a
    /// method lookup never materializes a `ParsedDoc` for the workspace.
    ///
    /// Files whose text is not yet in `text_cache` are included conservatively
    /// (safe superset — never produces false negatives).
    pub fn candidate_urls_for(&self, word: &str) -> Vec<Url> {
        use rayon::prelude::*;
        let urls: Vec<Url> = self
            .lsp_ws_files
            .iter()
            .filter(|e| !self.deleted_uris.contains(e.key()))
            .map(|e| e.key().clone())
            .collect();
        let finder = memchr::memmem::Finder::new(word.as_bytes());
        let wlen = word.len();
        urls.into_par_iter()
            .filter(|u| match self.caches.text_cache.get(u) {
                Some(src) => mentions_identifier(src.as_bytes(), &finder, wlen),
                None => true,
            })
            .collect()
    }

    /// Parsed documents limited to files whose raw source text contains `word`.
    ///
    /// [`Self::candidate_urls_for`] + a salsa parse per survivor. Used by the
    /// AST-walker reference paths (constructor refs, non-method kinds) that
    /// genuinely need the parsed AST. The method reference path uses
    /// `candidate_urls_for` directly and avoids these parses entirely.
    ///
    /// Files whose text is not yet in `text_cache` are included conservatively
    /// (safe superset — never produces false negatives).
    pub fn candidate_docs_for(&self, word: &str) -> Vec<(Url, Arc<ParsedDoc>)> {
        self.candidate_urls_for(word)
            .into_iter()
            .filter_map(|u| self.get_doc_salsa(&u).map(|d| (u, d)))
            .collect()
    }
}

/// Whether `hay` contains `finder`'s needle (length `wlen`) as a whole
/// identifier — i.e. not immediately preceded or followed by an ASCII
/// identifier byte (`[A-Za-z0-9_]`). Only ASCII bytes count as identifier
/// boundaries, so the check can over-accept near multibyte text but never
/// rejects a real whole-word match.
fn mentions_identifier(hay: &[u8], finder: &memchr::memmem::Finder, wlen: usize) -> bool {
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut from = 0;
    while let Some(rel) = finder.find(&hay[from..]) {
        let idx = from + rel;
        let before_ok = idx == 0 || !is_ident(hay[idx - 1]);
        let end = idx + wlen;
        let after_ok = end >= hay.len() || !is_ident(hay[end]);
        if before_ok && after_ok {
            return true;
        }
        from = idx + 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(path: &str) -> Url {
        Url::parse(&format!("file://{path}")).unwrap()
    }

    #[test]
    fn mentions_identifier_respects_word_boundaries() {
        let m = |hay: &str, word: &str| {
            let f = memchr::memmem::Finder::new(word.as_bytes());
            mentions_identifier(hay.as_bytes(), &f, word.len())
        };
        assert!(m("$this->save();", "save"));
        assert!(m("function save() {}", "save"));
        assert!(m("save", "save"));
        // substrings inside a larger identifier must not match
        assert!(!m("$this->saveAll();", "save"));
        assert!(!m("return $unsaved;", "save"));
        assert!(!m("class Saver {}", "save"));
        assert!(!m("no occurrence here", "save"));
    }

    /// Phase E4: open-file state lives on `Backend`, not `DocumentStore`.
    /// Tests that need to simulate "file is open" just mirror the text into
    /// the salsa input — the open/closed distinction is enforced by the
    /// caller (Backend) in production.
    fn open(store: &DocumentStore, u: Url, text: String) {
        store.mirror_text(&u, &text);
    }

    // Removed `salsa_codebase_aggregates_all_files`: the salsa-side codebase
    // aggregation was deleted with the mir 0.22 migration. Equivalent
    // behaviour is now covered by mir-analyzer's own session tests.

    #[test]
    fn index_registers_file_in_salsa() {
        let store = DocumentStore::new();
        store.ingest(uri("/lib.php"), "<?php\nfunction lib_fn() {}");
        let idx = store.get_index_salsa(&uri("/lib.php")).unwrap();
        assert_eq!(idx.functions.len(), 1);
        assert_eq!(idx.functions[0].name, "lib_fn".into());
    }

    #[test]
    fn remove_hides_file_from_index() {
        let store = DocumentStore::new();
        let u = uri("/lib.php");
        store.ingest(u.clone(), "<?php");
        store.remove(&u);
        assert!(store.get_index_salsa(&u).is_none());
    }

    #[test]
    fn remove_and_reopen_reuses_source_file_handle() {
        let store = DocumentStore::new();
        let u = uri("/lib.php");
        store.ingest(u.clone(), "<?php");
        let ft_before = store.source_file(&u).unwrap();
        store.remove(&u);
        assert!(
            store.source_file(&u).is_none(),
            "deleted file should be hidden"
        );
        store.mirror_text(&u, "<?php");
        let ft_after = store.source_file(&u).unwrap();
        assert!(
            ft_before == ft_after,
            "reopen must reuse the same FileText handle"
        );
    }

    #[test]
    fn delete_reopen_churn_does_not_amplify_salsa_inputs() {
        let store = DocumentStore::new();
        let uris: Vec<Url> = (0..20).map(|i| uri(&format!("/churn/f{i}.php"))).collect();
        for u in &uris {
            store.ingest(u.clone(), "<?php class A {}");
        }
        let count_before = store.source_files_len();
        for _ in 0..10 {
            for u in &uris {
                store.remove(u);
            }
            for u in &uris {
                store.ingest(u.clone(), "<?php class A {}");
            }
        }
        assert_eq!(
            store.source_files_len(),
            count_before,
            "delete-reopen cycles must not create new salsa inputs (L1-B regression guard)"
        );
    }

    #[test]
    fn all_indexes_includes_every_mirrored_file() {
        let store = DocumentStore::new();
        open(&store, uri("/a.php"), "<?php\nfunction a() {}".to_string());
        store.ingest(uri("/b.php"), "<?php\nfunction b() {}");
        assert_eq!(store.all_indexes().len(), 2);
    }

    #[test]
    fn other_indexes_excludes_current_uri() {
        let store = DocumentStore::new();
        open(&store, uri("/a.php"), "<?php\nfunction a() {}".to_string());
        open(&store, uri("/b.php"), "<?php\nfunction b() {}".to_string());
        assert_eq!(store.other_indexes(&uri("/a.php")).len(), 1);
    }

    #[test]
    fn other_docs_excludes_current_uri() {
        let store = DocumentStore::new();
        let ua = uri("/a.php");
        let ub = uri("/b.php");
        open(&store, ua.clone(), "<?php\nfunction a() {}".to_string());
        open(&store, ub.clone(), "<?php\nfunction b() {}".to_string());
        let open_urls = vec![ua.clone(), ub];
        assert_eq!(store.other_docs(&ua, &open_urls).len(), 1);
    }

    #[test]
    fn evict_token_cache_removes_entry() {
        let store = DocumentStore::new();
        let u = uri("/a.php");
        open(&store, u.clone(), "<?php".to_string());
        store.store_token_cache(&u, "id1".to_string(), Arc::new(vec![]));
        assert!(store.get_token_cache(&u, "id1").is_some());
        store.evict_token_cache(&u);
        assert!(store.get_token_cache(&u, "id1").is_none());
    }

    #[test]
    fn index_populates_file_index_with_symbols() {
        let store = DocumentStore::new();
        store.ingest(uri("/a.php"), "<?php\nfunction hello() {}");
        let idx = store.get_index_salsa(&uri("/a.php")).unwrap();
        assert_eq!(idx.functions.len(), 1);
        assert_eq!(idx.functions[0].name, "hello".into());
    }

    #[test]
    fn open_populates_file_index_with_symbols() {
        let store = DocumentStore::new();
        open(&store, uri("/a.php"), "<?php\nclass Foo {}".to_string());
        let idx = store.get_index_salsa(&uri("/a.php")).unwrap();
        assert_eq!(idx.classes.len(), 1);
        assert_eq!(idx.classes[0].name, "Foo".into());
    }

    // ── Mirror invariants ────────────────────────────────────────────────
    //
    // Every mutation path that changes file text must keep the salsa layer
    // consistent. These tests walk a set-edit-reopen cycle and assert that
    // the salsa-derived `FileIndex` reflects the latest text at each step.

    fn names_of(idx: &FileIndex) -> Vec<String> {
        let mut out: Vec<String> = idx.classes.iter().map(|c| c.name.to_string()).collect();
        out.extend(idx.functions.iter().map(|f| f.name.to_string()));
        out.sort();
        out
    }

    fn salsa_index_names(store: &DocumentStore, url: &Url) -> Vec<String> {
        store
            .snapshot_query_file_index(url)
            .map(|idx| names_of(&idx))
            .unwrap_or_default()
    }

    #[test]
    fn mirror_tracks_repeated_edits() {
        let store = DocumentStore::new();
        let u = uri("/mirror.php");

        open(&store, u.clone(), "<?php\nclass A {}".to_string());
        assert_eq!(salsa_index_names(&store, &u), vec!["A".to_string()]);

        open(
            &store,
            u.clone(),
            "<?php\nclass A {}\nclass B {}".to_string(),
        );
        assert_eq!(
            salsa_index_names(&store, &u),
            vec!["A".to_string(), "B".to_string()]
        );

        open(&store, u.clone(), "<?php\nfunction greet() {}".to_string());
        assert_eq!(salsa_index_names(&store, &u), vec!["greet".to_string()]);
    }

    #[test]
    fn mirror_tracks_ingest_and_ingest_from_doc() {
        let store = DocumentStore::new();

        // Background `index(url, text)` path.
        let u1 = uri("/bg1.php");
        store.ingest(u1.clone(), "<?php\nclass Bg1 {}");
        assert_eq!(salsa_index_names(&store, &u1), vec!["Bg1".to_string()]);

        // `ingest_from_doc(url, &doc)` path (workspace-scan Phase 2).
        let u2 = uri("/bg2.php");
        let doc = crate::analysis::diagnostics::parse_document_no_diags(
            "<?php\nclass Bg2 {}\nfunction f() {}",
        );
        store.ingest_from_doc(u2.clone(), &doc);
        assert_eq!(
            salsa_index_names(&store, &u2),
            vec!["Bg2".to_string(), "f".to_string()]
        );
    }

    /// G3: confirms the `parsed_cache` actually hits — two consecutive
    /// `get_doc_salsa` calls on unchanged text return the same `Arc`
    /// (pointer equality), and an edit forces a miss that produces a
    /// different `Arc`.
    /// parsed_cache must stay bounded — inserting more than
    /// `PARSED_CACHE_CAP` unique URLs must not cause unbounded growth.
    /// Eviction is probabilistic, so we only assert the bound, not which
    /// Seeding a cached index for a URL that was never mirrored is a no-op
    /// (returns `false`) — avoids silently allocating SourceFiles outside
    /// `mirror_text`'s control.
    #[test]
    fn seed_cached_index_noops_for_unknown_uri() {
        let store = DocumentStore::new();
        let u = uri("/never_mirrored.php");
        let index = Arc::new(crate::index::file_index::FileIndex::default());
        assert!(!store.seed_cached_index(&u, index));
    }

    /// entries survive.
    #[test]
    fn parsed_cache_stays_bounded_under_many_inserts() {
        let store = DocumentStore::new();
        use crate::document::cache_registry::PARSED_CACHE_CAP;
        let overflow = PARSED_CACHE_CAP + 100;
        for i in 0..overflow {
            let u = uri(&format!("/cap/file{i}.php"));
            store.ingest(u.clone(), "<?php\nclass A {}");
            // Force a parsed_cache insert via get_doc_salsa.
            let _ = store.get_doc_salsa(&u);
        }
        assert!(
            store.caches.parsed_cache.len() <= PARSED_CACHE_CAP,
            "parsed_cache grew to {} entries (cap {})",
            store.caches.parsed_cache.len(),
            PARSED_CACHE_CAP
        );
    }

    #[test]
    fn get_doc_salsa_cache_hits_across_calls() {
        let store = DocumentStore::new();
        let u = uri("/g3_cache.php");
        open(&store, u.clone(), "<?php\nclass G3 {}".to_string());

        let a = store.get_doc_salsa(&u).unwrap();
        let b = store.get_doc_salsa(&u).unwrap();
        assert!(
            Arc::ptr_eq(&a, &b),
            "parsed_cache hit should yield the same Arc across calls"
        );

        open(&store, u.clone(), "<?php\nclass G3b {}".to_string());
        let c = store.get_doc_salsa(&u).unwrap();
        assert!(
            !Arc::ptr_eq(&a, &c),
            "edit should invalidate the parsed_cache entry"
        );
    }

    #[test]
    fn get_doc_salsa_returns_some_for_mirrored_files() {
        // Phase E4: `get_doc_salsa` no longer gates on open-state. The
        // open/closed distinction now lives on `Backend::get_doc`.
        let store = DocumentStore::new();
        let u = uri("/e4_doc.php");
        store.ingest(u.clone(), "<?php\nclass P {}");
        assert!(store.get_doc_salsa(&u).is_some());
    }

    #[test]
    fn get_salsa_accessors_return_none_for_unknown_uri() {
        let store = DocumentStore::new();
        let u = uri("/never-seen.php");
        assert!(store.get_doc_salsa(&u).is_none());
        assert!(store.get_index_salsa(&u).is_none());
    }

    /// Phase E1: concurrent readers and writers must not deadlock, panic, or
    /// return stale data. Writers briefly bump inputs while readers are
    /// running on cloned snapshots; any `salsa::Cancelled` raised on the
    /// reader side must be caught and retried by `snapshot_query`.
    ///
    /// The salsa surface (`get_doc_salsa`, `get_index_salsa`) is protected by
    /// `snapshot_query`'s last-resort host-lock fallback.
    #[test]
    fn concurrent_reads_and_writes_do_not_panic() {
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant};

        let store = Arc::new(DocumentStore::new());
        let urls: Vec<Url> = (0..8).map(|i| uri(&format!("/f{i}.php"))).collect();
        for (i, u) in urls.iter().enumerate() {
            open(&store, u.clone(), format!("<?php\nclass C{i} {{}}"));
        }

        let deadline = Instant::now() + Duration::from_millis(400);
        let mut handles = Vec::new();

        // Writer thread: keep bumping every file's text.
        {
            let store = Arc::clone(&store);
            let urls = urls.clone();
            handles.push(thread::spawn(move || {
                let mut rev = 0u32;
                while Instant::now() < deadline {
                    for u in &urls {
                        let text = format!("<?php\nclass C{{}}\n// rev {rev}");
                        store.mirror_text(u, &text);
                    }
                    rev += 1;
                }
            }));
        }

        // Reader threads: hammer the salsa accessors.
        for _ in 0..4 {
            let store = Arc::clone(&store);
            let urls = urls.clone();
            handles.push(thread::spawn(move || {
                while Instant::now() < deadline {
                    for u in &urls {
                        let _ = store.get_doc_salsa(u);
                        let _ = store.get_index_salsa(u);
                    }
                    // Post mir 0.22: codebase + refs live in the session,
                    // not salsa. Concurrent-read smoke is limited to the
                    // remaining salsa surface (parsed_doc, file_index).
                }
            }));
        }

        for h in handles {
            h.join().expect("no panic under concurrent read/write");
        }
    }

    /// PSR-4 lazy-loading: `get_semantic_issues_salsa` must not emit
    /// `UndefinedClass` for a class that is PSR-4-resolvable on disk, even
    /// when the dependency file is not yet in `source_files`.
    #[test]
    fn psr4_lazy_load_suppresses_undefined_class() {
        let tmp = tempfile::tempdir().unwrap();

        // Write Entity.php to disk (not mirrored into the store).
        std::fs::create_dir_all(tmp.path().join("src/Model")).unwrap();
        std::fs::write(
            tmp.path().join("src/Model/Entity.php"),
            "<?php\nnamespace App\\Model;\nclass Entity {}\n",
        )
        .unwrap();

        // Write composer.json so Psr4Map::load can build the map.
        std::fs::write(
            tmp.path().join("composer.json"),
            r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#,
        )
        .unwrap();

        let store = DocumentStore::new();

        // Inject a PSR-4 map pointing at the tmp dir.
        store
            .psr4
            .store(Arc::new(crate::lang::autoload::Psr4Map::load(tmp.path())));

        // Mirror the consuming file (Entity not yet in source_files).
        // Uses Entity as a parameter type hint — the analyzer resolves these
        // through use statements, so this exercises the full PSR-4 lazy-load path.
        let handler_url = Url::from_file_path(tmp.path().join("src/Service/Handler.php")).unwrap();
        store.mirror_text(
            &handler_url,
            "<?php\nnamespace App\\Service;\nuse App\\Model\\Entity;\nfunction handle(Entity $e): Entity { return $e; }\n",
        );

        let issues = store.get_semantic_issues_salsa(&handler_url).unwrap();
        let undef: Vec<_> = issues
            .iter()
            .filter(|i| matches!(i.kind, mir_issues::IssueKind::UndefinedClass { .. }))
            .collect();
        assert!(
            undef.is_empty(),
            "PSR-4 lazy-loading must prevent UndefinedClass for App\\Model\\Entity; got: {undef:?}"
        );
    }

    /// Issue #191 regression: workspace-wide scans (find-references, rename,
    /// call-hierarchy) must not re-parse closed/indexed files on repeated
    /// invocations. Once a file's `ParsedDoc` has been produced, subsequent
    /// `all_docs_for_scan()` calls must hit the cache and return the same
    /// `Arc<ParsedDoc>` (pointer equality), proving no re-parse occurred.
    ///
    /// The cache layers protecting this are:
    ///   1. `parsed_cache` (cap [`PARSED_CACHE_CAP`]) — read-through, validated
    ///      via `Arc::ptr_eq` on the text Arc.
    ///   2. salsa `parsed_doc` memo (`lru = 2048`) — second line of defense
    ///      when `parsed_cache` evicts.
    ///
    /// Together they keep every workspace-scan op O(N) memo lookups, never
    /// O(N) parses, for any workspace whose file count fits the cap.
    #[test]
    fn all_docs_for_scan_does_not_reparse_indexed_files() {
        let store = DocumentStore::new();
        const N: usize = 50;
        for i in 0..N {
            let u = uri(&format!("/scan/file{i}.php"));
            store.ingest(u, &format!("<?php\nclass C{i} {{}}\nfunction f{i}() {{}}"));
        }

        let first: Vec<_> = store.all_docs_for_scan();
        let second: Vec<_> = store.all_docs_for_scan();
        assert_eq!(first.len(), N);
        assert_eq!(second.len(), N);

        let by_url_first: std::collections::HashMap<Url, Arc<ParsedDoc>> =
            first.into_iter().collect();
        for (u, doc2) in second {
            let doc1 = by_url_first
                .get(&u)
                .expect("second scan returned a URL the first didn't");
            assert!(
                Arc::ptr_eq(doc1, &doc2),
                "{u} re-parsed across all_docs_for_scan calls — \
                 cache (parsed_cache + salsa parsed_doc memo) failed to hit"
            );
        }

        // Editing one file's text must invalidate just that file's entry,
        // not the rest. This locks in self-eviction via Arc::ptr_eq on text.
        let edited_url = uri("/scan/file0.php");
        let pre_edit = store.get_doc_salsa(&edited_url).unwrap();
        store.ingest(edited_url.clone(), "<?php\nclass C0Edited {}");
        let post_edit = store.get_doc_salsa(&edited_url).unwrap();
        assert!(
            !Arc::ptr_eq(&pre_edit, &post_edit),
            "edited file must produce a fresh ParsedDoc"
        );
        for i in 1..N {
            let u = uri(&format!("/scan/file{i}.php"));
            let original = by_url_first.get(&u).unwrap();
            let after = store.get_doc_salsa(&u).unwrap();
            assert!(
                Arc::ptr_eq(original, &after),
                "{u} should not have re-parsed because of an unrelated edit"
            );
        }
    }

    /// Incremental analysis cache: a body-only edit to file A (no declaration
    /// changes) must not bump `decl_version`, so file B's cached analysis
    /// survives. A declaration edit MUST bump the version so B's entry goes
    /// stale.
    #[test]
    fn body_only_edit_does_not_invalidate_sibling_analysis_cache() {
        let store = DocumentStore::new();
        let ua = uri("/ic_a.php");
        let ub = uri("/ic_b.php");

        // Analyze both files to establish their fingerprints.
        open(
            &store,
            ua.clone(),
            "<?php\nfunction a() { return 1; }".to_string(),
        );
        open(
            &store,
            ub.clone(),
            "<?php\nfunction b() { return 2; }".to_string(),
        );
        let _ = store.cached_analysis(&ua).unwrap();
        let analysis_b_first = store.cached_analysis(&ub).unwrap();
        let ver_after_warm = store.caches.decl_version();

        // Body-only edit to A: same function name, different body → FileIndex unchanged.
        store.mirror_text(&ua, "<?php\nfunction a() { return 999; }");
        let _ = store.cached_analysis(&ua);
        let ver_after_body_edit = store.caches.decl_version();
        assert_eq!(
            ver_after_warm, ver_after_body_edit,
            "body-only edit must not bump decl_version"
        );

        // B's cached entry should still be valid (ptr-eq source AND same version).
        let analysis_b_second = store.cached_analysis_if_fresh(&ub);
        assert!(
            analysis_b_second.is_some(),
            "B's analysis should hit cache after body-only edit to A"
        );
        assert!(
            Arc::ptr_eq(&analysis_b_first, &analysis_b_second.unwrap()),
            "B's analysis should be the identical Arc (no re-analysis)"
        );

        // Declaration edit to A: rename the function → FileIndex changes.
        store.mirror_text(&ua, "<?php\nfunction a_renamed() { return 999; }");
        let _ = store.cached_analysis(&ua);
        let ver_after_decl_edit = store.caches.decl_version();
        assert!(
            ver_after_decl_edit > ver_after_body_edit,
            "declaration edit must bump decl_version (was {ver_after_body_edit}, now {ver_after_decl_edit})"
        );

        // B's entry is now stale — cached_analysis_if_fresh must return None.
        let analysis_b_stale = store.cached_analysis_if_fresh(&ub);
        assert!(
            analysis_b_stale.is_none(),
            "B's analysis should be stale after A's declaration changed"
        );
    }

    /// snapshot_query must complete without panic when a concurrent writer
    /// races the snapshot. The single-retry-then-lock logic should handle this
    /// correctly: the lock-held fallback guarantees progress.
    #[test]
    fn snapshot_query_survives_concurrent_writes() {
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant};

        let store = Arc::new(DocumentStore::new());
        let u = uri("/sq_test.php");
        open(
            &store,
            u.clone(),
            "<?php\nfunction f(): int { return 1; }".to_string(),
        );

        let deadline = Instant::now() + Duration::from_millis(200);
        let mut handles = Vec::new();

        // Writer: keep bumping the file text to trigger salsa::Cancelled.
        {
            let store = Arc::clone(&store);
            let u = u.clone();
            handles.push(thread::spawn(move || {
                let mut rev = 0u32;
                while Instant::now() < deadline {
                    store.mirror_text(&u, &format!("<?php\nfunction f(): int {{ return {rev}; }}"));
                    rev += 1;
                }
            }));
        }

        // Reader: hammer snapshot_query via get_doc_salsa.
        for _ in 0..4 {
            let store = Arc::clone(&store);
            let u = u.clone();
            handles.push(thread::spawn(move || {
                while Instant::now() < deadline {
                    let _ = store.get_doc_salsa(&u);
                    let _ = store.get_index_salsa(&u);
                }
            }));
        }

        for h in handles {
            h.join()
                .expect("no panic in snapshot_query under concurrent writes");
        }
    }

    /// When a sibling file's declaration changes (bumping decl_version), the
    /// owned_program_cache entry for the unchanged file B should be reused
    /// rather than deep-cloned again. We verify this via Arc pointer equality.
    #[test]
    fn owned_program_cache_reused_after_sibling_declaration_change() {
        let store = DocumentStore::new();
        let ua = uri("/prog_a.php");
        let ub = uri("/prog_b.php");

        open(
            &store,
            ua.clone(),
            "<?php\nfunction alpha(): void {}".to_string(),
        );
        open(
            &store,
            ub.clone(),
            "<?php\nfunction beta(): void {}".to_string(),
        );

        // Warm both analysis caches and populate owned_program_cache for both files.
        let _ = store.cached_analysis(&ua);
        let _ = store.cached_analysis(&ub);

        // Capture B's owned_program Arc before the sibling edit.
        let prog_b_first = store
            .caches
            .owned_program_cache
            .get(&ub)
            .map(|e| Arc::clone(&e.1))
            .expect("B's owned_program should be cached after first analysis");

        // Declaration change to A: bumps decl_version, invalidating all cached_analysis entries.
        store.mirror_text(&ua, "<?php\nfunction alpha_renamed(): void {}");
        // Re-analyze A to trigger the decl_version bump.
        let _ = store.cached_analysis(&ua);

        // Now re-analyze B. Its source is unchanged, so owned_program_cache must hit.
        let _ = store.cached_analysis(&ub);

        let prog_b_second = store
            .caches
            .owned_program_cache
            .get(&ub)
            .map(|e| Arc::clone(&e.1))
            .expect("B's owned_program should still be cached after sibling edit");

        assert!(
            Arc::ptr_eq(&prog_b_first, &prog_b_second),
            "B's owned_program Arc should be identical (cache hit) after sibling declaration change"
        );
    }
}
