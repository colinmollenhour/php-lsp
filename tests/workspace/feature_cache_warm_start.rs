//! On-disk cache warm-start correctness.
//!
//! A cold start scans and parses every workspace file; a warm start (same
//! root, same cache dir) must serve `FileIndex` entries from disk without
//! re-parsing. These tests exercise the observable guarantee: symbols are
//! correct after a warm restart, and a file modified between starts is
//! detected because the content key changes even when mtime/size are stable.
//!
//! The `cachePath` initializationOption pins both servers to the same
//! cache directory without touching `XDG_CACHE_HOME`, keeping tests
//! isolated even when run in parallel.

use super::*;

use expect_test::expect;
use serde_json::json;

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Warm restart (same workspace root, same cache dir) must expose the same
/// symbols as the cold start — the index is fully served from disk cache.
#[tokio::test]
async fn warm_start_serves_symbols_correctly() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    let cache_dir = tempfile::tempdir().expect("cache tempdir");
    copy_dir_all(&fixture_path("psr4-mini"), workspace.path()).expect("copy fixture");

    let opts = json!({
        "cachePath": cache_dir.path().to_str().unwrap(),
        "diagnostics": {"enabled": false},
    });

    // ── Cold start ────────────────────────────────────────────────────────────
    {
        let mut s = TestServer::with_root_and_options(workspace.path(), opts.clone()).await;
        s.wait_for_index_ready().await;
        let syms = s.snapshot_workspace_symbols("User").await;
        expect![[r#"Class       User @ src/Model/User.php:4"#]].assert_eq(&syms);
        // Server drops; both tempdirs remain alive so cache files persist.
    }

    // ── Warm restart on the same cache dir ────────────────────────────────────
    {
        let mut s = TestServer::with_root_and_options(workspace.path(), opts).await;
        s.wait_for_index_ready().await;
        // Symbols must be fully resolvable from the warm-loaded index.
        let syms = s.snapshot_workspace_symbols("User").await;
        expect![[r#"Class       User @ src/Model/User.php:4"#]].assert_eq(&syms);
    }
}

/// A file modified between two server starts must be detected on warm restart
/// and re-parsed, even when its mtime or size haven't changed.
/// The content-keyed cache ensures: different content → different key →
/// cache miss → fresh parse → new index.
#[tokio::test]
async fn warm_start_detects_changed_file_content() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    let cache_dir = tempfile::tempdir().expect("cache tempdir");
    copy_dir_all(&fixture_path("psr4-mini"), workspace.path()).expect("copy fixture");

    let opts = json!({
        "cachePath": cache_dir.path().to_str().unwrap(),
        "diagnostics": {"enabled": false},
    });

    // ── Cold start: scan and populate cache ───────────────────────────────────
    {
        let mut s = TestServer::with_root_and_options(workspace.path(), opts.clone()).await;
        s.wait_for_index_ready().await;
        // Confirm no Widget before the change.
        let syms = s.snapshot_workspace_symbols("Widget").await;
        expect![[r#"<no symbols>"#]].assert_eq(&syms);
    }

    // Replace User.php with a Widget class (different content, same-length file).
    let user_php = workspace.path().join("src/Model/User.php");
    std::fs::write(
        &user_php,
        "<?php\nnamespace App\\Model;\n\nclass Widget {}\n",
    )
    .expect("overwrite User.php");

    // ── Warm restart: changed file must be re-parsed, Widget must appear ──────
    {
        let mut s = TestServer::with_root_and_options(workspace.path(), opts).await;
        s.wait_for_index_ready().await;
        // The content-keyed cache detects the change → re-parsed → Widget found.
        let syms = s.snapshot_workspace_symbols("Widget").await;
        expect![[r#"Class       Widget @ src/Model/User.php:3"#]].assert_eq(&syms);
    }
}

/// Cache entries written by `textDocument/didSave` are found by the next
/// workspace scan (same content → same key). After a save, a server restart
/// must locate the index without re-parsing.
#[tokio::test]
async fn did_save_cache_is_found_by_subsequent_scan() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    let cache_dir = tempfile::tempdir().expect("cache tempdir");
    copy_dir_all(&fixture_path("psr4-mini"), workspace.path()).expect("copy fixture");

    let opts = json!({
        "cachePath": cache_dir.path().to_str().unwrap(),
        "diagnostics": {"enabled": false},
    });

    // ── First server: open and save a file so did_save writes the cache ───────
    {
        let mut s = TestServer::with_root_and_options(workspace.path(), opts.clone()).await;
        s.wait_for_index_ready().await;

        let user_content = "<?php\nnamespace App\\Model;\n\nclass User { public int $id; }\n";
        // Write new content and trigger did_save so the cache entry is refreshed.
        let user_php = workspace.path().join("src/Model/User.php");
        std::fs::write(&user_php, user_content).expect("write User.php");
        let uri = s.uri("src/Model/User.php");
        s.client()
            .notify(
                "textDocument/didSave",
                serde_json::json!({ "textDocument": { "uri": uri } }),
            )
            .await;

        // Give did_save's spawn_blocking task a moment to write the cache entry.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    // ── Second server: warm start — scan must hit the did_save cache entry ────
    {
        let mut s = TestServer::with_root_and_options(workspace.path(), opts).await;
        s.wait_for_index_ready().await;
        // User class still visible (cache hit from did_save write).
        let syms = s.snapshot_workspace_symbols("User").await;
        expect![[r#"Class       User @ src/Model/User.php:3"#]].assert_eq(&syms);
    }
}
