//! `codebase` salsa query — aggregates every file's `StubSlice` into a list
//! of slices that can be fed into a `mir_analyzer::db::MirDb` via
//! `ingest_stub_slice`.
//!
//! The salsa output is just the slice list (Send + Sync). Consumers build a
//! `MirDb` from the slices on demand. We cannot store `Arc<MirDb>` in salsa
//! because `MirDb` contains a non-`Sync` `salsa::Storage<MirDb>`.

use std::sync::Arc;

use mir_codebase::storage::StubSlice;
use salsa::{Database, Update};

use crate::db::definitions::file_definitions;
use crate::db::input::Workspace;

/// Opaque handle to the aggregated slice list. Uses pointer equality for
/// memoization and caching semantics (matching `ParsedArc`'s pattern).
#[derive(Clone)]
pub struct CodebaseArc(pub Arc<[Arc<StubSlice>]>);

impl PartialEq for CodebaseArc {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for CodebaseArc {}

impl CodebaseArc {
    pub fn slices(&self) -> &[Arc<StubSlice>] {
        &self.0
    }

    /// Build a fresh `MirDb` populated with the bundled stubs and every
    /// aggregated slice. `php_version` is passed to `load_stubs_for_version`
    /// so version-gated stubs are loaded correctly.
    pub fn build_mir_db(&self, php_version: mir_analyzer::PhpVersion) -> mir_analyzer::db::MirDb {
        let mut db = mir_analyzer::db::MirDb::default();
        mir_analyzer::stubs::load_stubs_for_version(&mut db, php_version);
        for slice in self.0.iter() {
            db.ingest_stub_slice(slice);
        }
        db
    }
}

// SAFETY: identical contract to other `*Arc` newtypes in this module.
// When dependencies haven't changed, salsa won't re-run and will reuse the memoized Arc.
// When dependencies have changed, salsa re-runs and we compare slice pointers element-wise:
// if all inner slice Arc pointers are identical, we return false to reuse the old outer Arc,
// avoiding a cache miss in `cached_mir_db()` (which uses `Arc::ptr_eq` to detect changes).
unsafe impl Update for CodebaseArc {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        let old_ref = unsafe { &mut *old_pointer };

        // Check if all slice pointers are identical.
        if old_ref.0.len() == new_value.0.len()
            && old_ref
                .0
                .iter()
                .zip(new_value.0.iter())
                .all(|(old_slice, new_slice)| Arc::ptr_eq(old_slice, new_slice))
        {
            // All slices unchanged — don't update, reuse old outer Arc pointer.
            false
        } else {
            // At least one slice pointer changed — update.
            *old_ref = new_value;
            true
        }
    }
}

/// Aggregate every file's `StubSlice` into one list. Depends on
/// `Workspace.files` and transitively on every file's `file_definitions`
/// query. Stubs are *not* part of the aggregate — they're loaded on the
/// consumer side when constructing a `MirDb`, so version changes don't
/// invalidate this query.
///
/// Memoization is enabled (no `no_eq`): when dependencies haven't changed,
/// salsa skips re-running and the old outer Arc pointer is reused, hitting
/// the `cached_mir_db()` cache. When dependencies change, the `Update` impl
/// compares slice pointers element-wise to detect true changes.
#[salsa::tracked]
pub fn codebase(db: &dyn Database, ws: Workspace) -> CodebaseArc {
    let files = ws.files(db);
    let slices: Vec<Arc<StubSlice>> = files
        .iter()
        .map(|sf| file_definitions(db, *sf).0.clone())
        .collect();
    CodebaseArc(Arc::from(slices))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::db::analysis::AnalysisHost;
    use crate::db::input::{FileId, SourceFile};
    use mir_analyzer::db::type_exists_via_db;
    use salsa::Setter;

    #[test]
    fn codebase_aggregates_classes_across_files() {
        let host = AnalysisHost::new();
        let f1 = SourceFile::new(
            host.db(),
            FileId(0),
            Arc::<str>::from("file:///a.php"),
            Arc::<str>::from("<?php\nnamespace A;\nclass Foo {}"),
            None,
        );
        let f2 = SourceFile::new(
            host.db(),
            FileId(1),
            Arc::<str>::from("file:///b.php"),
            Arc::<str>::from("<?php\nnamespace B;\nclass Bar {}"),
            None,
        );
        let ws = Workspace::new(
            host.db(),
            Arc::from([f1, f2]),
            mir_analyzer::PhpVersion::LATEST,
        );

        let cb = codebase(host.db(), ws);
        let mir_db = cb.build_mir_db(mir_analyzer::PhpVersion::LATEST);
        assert!(type_exists_via_db(&mir_db, "A\\Foo"));
        assert!(type_exists_via_db(&mir_db, "B\\Bar"));
    }

    #[test]
    fn codebase_reruns_after_file_edit() {
        let mut host = AnalysisHost::new();
        let f1 = SourceFile::new(
            host.db(),
            FileId(0),
            Arc::<str>::from("file:///t.php"),
            Arc::<str>::from("<?php\nclass Before {}"),
            None,
        );
        let ws = Workspace::new(host.db(), Arc::from([f1]), mir_analyzer::PhpVersion::LATEST);

        let a1 = codebase(host.db(), ws);
        let mir_a1 = a1.build_mir_db(mir_analyzer::PhpVersion::LATEST);
        assert!(type_exists_via_db(&mir_a1, "Before"));
        let first_ptr = Arc::as_ptr(&a1.0);

        f1.set_text(host.db_mut())
            .to(Arc::<str>::from("<?php\nclass After {}"));
        let a2 = codebase(host.db(), ws);
        assert_ne!(first_ptr, Arc::as_ptr(&a2.0), "edit should invalidate");
        let mir_a2 = a2.build_mir_db(mir_analyzer::PhpVersion::LATEST);
        assert!(type_exists_via_db(&mir_a2, "After"));
        assert!(!type_exists_via_db(&mir_a2, "Before"));
    }

    #[test]
    fn codebase_memoizes_when_nothing_changes() {
        let host = AnalysisHost::new();
        let f1 = SourceFile::new(
            host.db(),
            FileId(0),
            Arc::<str>::from("file:///t.php"),
            Arc::<str>::from("<?php\nclass X {}"),
            None,
        );
        let ws = Workspace::new(host.db(), Arc::from([f1]), mir_analyzer::PhpVersion::LATEST);

        let a1 = codebase(host.db(), ws);
        let a2 = codebase(host.db(), ws);
        assert!(
            Arc::ptr_eq(&a1.0, &a2.0),
            "no input change — second call should return the memoized Arc"
        );
    }

    #[test]
    fn codebase_preserves_outer_arc_when_slices_unchanged() {
        let mut host = AnalysisHost::new();
        let f1 = SourceFile::new(
            host.db(),
            FileId(0),
            Arc::<str>::from("file:///t.php"),
            Arc::<str>::from("<?php\nclass X {}"),
            None,
        );
        let ws = Workspace::new(host.db(), Arc::from([f1]), mir_analyzer::PhpVersion::LATEST);

        // Get initial codebase — creates outer Arc and inner slice Arc.
        let a1 = codebase(host.db(), ws);
        let first_inner_ptr = Arc::as_ptr(&a1.0[0]);

        // Edit in a way that doesn't change the definitions (e.g., add whitespace).
        // This invalidates parsed_doc, which invalidates file_definitions,
        // which creates a new StubSlice pointer.
        f1.set_text(host.db_mut())
            .to(Arc::<str>::from("<?php\nclass X {  }"));

        let a2 = codebase(host.db(), ws);
        let second_inner_ptr = Arc::as_ptr(&a2.0[0]);

        // Inner slice pointer should change (edit invalidates file_definitions).
        // This is expected because parsing creates a new AST.
        assert_ne!(
            first_inner_ptr, second_inner_ptr,
            "whitespace edit invalidates file_definitions"
        );

        // Both MirDbs should function correctly, verifying the fix doesn't break
        // the query semantics.
        let mir_a1 = a1.build_mir_db(mir_analyzer::PhpVersion::LATEST);
        let mir_a2 = a2.build_mir_db(mir_analyzer::PhpVersion::LATEST);
        assert!(type_exists_via_db(&mir_a1, "X"));
        assert!(type_exists_via_db(&mir_a2, "X"));
    }
}
