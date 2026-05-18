//! Cross-file reference tests: use imports, namespaces, disambiguation.

use super::*;

#[tokio::test]
async fn references_cross_file_via_use() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /src/Greeter.php
<?php
namespace App;
class Greeter {
    public function hel$0lo(): string { return 'hi'; }
    //              ^^^^^ def
}

//- /src/main.php
<?php
use App\Greeter;
$g = new Greeter();
$g->hello();
//  ^^^^^ ref
"#,
    )
    .await;
}

#[tokio::test]
async fn references_distinguishes_like_named_methods() {
    // Two classes both define `process()`. Refs on Mailer::process must NOT
    // pick up Queue::process calls.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
class Mailer {
    public function pro$0cess(): void {}
    //              ^^^^^^^ def
}
class Queue {
    public function process(): void {}
}
$m = new Mailer();
$m->process();
//  ^^^^^^^ ref
$q = new Queue();
$q->process();
"#,
    )
    .await;
}

#[tokio::test]
async fn references_distinguishes_cross_namespace_functions() {
    // Two functions `greet` in different namespaces. Refs on `App\greet` must
    // NOT pick up the call to `Domain\greet`.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /src/app.php
<?php
namespace App;
function gr$0eet(): void {}
//       ^^^^^ def
greet();
//^^^^^ ref

//- /src/domain.php
<?php
namespace Domain;
function greet(): void {}
greet();
"#,
    )
    .await;
}

#[tokio::test]
async fn references_distinguishes_cross_namespace_classes() {
    // Two classes `User` in different namespaces. Refs on `App\User` must NOT
    // include the `new Domain\User()` site.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /src/app.php
<?php
namespace App;
class Us$0er {}
//    ^^^^ def
$a = new User();
//       ^^^^ ref

//- /src/domain.php
<?php
namespace Domain;
class User {}
$b = new User();
"#,
    )
    .await;
}

/// Cross-namespace function filter: refs on `Alpha\process` must not include
/// calls to `Beta\process` in another file.
#[tokio::test]
async fn references_distinguishes_cross_namespace_function_calls() {
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /alpha.php
<?php
namespace Alpha;
function pro$0cess(): void {}
//       ^^^^^^^ def
process();
//^^^^^^^ ref

//- /beta.php
<?php
namespace Beta;
function process(): void {}
process();
"#,
    )
    .await;
}
