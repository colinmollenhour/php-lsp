use std::sync::Arc;

use tower_lsp::Client;
use tower_lsp::lsp_types::Url;
use tower_lsp::lsp_types::request::{
    CodeLensRefresh, InlayHintRefreshRequest, InlineValueRefreshRequest, SemanticTokensRefresh,
    WorkspaceDiagnosticRefresh,
};

use crate::diagnostics::parse_document_no_diags;
use crate::document_store::DocumentStore;
use crate::open_files::OpenFiles;

/// Ask all connected clients to re-request semantic tokens, code lenses, inlay hints,
/// and diagnostics. Called after bulk index operations so that previously-opened editors
/// immediately pick up the newly indexed symbol information.
pub(crate) async fn send_refresh_requests(client: &Client) {
    client.send_request::<SemanticTokensRefresh>(()).await.ok();
    client.send_request::<CodeLensRefresh>(()).await.ok();
    client
        .send_request::<InlayHintRefreshRequest>(())
        .await
        .ok();
    client
        .send_request::<WorkspaceDiagnosticRefresh>(())
        .await
        .ok();
    client
        .send_request::<InlineValueRefreshRequest>(())
        .await
        .ok();
}

/// Recursively scan `root` for `*.php` files and add them to the document store.
/// Skips hidden directories (names starting with `.`) and any path whose string
/// representation contains a segment matching one of the `exclude_paths` patterns.
/// Returns the number of files indexed.
///
/// Phase 1 — directory traversal: async, serial (I/O-bound; tokio handles it well).
/// Phase 2 — file reading + parsing: concurrent, bounded by available CPU cores.
///
/// Post-salsa: we only populate the DocumentStore here. The codebase is built
/// on demand by the salsa `codebase` query the first time a feature asks for
/// it — stubs + every indexed file's StubSlice, memoized thereafter.
#[tracing::instrument(
    skip(docs, open_files, cache, exclude_paths),
    fields(root = %root.display())
)]
pub(crate) async fn scan_workspace(
    root: std::path::PathBuf,
    docs: Arc<DocumentStore>,
    open_files: OpenFiles,
    cache: Option<crate::cache::WorkspaceCache>,
    exclude_paths: &[String],
    max_files: usize,
) -> usize {
    // Phase 1: collect PHP file paths via async directory walk.
    let mut php_files: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![root];

    'walk: while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            // Normalize to forward slashes so patterns like "src/Service/*"
            // match on Windows where paths use backslashes.
            let path_str = path.to_string_lossy().replace('\\', "/");
            // Check user-configured exclude patterns. Match as path components
            // to avoid false positives: "src/" should match "src/file" but not
            // "test_src/file" (where "src/" appears as a substring within a dir name).
            if exclude_paths.iter().any(|pat| {
                let p = pat.trim_end_matches('*').trim_end_matches('/');
                // Split path into components and check each against the pattern.
                // This ensures "src" matches "src/file" but not "test_src/file".
                path_str.split('/').any(|component| component == p)
                    || path_str.starts_with(&format!("{}/", p))
                    || path_str.contains(&format!("/{}/", p))
            }) {
                continue;
            }
            let file_type = match entry.file_type().await {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Skip hidden directories; vendor is indexed unless excluded above.
                if !name.starts_with('.') {
                    stack.push(path);
                }
            } else if file_type.is_file() && path.extension().is_some_and(|e| e == "php") {
                php_files.push(path);
                if php_files.len() >= max_files {
                    break 'walk;
                }
            }
        }
    }

    // Phase 2: read and parse files concurrently, bounded by available CPU cores.
    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let sem = Arc::new(tokio::sync::Semaphore::new(parallelism));
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

    for path in php_files {
        let permit = Arc::clone(&sem).acquire_owned().await.unwrap();
        let docs = Arc::clone(&docs);
        let open_files = open_files.clone();
        let cache = cache.clone();
        let count = Arc::clone(&count);
        set.spawn(async move {
            let _permit = permit;
            let Ok(text) = tokio::fs::read_to_string(&path).await else {
                return;
            };
            let Ok(uri) = Url::from_file_path(&path) else {
                return;
            };
            tokio::task::spawn_blocking(move || {
                // Skip files the editor has already opened — their buffer
                // is authoritative; scan must not overwrite their salsa
                // input with disk contents.
                if open_files.contains(&uri) {
                    return;
                }

                // Phase K2b read path: if the on-disk cache has a StubSlice
                // for this (uri, content) key, mirror the text and seed
                // the cached slice — `file_definitions` will return it
                // directly on the first query, skipping parse and
                // `DefinitionCollector` entirely. An edit later clears
                // the seeded slice via `mirror_text` (K2a).
                let cache_key = cache
                    .as_ref()
                    .map(|_| crate::cache::WorkspaceCache::key_for(uri.as_str(), &text));
                if let (Some(cache), Some(key)) = (cache.as_ref(), cache_key.as_ref())
                    && let Some(slice) = cache.read::<mir_codebase::storage::StubSlice>(key)
                {
                    docs.mirror_text(&uri, &text);
                    docs.seed_cached_slice(&uri, Arc::new(slice));
                    count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return;
                }

                // Cache miss: normal parse + mirror.
                let doc = parse_document_no_diags(&text);
                docs.index_from_doc(uri.clone(), &doc);
                count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                // Post-0.22: the analyzer-side definition cache is owned by
                // `AnalysisSession::with_cache_dir`. We no longer extract a
                // separate `StubSlice` for our own on-disk cache — that data
                // path is gone. Keep `cache` plumbing intact for future use.
                let _ = (cache.as_ref(), cache_key.as_ref());
            })
            .await
            .ok();
        });
    }

    while set.join_next().await.is_some() {}

    count.load(std::sync::atomic::Ordering::Relaxed)
}
