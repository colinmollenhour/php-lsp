//! Protocol-wired tests for Laravel `route('name')` go-to-definition and
//! completion, against a synthetic minimal Laravel project.

use super::*;

use expect_test::expect;

fn write_minimal_laravel_project(root: &std::path::Path) {
    std::fs::write(root.join("artisan"), "#!/usr/bin/env php\n").unwrap();
    std::fs::create_dir_all(root.join("routes")).unwrap();
    std::fs::write(
        root.join("routes").join("web.php"),
        "<?php\n\nRoute::get('/', HomeController::class)->name('home');\n\nRoute::group(['as' => 'admin.'], function () {\n    Route::get('/admin/dashboard', DashboardController::class)->name('dashboard');\n});\n",
    )
    .unwrap();
}

#[tokio::test]
async fn route_call_goto_definition_resolves_top_level_route() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    write_minimal_laravel_project(workspace.path());
    let php = "<?php\n$url = route('home');\n";
    std::fs::write(workspace.path().join("app.php"), php).unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("app.php", php).await;

    // Line 1 (0-based), character 16 = inside "home".
    let resp = s.definition("app.php", 1, 16).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["routes/web.php:2:46-2:50"].assert_eq(&out);
}

#[tokio::test]
async fn route_call_goto_definition_resolves_group_prefixed_route() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    write_minimal_laravel_project(workspace.path());
    let php = "<?php\n$url = route('admin.dashboard');\n";
    std::fs::write(workspace.path().join("app.php"), php).unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("app.php", php).await;

    // Line 1 (0-based), character 20 = inside "admin.dashboard".
    let resp = s.definition("app.php", 1, 20).await;
    let out = render_locations(&resp, &s.uri(""));
    expect!["routes/web.php:5:70-5:79"].assert_eq(&out);
}

#[tokio::test]
async fn route_call_completion_lists_route_names_by_prefix() {
    let workspace = tempfile::tempdir().expect("workspace tempdir");
    write_minimal_laravel_project(workspace.path());
    let php = "<?php\nroute('admin.\n";
    std::fs::write(workspace.path().join("app.php"), php).unwrap();

    let mut s = TestServer::with_root(workspace.path()).await;
    s.wait_for_index_ready().await;
    s.open("app.php", php).await;

    // Line 1 (0-based), character 13 = right after "admin.".
    let resp = s.completion("app.php", 1, 13).await;
    let out = render_completion(&resp);
    expect!["Reference   admin.dashboard"].assert_eq(&out);
}
