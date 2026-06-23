//! Visibility-scoped method references.
//!
//! `private`/`protected` method references are narrowed to the declaring class
//! file (and subtype files for `protected`) before analysis. These tests pin
//! that the narrowing stays *complete* — every real reference is still found,
//! and same-named methods on unrelated classes are never picked up.

use super::*;

#[tokio::test]
async fn references_private_method_scoped_to_declaring_file() {
    // `A::help` is private; its references live only in A's file. A same-named
    // private method on an unrelated class B in another file must be ignored.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /src/A.php
<?php
namespace App;
class A {
    private function he$0lp(): void {}
    //               ^^^^ def
    public function run(): void {
        $this->help();
        //     ^^^^ ref
    }
}

//- /src/B.php
<?php
namespace App;
class B {
    private function help(): void {}
    public function go(): void {
        $this->help();
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn references_protected_method_found_across_subclass_files() {
    // `Base::boot` is protected. It is NOT narrowed (its complete scope needs
    // the transitive subtype set, only known once indexing finishes), so the
    // full-scope search must still find calls in both the declaring class and a
    // subclass in another file.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"//- /src/Base.php
<?php
namespace App;
class Base {
    protected function bo$0ot(): void {}
    //                 ^^^^ def
    public function init(): void {
        $this->boot();
        //     ^^^^ ref
    }
}

//- /src/Child.php
<?php
namespace App;
class Child extends Base {
    public function start(): void {
        $this->boot();
        //     ^^^^ ref
    }
}
"#,
    )
    .await;
}

#[tokio::test]
async fn references_private_method_on_trait_using_class_stays_complete() {
    // A class that composes a trait is excluded from narrowing (trait bodies
    // could resolve elsewhere), so the search keeps full scope. Either way the
    // result must be complete: the in-class call to the private method is found.
    let mut s = TestServer::new().await;
    s.check_references_annotated(
        r#"<?php
trait Loggable {
    public function log(): void {}
}
class Service {
    use Loggable;
    private function pre$0pare(): void {}
    //               ^^^^^^^ def
    public function handle(): void {
        $this->prepare();
        //     ^^^^^^^ ref
    }
}
"#,
    )
    .await;
}
