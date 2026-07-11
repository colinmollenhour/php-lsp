//! Read-path cost guards for `textDocument/references`.
//!
//! The method reference path resolves usages from mir's per-file `analyze_file`
//! query and must never materialize a php-lsp `ParsedDoc` for the
//! text-matching candidate set — doing so reintroduced whole-workspace parsing
//! that grew with project size. These tests drive the real LSP request against
//! a background-scanned workspace and assert, via `$/php-lsp/debugStats`, that
//! the request parses (next to) nothing regardless of how many candidate files
//! mention the symbol's name.
//!
//! The `references_stress` fixture is one declaring class (`Target`) plus 30
//! unrelated classes that each textually contain `compute` and `process`, so
//! the text pre-filter admits every file as a candidate.

use super::*;

/// Line/utf-16-col of the first occurrence of `needle` in `text`.
fn pos_of(text: &str, needle: &str) -> (u32, u32) {
    for (line, content) in text.lines().enumerate() {
        if let Some(byte_col) = content.find(needle) {
            let col = content[..byte_col].encode_utf16().count() as u32;
            return (line as u32, col);
        }
    }
    panic!("`{needle}` not found in fixture text");
}

fn target_text() -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/references_stress/src/Target.php");
    std::fs::read_to_string(path).expect("read Target.php fixture")
}

fn location_uris(resp: &serde_json::Value) -> Vec<String> {
    resp["result"]
        .as_array()
        .expect("references returns a result array")
        .iter()
        .map(|loc| loc["uri"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn references_public_method_does_not_parse_candidate_files() {
    // `Target::process` is public, so the candidate set is NOT visibility-
    // narrowed — all 31 files mention `process`. The method path still answers
    // from `analyze_file` without parsing a single `ParsedDoc`, so the parse
    // count must not climb with the candidate count.
    let mut s = TestServer::with_fixture("references_stress").await;
    s.wait_for_index_ready().await;

    let text = target_text();
    s.open("src/Target.php", &text).await;
    let (line, col) = pos_of(&text, "process");

    let before = s.debug_stats_parses().await;
    let resp = s.references("src/Target.php", line, col, true).await;
    let after = s.debug_stats_parses().await;

    let uris = location_uris(&resp);
    assert!(
        uris.iter().all(|u| u.ends_with("Target.php")),
        "public method refs must resolve to the declaring class only, got {uris:?}"
    );
    assert!(
        !uris.is_empty(),
        "expected at least the declaration + the in-class call"
    );
    assert!(
        after - before <= 2,
        "references parsed {} candidate docs; the method path must not parse \
         the text-matching workspace (30 noise files mention `process`)",
        after - before
    );
}

#[tokio::test]
async fn references_protected_method_narrowed_to_hierarchy_stays_complete() {
    // Once the index is ready, `Base::boot` (protected) is narrowed to the
    // declaring file + its transitive subtype files. The narrowed search must
    // still find the in-class call and the subclass call, and must never reach
    // `Stranger::boot` (a same-named protected method on an unrelated class).
    let mut s = TestServer::with_fixture("references_protected").await;
    s.wait_for_index_ready().await;

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/references_protected/src/Base.php");
    let text = std::fs::read_to_string(path).expect("read Base.php fixture");
    s.open("src/Base.php", &text).await;
    let (line, col) = pos_of(&text, "boot");

    let resp = s.references("src/Base.php", line, col, true).await;
    let uris = location_uris(&resp);

    assert!(
        uris.iter().any(|u| u.ends_with("Base.php")),
        "must keep the declaring-file references, got {uris:?}"
    );
    assert!(
        uris.iter().any(|u| u.ends_with("Child.php")),
        "subclass call must be found — narrowing must include subtype files, got {uris:?}"
    );
    assert!(
        !uris.iter().any(|u| u.ends_with("Stranger.php")),
        "an unrelated class's same-named protected method must not be reported, got {uris:?}"
    );
    assert!(
        uris.iter().any(|u| u.ends_with("Grandchild.php")),
        "subclass extending via FQN (`\\App\\Base`) must still be found — narrowing \
         must not under-report on qualified extends, got {uris:?}"
    );
    assert!(
        uris.iter().any(|u| u.ends_with("Aliased.php")),
        "subclass extending via a `use ... as` alias must still be found — narrowing \
         must not under-report on aliased extends, got {uris:?}"
    );
}

#[tokio::test]
async fn edits_and_reads_never_lock_the_reference_index() {
    // The session opts out of mir's legacy RefIndex maintenance
    // (`without_reference_index`): references answer from the memoized
    // `analyze_file` path and diagnostics republish via the open-file set.
    // The lock counter must stay flat across the full edit → republish →
    // references cycle — any drift means index work crept back onto a hot path.
    let mut s = TestServer::with_fixture("references_stress").await;
    s.wait_for_index_ready().await;

    let text = target_text();
    s.open("src/Target.php", &text).await;
    let (line, col) = pos_of(&text, "process");

    let before = s.debug_stats_ref_index_locks().await;

    // Edit path: a change triggers analysis + the dependent republish sweep.
    s.change("src/Target.php", 2, &format!("{text}\n// edited\n"))
        .await;
    let _ = s
        .client()
        .drain_publish_diagnostics_uris(tokio::time::Duration::from_millis(300))
        .await;
    // Read path: references over the full candidate set.
    let resp = s.references("src/Target.php", line, col, true).await;
    assert!(
        !location_uris(&resp).is_empty(),
        "references must still answer while the index is unmaintained"
    );

    let after = s.debug_stats_ref_index_locks().await;
    assert_eq!(
        after - before,
        0,
        "RefIndex was locked {} time(s) on the edit/read path",
        after - before
    );
}

#[tokio::test]
async fn references_private_method_does_not_parse_candidate_files() {
    // `Target::compute` is private — narrowed to its declaring file. The
    // narrowing happens on the URL list *before* any parse, so neither the 30
    // noise files nor the scope filtering trigger a `ParsedDoc` parse.
    let mut s = TestServer::with_fixture("references_stress").await;
    s.wait_for_index_ready().await;

    let text = target_text();
    s.open("src/Target.php", &text).await;
    let (line, col) = pos_of(&text, "compute");

    let before = s.debug_stats_parses().await;
    let resp = s.references("src/Target.php", line, col, true).await;
    let after = s.debug_stats_parses().await;

    let uris = location_uris(&resp);
    assert!(
        uris.iter().all(|u| u.ends_with("Target.php")),
        "private method refs must stay in the declaring file, got {uris:?}"
    );
    assert!(
        after - before <= 2,
        "private references parsed {} candidate docs; narrowing must precede \
         (and elide) parsing",
        after - before
    );
}
