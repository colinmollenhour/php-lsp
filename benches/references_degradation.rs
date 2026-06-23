//! Find-references read-path performance guard.
//!
//! The references read path is a memoized `analyze_file` query over the
//! candidate set — no ingest, no shared-index mutation. This bench pins two
//! properties on the Laravel fixture:
//!
//! - per-request time stays FLAT as the background-warmed file count grows
//!   (re-introducing per-request mutation would make it climb again); and
//! - visibility scoping (a private method's references live only in its
//!   declaring file) collapses the candidate set from every text-match to one
//!   file.
//!
//! Auto-skips when the Laravel fixture is absent.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mir_analyzer::{AnalysisSession, Name, PhpVersion};
use tower_lsp::lsp_types::Url;

const METHOD: &str = "save";
const CANDIDATE_CAP: usize = 30;
const ITERS_PER_LEVEL: usize = 12;
const WARMUP_ITERS: usize = 4;

struct SourceFile {
    file: Arc<str>,
    text: Arc<str>,
}

fn laravel_sources() -> Option<Vec<SourceFile>> {
    let fixture_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/fixtures/laravel/src");
    if !fixture_dir.exists() {
        return None;
    }
    let files: Vec<SourceFile> = walkdir::WalkDir::new(&fixture_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "php"))
        .filter_map(|e| {
            let url = Url::from_file_path(e.path()).ok()?;
            let text = std::fs::read_to_string(e.path()).ok()?;
            Some(SourceFile {
                file: Arc::from(url.as_str()),
                text: Arc::from(text.as_str()),
            })
        })
        .collect();
    Some(files)
}

/// The production references read: a pure `references_to_in_files` over the
/// candidate set. No mutation; warm files are `analyze_file` memo hits.
fn references(session: &AnalysisSession, sym: &Name, files: &[Arc<str>]) {
    std::hint::black_box(session.references_to_in_files(sym, files));
}

fn mean_ms(samples: &[Duration]) -> f64 {
    let total: Duration = samples.iter().sum();
    total.as_secs_f64() * 1000.0 / samples.len() as f64
}

fn measure(mut op: impl FnMut()) -> f64 {
    let mut samples: Vec<Duration> = Vec::with_capacity(ITERS_PER_LEVEL);
    for iter in 0..ITERS_PER_LEVEL {
        let t0 = Instant::now();
        op();
        let dt = t0.elapsed();
        if iter >= WARMUP_ITERS {
            samples.push(dt);
        }
    }
    mean_ms(&samples)
}

fn main() {
    let Some(all) = laravel_sources() else {
        eprintln!(
            "Laravel fixture not found — run scripts/setup_laravel_fixture.sh to enable references_degradation"
        );
        return;
    };

    let candidates: Vec<SourceFile> = all
        .iter()
        .filter(|f| f.text.contains(METHOD))
        .take(CANDIDATE_CAP)
        .map(|f| SourceFile {
            file: f.file.clone(),
            text: f.text.clone(),
        })
        .collect();
    if candidates.is_empty() {
        eprintln!("no candidate files mention `{METHOD}` — cannot run the bench");
        return;
    }
    let candidate_files: Vec<Arc<str>> = candidates.iter().map(|c| c.file.clone()).collect();

    let candidate_keys: std::collections::HashSet<&str> =
        candidates.iter().map(|c| c.file.as_ref()).collect();
    let background: Vec<&SourceFile> = all
        .iter()
        .filter(|f| !candidate_keys.contains(f.file.as_ref()))
        .collect();

    eprintln!(
        "Laravel fixture: {} files; {} candidates mention `{METHOD}`",
        all.len(),
        candidates.len()
    );

    // The codebase key only filters results; the measured cost is `analyze_file`
    // over the candidate set, so any method symbol exercises the path.
    let sym = Name::method("App\\Models\\Model", METHOD);

    // Throwaway run so the first measured level isn't paying first-touch
    // CPU/allocator costs that would skew the cold sample.
    {
        let session = AnalysisSession::new(PhpVersion::LATEST);
        for cand in &candidates {
            session.set_file_text(cand.file.clone(), cand.text.clone());
        }
        for _ in 0..WARMUP_ITERS {
            references(&session, &sym, &candidate_files);
        }
    }

    // Flatness: hold the candidate set fixed, grow the background-warmed file
    // count. Per-request time must not climb with warmed-set size.
    let warm_levels: Vec<usize> = [0usize, 200, 500, 1000, background.len()]
        .into_iter()
        .filter(|&n| n <= background.len())
        .collect();
    println!(
        "\nwarm_files   mean_ms (fixed {}-file references op)",
        candidates.len()
    );
    let mut first = f64::NAN;
    let mut last = f64::NAN;
    for &warm in &warm_levels {
        let session = AnalysisSession::new(PhpVersion::LATEST);
        for sf in background.iter().take(warm) {
            session.set_file_text(sf.file.clone(), sf.text.clone());
        }
        for cand in &candidates {
            session.set_file_text(cand.file.clone(), cand.text.clone());
        }
        let m = measure(|| references(&session, &sym, &candidate_files));
        if first.is_nan() {
            first = m;
        }
        last = m;
        println!("{warm:>10}   {m:>7.3}");
    }
    let ratio = last / first;
    println!(
        "last/first = {ratio:.2}x  → {}",
        if ratio >= 1.30 { "DEGRADES" } else { "FLAT" }
    );

    // Visibility scoping: a private method's references can only live in its
    // declaring file, so the handler narrows the candidate set to that one file.
    {
        let session = AnalysisSession::new(PhpVersion::LATEST);
        for sf in &background {
            session.set_file_text(sf.file.clone(), sf.text.clone());
        }
        for cand in &candidates {
            session.set_file_text(cand.file.clone(), cand.text.clone());
        }
        for _ in 0..WARMUP_ITERS {
            references(&session, &sym, &candidate_files);
            references(&session, &sym, &candidate_files[..1]);
        }
        let full = measure(|| references(&session, &sym, &candidate_files));
        let scoped = measure(|| references(&session, &sym, &candidate_files[..1]));
        println!(
            "\nprivate scoping @ full warm: {}-file {full:.3} ms → 1-file {scoped:.3} ms  ({:.0}x)",
            candidates.len(),
            full / scoped,
        );
    }
}
