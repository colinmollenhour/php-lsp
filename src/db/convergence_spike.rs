//! Phase 0 spike for the database-convergence work (DB_CONVERGENCE_PLAN.md).
//!
//! Proves the load-bearing assumption: a `#[salsa::tracked]` query *defined in
//! the php-lsp crate* can operate on mir-analyzer's concrete `MirDbStorage`,
//! via mir's `MirDatabase` trait + `SourceFile` input, and that salsa's
//! cross-crate ingredient registration, memoization, and input-invalidation all
//! work. If this compiles and the tests pass, the "mir owns the single db,
//! php-lsp contributes queries" direction is viable.
//!
//! Test-only: the tracked fn ships nowhere near the real binary.
#![cfg(test)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mir_analyzer::db::{MirDatabase, MirDbStorage, SourceFile};
use salsa::Setter;
use serial_test::serial;

/// Counts actual (non-memoized) executions so the tests can assert that salsa
/// served a cached value rather than recomputing.
static EXEC_COUNT: AtomicUsize = AtomicUsize::new(0);

/// A trivial php-lsp-owned tracked query over mir's db + input.
#[salsa::tracked]
fn spike_text_len(db: &dyn MirDatabase, file: SourceFile) -> usize {
    EXEC_COUNT.fetch_add(1, Ordering::SeqCst);
    file.text(db).len()
}

#[test]
#[serial]
fn cross_crate_ingredient_registers_and_memoizes() {
    EXEC_COUNT.store(0, Ordering::SeqCst);
    let db = MirDbStorage::default();

    let file = SourceFile::new(&db, Arc::from("a.php"), Arc::from("<?php echo 1;"));

    let first = spike_text_len(&db, file);
    let second = spike_text_len(&db, file);

    assert_eq!(first, "<?php echo 1;".len());
    assert_eq!(first, second);
    assert_eq!(
        EXEC_COUNT.load(Ordering::SeqCst),
        1,
        "second call must be served from the salsa memo, not recomputed"
    );
}

#[test]
#[serial]
fn input_mutation_invalidates_the_memo() {
    EXEC_COUNT.store(0, Ordering::SeqCst);
    let mut db = MirDbStorage::default();

    let file = SourceFile::new(&db, Arc::from("b.php"), Arc::from("<?php echo 1;"));
    let before = spike_text_len(&db, file);

    file.set_text(&mut db)
        .to(Arc::from("<?php echo 123456789;"));
    let after = spike_text_len(&db, file);

    assert_ne!(before, after, "changing the input must recompute");
    assert_eq!(
        EXEC_COUNT.load(Ordering::SeqCst),
        2,
        "one compute before the edit, one after"
    );
}

#[test]
#[serial]
fn php_lsp_query_coexists_with_mir_query_on_same_db() {
    let db = MirDbStorage::default();
    let src: Arc<str> = Arc::from("<?php\nfunction foo() {}\n");
    let file = SourceFile::new(&db, Arc::from("c.php"), src.clone());

    // php-lsp-owned query and a mir-owned tracked query on the same db instance.
    let len = spike_text_len(&db, file);
    let defs = mir_analyzer::db::collect_file_definitions(&db, file);

    assert_eq!(len, src.len());
    let _ = defs; // existence + no panic is the signal: both ingredients share one Storage
}
