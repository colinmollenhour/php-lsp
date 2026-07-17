//! CRLF sources must produce the same reference postings as LF sources.
//!
//! Regression: on Windows CI (git autocrlf) the symfony-demo fixture is
//! checked out with CRLF endings, and blank lines inside AppFixtures.php's
//! indented nowdoc tripped a spurious "Invalid body indentation level" parse
//! error that silently dropped the file's `new Post()` reference posting.

use std::sync::Arc;

fn post_refs(line_ending: &str) -> Vec<String> {
    let root =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/symfony-demo");
    let read = |p: &str| -> String {
        std::fs::read_to_string(root.join(p))
            .unwrap()
            .replace('\n', line_ending)
    };
    let session = mir_analyzer::AnalysisSession::new(mir_analyzer::PhpVersion::LATEST);
    let files = ["src/Entity/Post.php", "src/DataFixtures/AppFixtures.php"];
    for f in files {
        session.ingest_file(Arc::from(f), Arc::from(read(f).as_str()));
    }
    let paths: Vec<Arc<str>> = files.iter().map(|f| Arc::from(*f)).collect();
    session
        .indexed_references_to(
            &mir_analyzer::Name::class("App\\Entity\\Post"),
            &paths,
            false,
            &|| false,
        )
        .unwrap()
        .into_iter()
        .map(|(f, r)| format!("{f}:{}:{}-{}", r.start.line, r.start.column, r.end.column))
        .collect()
}

#[test]
fn crlf_reference_postings_match_lf() {
    let lf = post_refs("\n");
    let crlf = post_refs("\r\n");
    assert!(
        lf.iter()
            .any(|l| l.contains("AppFixtures.php:74:24-28")),
        "expected the new Post() reference in AppFixtures.php, got: {lf:#?}"
    );
    assert_eq!(lf, crlf, "LF vs CRLF reference postings diverge");
}
