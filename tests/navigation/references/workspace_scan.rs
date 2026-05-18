//! Workspace scan, fast-path, and parallelism tests: cross-file indexing, consistency.

use super::*;

#[tokio::test]
async fn references_fast_path_final_class_cross_file() {
    let dir = tempfile::tempdir().unwrap();

    std::fs::write(
        dir.path().join("class.php"),
        "<?php\nfinal class Order {\n    public function submit(): void {}\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("caller.php"),
        "<?php\n$order = new Order();\n$order->submit();\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("ignored.php"),
        "<?php\n$unknown->submit();\n",
    )
    .unwrap();

    let mut server = TestServer::with_root(dir.path()).await;
    server.wait_for_index_ready().await;

    let caller_uri = server.uri("caller.php");
    let ignored_uri = server.uri("ignored.php");

    server
        .open(
            "class.php",
            "<?php\nfinal class Order {\n    public function submit(): void {}\n}\n",
        )
        .await;

    let resp = server.references("class.php", 2, 20, false).await;

    assert!(resp["error"].is_null(), "references error: {resp:?}");
    let uris: Vec<&str> = resp["result"]
        .as_array()
        .expect("array")
        .iter()
        .map(|l| l["uri"].as_str().unwrap())
        .collect();

    assert!(
        uris.iter().any(|u| *u == caller_uri.as_str()),
        "caller.php missing: {uris:?}"
    );
    assert!(
        !uris.iter().any(|u| *u == ignored_uri.as_str()),
        "ignored.php (untyped) must be excluded by fast path: {uris:?}"
    );
}

#[tokio::test]
async fn parallel_warm_finds_all_references_across_many_files() {
    let dir = tempfile::tempdir().unwrap();
    let caller_count = 15usize;
    std::fs::write(
        dir.path().join("def.php"),
        "<?php\nfunction target(): void {}",
    )
    .unwrap();
    for i in 0..caller_count {
        std::fs::write(
            dir.path().join(format!("caller_{i}.php")),
            "<?php\ntarget();",
        )
        .unwrap();
    }
    for i in 0..5usize {
        std::fs::write(
            dir.path().join(format!("other_{i}.php")),
            format!("<?php\nfunction other_{i}() {{}}"),
        )
        .unwrap();
    }

    let mut server = TestServer::with_root(dir.path()).await;
    server.wait_for_index_ready().await;
    server
        .open("def.php", "<?php\nfunction target(): void {}")
        .await;

    let resp = server.references("def.php", 1, 9, false).await;
    assert!(resp["error"].is_null(), "references error: {resp:?}");
    let locs = resp["result"].as_array().expect("expected array");
    assert_eq!(
        locs.len(),
        caller_count,
        "expected {caller_count} references, got {}: {locs:?}",
        locs.len()
    );
}

#[tokio::test]
async fn parallel_warm_gives_consistent_results_on_repeated_references_calls() {
    let mut server = TestServer::new().await;
    let opened = server
        .open_fixture(
            r#"//- /a.php
<?php
function fo$0o(): void {}

//- /b.php
<?php
foo();

//- /c.php
<?php
foo(); foo();
"#,
        )
        .await;
    let c = opened.cursor();

    let resp1 = server.references(&c.path, c.line, c.character, false).await;
    let resp2 = server.references(&c.path, c.line, c.character, false).await;

    let locs1 = resp1["result"].as_array().expect("array");
    let locs2 = resp2["result"].as_array().expect("array");
    assert_eq!(
        locs1.len(),
        3,
        "expected 3 references (1 from b.php, 2 from c.php): {locs1:?}"
    );
    assert_eq!(
        locs1.len(),
        locs2.len(),
        "repeated references calls returned different counts"
    );
}
