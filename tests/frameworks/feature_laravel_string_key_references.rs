//! Protocol-wired tests for find-references starting from a Laravel
//! string-key *definition* site (`.env` entry, `config/*.php` array key,
//! route `->name(...)`) — the reverse direction of go-to-definition, which
//! starts from a call site.

use super::*;

use expect_test::expect;

#[tokio::test]
async fn references_from_env_declaration_finds_every_call_site() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    std::fs::write(workspace.path().join("artisan"), "#!/usr/bin/env php\n").unwrap();
    std::fs::write(workspace.path().join(".env"), "APP_NAME=Test\n").unwrap();
    std::fs::write(
        workspace.path().join("a.php"),
        "<?php\n$x = env('APP_NAME');\n",
    )
    .unwrap();
    std::fs::write(
        workspace.path().join("b.php"),
        "<?php\n$y = env('APP_NAME');\n$z = env('OTHER');\n",
    )
    .unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open(".env", "APP_NAME=Test\n").await;

    // Line 0, character 2 = inside "APP_NAME" in the .env file.
    let resp = s.references(".env", 0, 2, false).await;
    let out = render_locations(&resp, &s.uri(""));
    expect![[r#"
        a.php:1:10-1:18
        b.php:1:10-1:18"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn references_from_env_declaration_includes_declaration_when_requested() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    std::fs::write(workspace.path().join("artisan"), "#!/usr/bin/env php\n").unwrap();
    std::fs::write(workspace.path().join(".env"), "APP_NAME=Test\n").unwrap();
    std::fs::write(
        workspace.path().join("a.php"),
        "<?php\n$x = env('APP_NAME');\n",
    )
    .unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open(".env", "APP_NAME=Test\n").await;

    let resp = s.references(".env", 0, 2, true).await;
    let out = render_locations(&resp, &s.uri(""));
    expect![[r#"
        .env:0:0-0:8
        a.php:1:10-1:18"#]]
    .assert_eq(&out);
}

#[tokio::test]
async fn references_from_config_key_declaration_finds_call_sites() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    std::fs::write(workspace.path().join("artisan"), "#!/usr/bin/env php\n").unwrap();
    std::fs::create_dir_all(workspace.path().join("config")).unwrap();
    let config_php = "<?php\nreturn [\n    'name' => 'Test',\n];\n";
    std::fs::write(workspace.path().join("config").join("app.php"), config_php).unwrap();
    std::fs::write(
        workspace.path().join("a.php"),
        "<?php\n$x = config('app.name');\n",
    )
    .unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("config/app.php", config_php).await;

    // Line 2 (0-based), character 6 = inside "name".
    let resp = s.references("config/app.php", 2, 6, false).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["a.php:1:13-1:21"].assert_eq(&out);
}

#[tokio::test]
async fn references_from_route_name_declaration_finds_call_sites() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    std::fs::write(workspace.path().join("artisan"), "#!/usr/bin/env php\n").unwrap();
    std::fs::create_dir_all(workspace.path().join("routes")).unwrap();
    let routes_php = "<?php\nRoute::get('/', Foo::class)->name('home');\n";
    std::fs::write(workspace.path().join("routes").join("web.php"), routes_php).unwrap();
    std::fs::write(
        workspace.path().join("a.php"),
        "<?php\n$x = route('home');\n",
    )
    .unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("routes/web.php", routes_php).await;

    // Line 1 (0-based), character 38 = inside "home".
    let resp = s.references("routes/web.php", 1, 38, false).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["a.php:1:12-1:16"].assert_eq(&out);
}

#[tokio::test]
async fn references_not_triggered_outside_laravel_project() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    // No `artisan`, no Laravel composer.json — plain PHP project.
    let php = "<?php\n$x = 'not_a_key';\n";
    std::fs::write(workspace.path().join("app.php"), php).unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("app.php", php).await;

    let resp = s.references("app.php", 1, 10, false).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["<none>"].assert_eq(&out);
}
