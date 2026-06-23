//! Find-references session-age degradation: old vs new read path.
//!
//! The OLD `textDocument/references` read path ran `ensure_files_ingested` on
//! every request — for each candidate file: `AnalysisSession::ingest_file`
//! (definition churn + `update_reverse_deps_for` + `evict_with_dependents` over
//! the dependency graph) + a `to_owned_program` clone + a parallel
//! `analyze_batch`. The graph those mutations walk grows as the workspace warms,
//! so the *same* request slows down the longer a session runs.
//!
//! The NEW path (`AnalysisSession::references_to_in_files`) reads the memoized
//! `analyze_file` query over the same candidate set — no ingest, no reverse-dep
//! churn, no index mutation. Warm files are memo hits; an edit re-analyzes only
//! the touched file.
//!
//! This bench holds the candidate set FIXED and varies the background-warmed
//! file count, measuring both paths per level. OLD should climb with warm size;
//! NEW should stay flat. Auto-skips when the Laravel fixture is absent.

use std::sync::Arc;
use std::time::{Duration, Instant};

use mir_analyzer::{AnalysisSession, BatchFileAnalyzer, Name, ParsedFile, PhpVersion};
use php_ast::owned::to_owned_program;
use php_lsp::ast::ParsedDoc;
use php_rs_parser::source_map::SourceMap;
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

fn edited(text: &str, tag: usize) -> Arc<str> {
    Arc::from(format!("{text}\n// bench-edit-{tag}\n").as_str())
}

/// OLD path: one `ensure_files_ingested`-equivalent pass over the candidates,
/// with `candidates[0]` re-parsed from edited source so the re-ingest does real
/// work (mirrors a keystroke landing on an open file).
fn old_path(session: &AnalysisSession, candidates: &[SourceFile], tag: usize) {
    let mut parsed_files: Vec<ParsedFile> = Vec::with_capacity(candidates.len());
    for (i, cand) in candidates.iter().enumerate() {
        let src = if i == 0 {
            edited(&cand.text, tag)
        } else {
            cand.text.clone()
        };
        let doc = ParsedDoc::parse(src.clone());
        session.ingest_file(cand.file.clone(), src.clone());
        let source_map = SourceMap::new(doc.source());
        let owned = to_owned_program(doc.program());
        parsed_files.push(ParsedFile::new(cand.file.clone(), src, owned, source_map));
    }
    let batch = BatchFileAnalyzer::new(session);
    std::hint::black_box(batch.analyze_batch(parsed_files));
}

/// NEW path: the production references read — a pure `references_to_in_files`
/// over the candidate set. No mutation; warm files are `analyze_file` memo hits.
/// This is what repeats per request, so flatness here is the whole point.
fn new_path(session: &AnalysisSession, sym: &Name, candidates: &[SourceFile]) {
    let files: Vec<Arc<str>> = candidates.iter().map(|c| c.file.clone()).collect();
    std::hint::black_box(session.references_to_in_files(sym, &files));
}

fn mean_ms(samples: &[Duration]) -> f64 {
    let total: Duration = samples.iter().sum();
    total.as_secs_f64() * 1000.0 / samples.len() as f64
}

fn measure(mut op: impl FnMut(usize)) -> f64 {
    let mut samples: Vec<Duration> = Vec::with_capacity(ITERS_PER_LEVEL);
    for iter in 0..ITERS_PER_LEVEL {
        let t0 = Instant::now();
        op(iter);
        let dt = t0.elapsed();
        if iter >= WARMUP_ITERS {
            samples.push(dt);
        }
    }
    mean_ms(&samples)
}

fn run_path(
    label: &str,
    background: &[&SourceFile],
    candidates: &[SourceFile],
    sym: &Name,
    new: bool,
) {
    let warm_levels: Vec<usize> = [0usize, 200, 500, 1000, background.len()]
        .into_iter()
        .filter(|&n| n <= background.len())
        .collect();

    println!("\n[{label}]  warm_files   mean_ms");
    let mut first = f64::NAN;
    let mut last = f64::NAN;
    for &warm in &warm_levels {
        let session = AnalysisSession::new(PhpVersion::LATEST);
        if new {
            for sf in background.iter().take(warm) {
                session.set_file_text(sf.file.clone(), sf.text.clone());
            }
            for cand in candidates {
                session.set_file_text(cand.file.clone(), cand.text.clone());
            }
        } else {
            for sf in background.iter().take(warm) {
                session.ingest_file(sf.file.clone(), sf.text.clone());
            }
            for cand in candidates {
                session.ingest_file(cand.file.clone(), cand.text.clone());
            }
        }

        let m = if new {
            measure(|_| new_path(&session, sym, candidates))
        } else {
            measure(|tag| old_path(&session, candidates, tag))
        };
        if first.is_nan() {
            first = m;
        }
        last = m;
        println!("           {warm:>10}   {m:>7.2}");
    }
    let ratio = last / first;
    let verdict = if ratio >= 1.30 { "DEGRADES" } else { "FLAT" };
    println!("           last/first = {ratio:.2}x  → {verdict}");
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
        eprintln!("no candidate files mention `{METHOD}` — cannot run degradation bench");
        return;
    }

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

    // The codebase key only filters results; the measured cost is analyze_file /
    // ingest over the candidate set, so any method symbol exercises the path.
    let sym = Name::method("App\\Models\\Model", METHOD);

    // Process warm-up: a throwaway run so the first measured level isn't paying
    // first-touch CPU/allocator costs that would inflate the cold ratio.
    {
        let session = AnalysisSession::new(PhpVersion::LATEST);
        for cand in &candidates {
            session.set_file_text(cand.file.clone(), cand.text.clone());
        }
        for _ in 0..WARMUP_ITERS {
            new_path(&session, &sym, &candidates);
        }
    }

    run_path("NEW analyze_file", &background, &candidates, &sym, true);
    run_path("OLD ensure_ingested", &background, &candidates, &sym, false);

    // Stage 4 visibility scoping: a private method's references can only live in
    // its declaring file, so the handler narrows the candidate set from every
    // text-match to that one file. Contrast the full candidate query with the
    // single-file query both at full warm.
    {
        let session = AnalysisSession::new(PhpVersion::LATEST);
        for sf in &background {
            session.set_file_text(sf.file.clone(), sf.text.clone());
        }
        for cand in &candidates {
            session.set_file_text(cand.file.clone(), cand.text.clone());
        }
        for _ in 0..WARMUP_ITERS {
            new_path(&session, &sym, &candidates);
            new_path(&session, &sym, &candidates[..1]);
        }
        let full = measure(|_| new_path(&session, &sym, &candidates));
        let scoped = measure(|_| new_path(&session, &sym, &candidates[..1]));
        println!(
            "\n[Stage 4: private scoping @ full warm]  {}-file full: {full:.3} ms   1-file scoped: {scoped:.3} ms   {:.1}x faster",
            candidates.len(),
            full / scoped,
        );
    }
}
