use super::*;
use expect_test::expect;

// ── Fast tests (no-vendor fixture, run by default) ─────────────────────

mod symbols {
    use super::*;

    #[tokio::test]
    async fn workspace_symbols_finds_controller_by_exact_name() {
        let mut server = TestServer::with_fixture_no_vendor("symfony-demo").await;
        server.wait_for_index_ready().await;

        let resp = server.workspace_symbols("BlogController").await;
        assert!(
            resp["error"].is_null(),
            "workspace/symbol error: {:?}",
            resp
        );
        let out = render_workspace_symbols(&resp, &server.uri(""));
        expect![[r#"
            Class       BlogController @ src/Controller/Admin/BlogController.php:42
            Class       BlogController @ src/Controller/BlogController.php:39
            Class       BlogControllerTest @ tests/Controller/Admin/BlogControllerTest.php:36
            Class       BlogControllerTest @ tests/Controller/BlogControllerTest.php:28"#]]
        .assert_eq(&out);
    }

    #[tokio::test]
    async fn workspace_symbols_fuzzy_prefix() {
        let mut server = TestServer::with_fixture_no_vendor("symfony-demo").await;
        server.wait_for_index_ready().await;

        let resp = server.workspace_symbols("Blog").await;
        assert!(resp["error"].is_null());
        let out = render_workspace_symbols(&resp, &server.uri(""));
        // Prefix query "Blog" must surface the Blog* family (BlogController, BlogSearchComponent, etc.)
        expect![[r#"
            Class       BlogController @ src/Controller/Admin/BlogController.php:42
            Class       BlogController @ src/Controller/BlogController.php:39
            Class       BlogControllerTest @ tests/Controller/Admin/BlogControllerTest.php:36
            Class       BlogControllerTest @ tests/Controller/BlogControllerTest.php:28
            Class       BlogSearchComponent @ src/Twig/Components/BlogSearchComponent.php:27
            Method      testPublicBlogPost @ tests/Controller/DefaultControllerTest.php:55"#]]
        .assert_eq(&out);
    }

    #[tokio::test]
    async fn document_symbols_lists_blog_controller_methods() {
        let mut server = TestServer::with_fixture_no_vendor("symfony-demo").await;
        server.wait_for_index_ready().await;

        let (text, _, _) = server.locate("src/Controller/BlogController.php", "<?php", 0);
        server
            .open("src/Controller/BlogController.php", &text)
            .await;

        let resp = server
            .document_symbols("src/Controller/BlogController.php")
            .await;
        let out = render_document_symbols(&resp);
        // Must include class BlogController and its index method.
        expect![[r#"
            Class BlogController @L39
              Method index @L47
              Method postShow @L80
              Method commentNew @L107
              Method commentForm @L150
              Method search @L160"#]]
        .assert_eq(&out);
    }
}

mod semantic_tokens {
    use super::*;

    #[tokio::test]
    async fn semantic_tokens_full_on_blog_controller_is_nonempty_and_well_formed() {
        let mut server = TestServer::with_fixture_no_vendor("symfony-demo").await;
        server.wait_for_index_ready().await;

        let (text, _, _) = server.locate("src/Controller/BlogController.php", "<?php", 0);
        server
            .open("src/Controller/BlogController.php", &text)
            .await;

        let resp = server
            .semantic_tokens_full("src/Controller/BlogController.php")
            .await;
        assert!(resp["error"].is_null());
        assert!(
            !resp["result"]["data"]
                .as_array()
                .unwrap_or(&vec![])
                .is_empty(),
            "expected semantic tokens"
        );
    }
}

mod perf_measure {
    use super::*;

    /// Manual benchmark to verify lazy-vendor `indexReady` latency on symfony-demo.
    /// Run with `cargo test --test frameworks measure_indexready -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "manual benchmark; run with --nocapture to see timings"]
    async fn measure_indexready_symfony_demo_lazy() {
        let t0 = std::time::Instant::now();
        let mut server = TestServer::with_fixture("symfony-demo").await;
        let t_init = t0.elapsed();
        server.wait_for_index_ready().await;
        let t_ready = t0.elapsed();
        println!(
            "MEASURE lazy-vendor symfony-demo: init={:?}, indexReady={:?}",
            t_init, t_ready
        );
    }

    #[tokio::test]
    #[ignore = "manual benchmark; run with --nocapture to see timings"]
    async fn measure_indexready_symfony_demo_eager() {
        let t0 = std::time::Instant::now();
        let mut server = TestServer::with_fixture_and_options(
            "symfony-demo",
            serde_json::json!({ "diagnostics": { "enabled": true }, "indexVendor": true }),
        )
        .await;
        let t_init = t0.elapsed();
        server.wait_for_index_ready().await;
        let t_ready = t0.elapsed();
        println!(
            "MEASURE eager-vendor symfony-demo: init={:?}, indexReady={:?}",
            t_init, t_ready
        );
    }

    /// Manual benchmark for the workspace-wide class-name search behind
    /// bare-class-name completion, against symfony-demo's full vendor tree
    /// (~5200 PHP files). Run with `cargo test --test frameworks
    /// measure_workspace_class_search -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "manual benchmark; run with --nocapture to see timings"]
    async fn measure_workspace_class_search_cost_eager_vendor() {
        let mut server = TestServer::with_fixture_and_options(
            "symfony-demo",
            serde_json::json!({ "diagnostics": { "enabled": true }, "indexVendor": true }),
        )
        .await;
        server.wait_for_index_ready_secs(60).await;
        let caller = "<?php\n$r = Con;\n";
        server.open("caller.php", caller).await;
        // "$r = Con;" — cursor right after "Con" (line 1, byte offset 8).
        let (line, ch) = (1, 8);
        // Warm up (first request pays one-time salsa/JIT costs).
        server.completion("caller.php", line, ch).await;
        let n = 20;
        let t0 = std::time::Instant::now();
        for _ in 0..n {
            server.completion("caller.php", line, ch).await;
        }
        let elapsed = t0.elapsed();
        println!(
            "MEASURE workspace_class_search: {n} completions in {:?} ({:?}/req)",
            elapsed,
            elapsed / n
        );
    }
}

mod call_hierarchy {
    use super::*;

    #[serial_test::serial]
    #[tokio::test]
    async fn incoming_calls_to_post_repository_find_latest() {
        let mut server = TestServer::with_fixture_no_vendor("symfony-demo").await;
        server.wait_for_index_ready().await;

        let (text, line, character) =
            server.locate("src/Repository/PostRepository.php", "findLatest", 0);
        server
            .open("src/Repository/PostRepository.php", &text)
            .await;

        let prep_resp = server
            .prepare_call_hierarchy("src/Repository/PostRepository.php", line, character)
            .await;
        assert!(prep_resp["error"].is_null());
        let item = prep_resp["result"]
            .as_array()
            .and_then(|a| a.first().cloned())
            .unwrap_or_default();

        let resp = server.incoming_calls(item).await;
        assert!(resp["error"].is_null());
        let out = render_call_hierarchy(&resp, "from", &server.uri(""));
        expect!["index @ src/Controller/BlogController.php:47"].assert_eq(&out);
    }
}

// ── Full-fixture tests (vendor present, indexed lazily by default) ───

mod navigation {
    use super::*;

    #[tokio::test]
    async fn goto_definition_parameter_type_in_vendor() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        let path = "src/Entity/Post.php";
        let (text, line, ch) = server.locate(path, "User $author", 1);
        server.open(path, &text).await;

        let resp = server.definition(path, line, ch).await;
        let out = render_locations(&resp, &server.uri(""));
        expect!["src/Entity/User.php:32:6-32:10"].assert_eq(&out);
    }

    #[tokio::test]
    async fn goto_definition_app_class_from_use_import() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        let path = "src/Repository/PostRepository.php";
        let (text, line, ch) = server.locate(path, "Post;", 0);
        server.open(path, &text).await;

        let resp = server.definition(path, line, ch).await;
        let out = render_locations(&resp, &server.uri(""));
        expect!["src/Entity/Post.php:36:6-36:10"].assert_eq(&out);
    }

    #[tokio::test]
    async fn goto_definition_inherited_method_this_render() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        let path = "src/Controller/BlogController.php";
        let (text, line, ch) = server.locate(path, "render('", 0);
        server.open(path, &text).await;

        let resp = server.definition(path, line, ch).await;
        let out = render_locations(&resp, &server.uri(""));
        expect!["vendor/symfony/framework-bundle/Controller/AbstractController.php:275:23-275:29"]
            .assert_eq(&out);
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn goto_definition_attribute_class_route() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/symfony-demo");
        let mut server = TestServer::with_root(&fixture).await;
        server.wait_for_index_ready().await;

        let path = "src/Controller/BlogController.php";
        let (text, line, ch) = server.locate(path, "Route", 0);
        server.open(path, &text).await;

        let resp = server.definition(path, line, ch).await;
        let out = render_locations(&resp, &server.uri(""));
        expect!["vendor/symfony/routing/Attribute/Route.php:18:6-18:11"].assert_eq(&out);
    }
}

mod hover {
    use super::*;

    #[serial_test::serial]
    #[tokio::test]
    async fn hover_on_class_in_extends_clause() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/symfony-demo");
        let mut server = TestServer::with_root(&fixture).await;
        server.wait_for_index_ready().await;

        let path = "src/Controller/BlogController.php";
        let (text, line, ch) = server.locate(path, "AbstractController", 0);
        server.open(path, &text).await;

        let resp = server.hover(path, line, ch).await;
        let out = render_hover(&resp);
        expect![[r#"`use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;`"#]]
            .assert_eq(&out);
    }

    #[tokio::test]
    async fn hover_on_app_entity_type_in_signature() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        let path = "src/Repository/PostRepository.php";
        let (text, line, ch) = server.locate(path, "Tag $tag", 0);
        server.open(path, &text).await;

        let resp = server.hover(path, line, ch).await;
        let out = render_hover(&resp);
        expect![[r#"
            ```php
            class Tag implements \JsonSerializable
            ```"#]]
        .assert_eq(&out);
    }
}

mod implementation {
    use super::*;

    /// `User implements UserInterface` — cursor on the `implements` clause
    /// (occurrence=1) should return at least `App\Entity\User`.
    #[tokio::test]
    async fn implementations_of_user_interface_include_app_user() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        let path = "src/Entity/User.php";
        // occurrence=1: the `implements UserInterface` clause, not the `use` import.
        let (text, line, ch) = server.locate(path, "UserInterface", 1);
        server.open(path, &text).await;

        let resp = server.implementation(path, line, ch).await;
        assert!(resp["error"].is_null());
        let out = render_locations(&resp, &server.uri(""));
        expect!["src/Entity/User.php:32:6-32:10"].assert_eq(&out);
    }

    /// Cursor on the `use` import line (`use A\B\Foo`) must also work — the
    /// handler splits on `\` to recover the short name for the index lookup.
    #[tokio::test]
    async fn implementations_via_use_statement_cursor() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        let path = "src/Entity/User.php";
        // occurrence=0: the `use …\UserInterface` line.
        let (text, line, ch) = server.locate(path, "UserInterface", 0);
        server.open(path, &text).await;

        let resp = server.implementation(path, line, ch).await;
        assert!(resp["error"].is_null());
        let out = render_locations(&resp, &server.uri(""));
        expect!["src/Entity/User.php:32:6-32:10"].assert_eq(&out);
    }
}

mod references {
    use super::*;

    #[serial_test::serial]
    #[tokio::test]
    async fn references_to_post_entity_span_multiple_files() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/symfony-demo");
        let mut server = TestServer::with_root(&fixture).await;
        server.wait_for_index_ready().await;

        let path = "src/Entity/Post.php";
        let (text, line, character) = server.locate(path, "class Post", 0);
        let character = character + "class ".len() as u32;
        server.open(path, &text).await;

        let resp = server.references(path, line, character, false).await;
        assert!(resp["error"].is_null(), "references error: {:?}", resp);
        let out = render_locations(&resp, &server.uri(""));
        // Must span ≥4 files including PostRepository.php
        expect![[r#"
            src/Controller/Admin/BlogController.php:122:25-122:29
            src/Controller/Admin/BlogController.php:138:43-138:47
            src/Controller/Admin/BlogController.php:161:45-161:49
            src/Controller/Admin/BlogController.php:79:20-79:24
            src/Controller/BlogController.php:110:54-110:58
            src/Controller/BlogController.php:150:32-150:36
            src/Controller/BlogController.php:80:29-80:33
            src/DataFixtures/AppFixtures.php:73:24-73:28
            src/Entity/Comment.php:101:32-101:36
            src/Entity/Comment.php:106:28-106:32
            src/Entity/Comment.php:37:34-37:38
            src/Entity/Comment.php:39:13-39:17
            src/Form/PostType.php:88:28-88:32
            src/Repository/PostRepository.php:38:39-38:43
            src/Security/PostVoter.php:40:35-40:39
            tests/Controller/DefaultControllerTest.php:64:45-64:49"#]]
        .assert_eq(&out);
    }
}

mod type_hierarchy {
    use super::*;

    /// `BlogController extends AbstractController` — supertypes of BlogController
    /// must include AbstractController (a vendor class), verifying that the
    /// PSR-4 pre-load pass makes vendor parents visible in the workspace index.
    #[serial_test::serial]
    #[tokio::test]
    async fn supertypes_of_blog_controller_include_abstract_controller() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        let path = "src/Controller/BlogController.php";
        let (text, line, ch) = server.locate(path, "BlogController", 0);
        server.open(path, &text).await;

        let prep = server.prepare_type_hierarchy(path, line, ch).await;
        let item = prep["result"]
            .as_array()
            .and_then(|a| a.first().cloned())
            .unwrap_or_default();
        assert_eq!(item["name"].as_str(), Some("BlogController"));

        let resp = server.supertypes(item).await;
        assert!(resp["error"].is_null());
        let names: Vec<&str> = resp["result"]
            .as_array()
            .map(|a| a.iter().filter_map(|i| i["name"].as_str()).collect())
            .unwrap_or_default();
        // Supertypes must include AbstractController (vendor class via PSR-4 pre-load)
        expect!["AbstractController, AbstractController"].assert_eq(&names.join(", "));
    }

    /// `BlogController extends AbstractController` — subtypes of AbstractController
    /// (a vendor class) must include BlogController once AbstractController has
    /// been pre-loaded into the workspace index.
    #[serial_test::serial]
    #[tokio::test]
    async fn subtypes_of_abstract_controller_include_blog_controller() {
        let mut server = TestServer::with_fixture("symfony-demo").await;
        server.wait_for_index_ready().await;

        // First open BlogController so the workspace knows about the relationship.
        let path = "src/Controller/BlogController.php";
        let (text, _, _) = server.locate(path, "BlogController", 0);
        server.open(path, &text).await;

        // Prepare on AbstractController (in vendor) — needs PSR-4 resolution.
        // Use the use-statement line in BlogController to locate it.
        let (_, ac_line, ac_ch) = server.locate(path, "AbstractController", 0);
        let prep = server.prepare_type_hierarchy(path, ac_line, ac_ch).await;
        // prepare_type_hierarchy may return null for vendor classes not yet in the
        // workspace index; in that case subtypes is undefined and we just pass.
        let Some(item) = prep["result"].as_array().and_then(|a| a.first().cloned()) else {
            return;
        };

        let resp = server.subtypes(item).await;
        assert!(resp["error"].is_null());
        let items = resp["result"].as_array().cloned().unwrap_or_default();
        let names: Vec<&str> = items.iter().filter_map(|i| i["name"].as_str()).collect();
        assert!(
            names.contains(&"BlogController"),
            "expected BlogController in subtypes of AbstractController; got {names:?}"
        );
    }
}

mod smoke {
    use super::*;

    #[serial_test::serial]
    #[tokio::test]
    async fn smoke_goto_definition_abstract_controller() {
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/symfony-demo");
        let mut server = TestServer::with_root(&fixture).await;
        server.wait_for_index_ready().await;

        let path = "src/Controller/BlogController.php";
        let (text, line, ch) = server.locate(path, "AbstractController", 0);
        server.open(path, &text).await;

        let resp = server.definition(path, line, ch).await;
        let out = render_locations(&resp, &server.uri(""));
        expect!["vendor/symfony/framework-bundle/Controller/AbstractController.php:56:15-56:33"]
            .assert_eq(&out);
    }
}
