//! Protocol-wired tests for Laravel `view('a.b.c')` go-to-definition and
//! completion, against a synthetic minimal Laravel project.

use super::*;

use expect_test::expect;

fn write_minimal_laravel_project(root: &std::path::Path) {
    std::fs::write(root.join("artisan"), "#!/usr/bin/env php\n").unwrap();
    let views = root.join("resources").join("views");
    std::fs::create_dir_all(views.join("admin")).unwrap();
    std::fs::write(views.join("welcome.blade.php"), "<h1>Welcome</h1>\n").unwrap();
    std::fs::write(
        views.join("admin").join("dashboard.blade.php"),
        "<h1>Dashboard</h1>\n",
    )
    .unwrap();
}

#[tokio::test]
async fn view_call_goto_definition_resolves_top_level_view() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    write_minimal_laravel_project(workspace.path());
    let php = "<?php\nreturn view('welcome');\n";
    std::fs::write(workspace.path().join("app.php"), php).unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("app.php", php).await;

    // Line 1 (0-based), character 16 = inside "welcome".
    let resp = s.definition("app.php", 1, 16).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["resources/views/welcome.blade.php:0:0-0:0"].assert_eq(&out);
}

#[tokio::test]
async fn view_call_goto_definition_resolves_nested_view() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    write_minimal_laravel_project(workspace.path());
    let php = "<?php\nreturn view('admin.dashboard');\n";
    std::fs::write(workspace.path().join("app.php"), php).unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("app.php", php).await;

    // Line 1 (0-based), character 20 = inside "admin.dashboard".
    let resp = s.definition("app.php", 1, 20).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["resources/views/admin/dashboard.blade.php:0:0-0:0"].assert_eq(&out);
}

#[tokio::test]
async fn view_call_completion_lists_views_by_prefix() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    write_minimal_laravel_project(workspace.path());
    let php = "<?php\nview('admin.\n";
    std::fs::write(workspace.path().join("app.php"), php).unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("app.php", php).await;

    // Line 1 (0-based), character 12 = right after "admin.".
    let resp = s.completion("app.php", 1, 12).await;
    let out = render_completion(&resp);
    expect!["File        admin.dashboard"].assert_eq(&out);
}
