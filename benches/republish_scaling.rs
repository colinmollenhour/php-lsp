//! Per-edit diagnostics-republish scaling guard (WS3 acceptance).
//!
//! Simulates the LSP edit hot path at growing workspace sizes and pins the
//! WS3 property: per-edit republish cost is O(open files) — flat as the
//! ingested-file count grows — because `reanalyze_files_cancellable`
//! re-analyzes only the caller's open set and salsa memoization absorbs the
//! rest. The superseded dependent-sweep path (`reanalyze_dependents`) is
//! measured alongside as the degradation reference: it rebuilds
//! `dependency_graph()` per edit, an O(all-ingested-files) walk.
//!
//! Synthetic workspace: `base.php` defines `Base`; every file extends it, so
//! the whole workspace is a dependent of every base edit — the worst case for
//! the old path and exactly the shape (edit a base class under many
//! dependents) users report as "the server slows down".
//!
//! Run with `cargo bench --bench republish_scaling`. Release mode matters.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mir_analyzer::{AnalysisSession, IndexCancel, PhpVersion};

const SIZES: &[usize] = &[100, 1000, 5000];
const OPEN_FILES: usize = 4;
const EDITS: usize = 12;
const WARMUP_EDITS: usize = 4;
/// New-path flatness gate: mean per-edit time at the largest size must stay
/// within this factor of the smallest size.
const FLAT_RATIO: f64 = 1.5;

fn base_text(edit: usize) -> Arc<str> {
    Arc::from(format!(
        "<?php\nclass Base {{\n    public function ping(): int {{\n        $v = {edit};\n        return $v + 1;\n    }}\n}}\n"
    ))
}

fn dependent_text(i: usize) -> Arc<str> {
    Arc::from(format!(
        "<?php\nclass Dep{i} extends Base {{\n    public function go(): int {{\n        return $this->ping() + {i};\n    }}\n}}\n"
    ))
}

struct Workspace {
    session: AnalysisSession,
    base: Arc<str>,
    open_set: Vec<Arc<str>>,
}

/// Build a session with `size` ingested dependents, `OPEN_FILES` of which are
/// treated as the editor's open files.
fn build(size: usize, maintain_ref_index: bool) -> Workspace {
    let session = if maintain_ref_index {
        AnalysisSession::new(PhpVersion::LATEST)
    } else {
        AnalysisSession::new(PhpVersion::LATEST).without_reference_index()
    };
    session.ensure_all_stubs();

    let base: Arc<str> = Arc::from("bench://base.php");
    session.ingest_file(base.clone(), base_text(0));

    let mut open_set: Vec<Arc<str>> = Vec::with_capacity(OPEN_FILES);
    for i in 0..size {
        let file: Arc<str> = Arc::from(format!("bench://dep{i}.php"));
        session.ingest_file(file.clone(), dependent_text(i));
        if i < OPEN_FILES {
            open_set.push(file);
        }
    }
    Workspace {
        session,
        base,
        open_set,
    }
}

/// One simulated keystroke on the base file followed by the republish sweep.
fn edit_cycle(ws: &Workspace, edit: usize, sweep: impl Fn()) -> Duration {
    let t0 = Instant::now();
    ws.session.ingest_file(ws.base.clone(), base_text(edit));
    sweep();
    t0.elapsed()
}

fn mean_ms(samples: &[Duration]) -> f64 {
    samples.iter().sum::<Duration>().as_secs_f64() * 1000.0 / samples.len() as f64
}

fn measure_edits(ws: &Workspace, sweep: impl Fn()) -> f64 {
    let mut samples = Vec::with_capacity(EDITS - WARMUP_EDITS);
    for edit in 0..EDITS {
        let dt = edit_cycle(ws, edit + 1, &sweep);
        if edit >= WARMUP_EDITS {
            samples.push(dt);
        }
    }
    mean_ms(&samples)
}

fn main() {
    println!("republish scaling — per-edit cost (ingest + sweep), mean of {} edits", EDITS - WARMUP_EDITS);
    println!("{:>8}  {:>18}  {:>22}", "files", "open-set sweep ms", "dependent sweep ms");

    let mut open_first = f64::NAN;
    let mut open_last = f64::NAN;
    for &size in SIZES {
        // New path: re-analyze exactly the open files; no dependency graph.
        let ws = build(size, false);
        let open_ms = measure_edits(&ws, || {
            let analyses = ws
                .session
                .reanalyze_files_cancellable(&ws.open_set, &IndexCancel::new());
            std::hint::black_box(analyses);
        });

        // Old path: compute + re-analyze the transitive dependents of base.
        let ws_old = build(size, true);
        let dep_ms = measure_edits(&ws_old, || {
            let analyses = ws_old.session.reanalyze_dependents(ws_old.base.as_ref());
            std::hint::black_box(analyses);
        });

        if open_first.is_nan() {
            open_first = open_ms;
        }
        open_last = open_ms;
        println!("{size:>8}  {open_ms:>18.3}  {dep_ms:>22.3}");
    }

    let ratio = open_last / open_first;
    println!(
        "\nopen-set sweep {}→{} files: {ratio:.2}x  → {}",
        SIZES[0],
        SIZES[SIZES.len() - 1],
        if ratio > FLAT_RATIO { "DEGRADES (WS3 regression!)" } else { "FLAT" }
    );
    if ratio > FLAT_RATIO {
        std::process::exit(1);
    }
}
