//! Fixture-free find-references improvement benchmark.
//!
//! Models the real cost driver localized earlier: a *common* public method name
//! (`process`) shared across unrelated classes, where only a fraction of the
//! text-matching files actually reference the target `App\Service`. This is the
//! case where the reachability pre-filter helps.
//!
//!   BEFORE — analyze `references_to_in_files` over *every* file that
//!     text-matches the method name (the old post-filter behavior).
//!   AFTER  — analyze only files that also mention the owner class `Service`
//!     (the new pre-filter in `ReferenceQuery::collect`).
//!
//! Both produce the same references (a file that never names `Service` can't
//! resolve `Service::process`), so this is pure speedup. Reports the cold
//! (first-query) latency the user feels, and the SESSION axis for regression.
//!
//! Run: `cargo bench --bench references_scaling`

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mir_analyzer::Name;
use php_lsp::document_store::DocumentStore;
use tower_lsp::lsp_types::Url;

const HOT_METHOD: &str = "process";
const OWNER: &str = "Service";
/// 1 in N files actually references `App\Service`; the rest define/call their
/// own `process()` (text-match the method, never name the owner).
const REACH_EVERY: usize = 10;

fn service_file() -> (Url, String) {
    let url = Url::parse("file:///synth/Service.php").unwrap();
    let text = format!(
        "<?php\nnamespace App;\nclass {OWNER} {{\n    public function {HOT_METHOD}(): void {{}}\n}}\n"
    );
    (url, text)
}

/// Filler so each file is a representative size (~24 methods with real bodies
/// for `analyze_file` to infer), not a toy — otherwise a fixed setup cost swamps
/// the per-candidate analysis the pre-filter actually removes.
fn filler() -> String {
    let mut s = String::new();
    for j in 0..24 {
        s.push_str(&format!(
            "    public function helper{j}(int $x, string $s): int {{\n\
             \x20       $y = $x * {j} + strlen($s);\n\
             \x20       return $y > 0 ? $y : -$y;\n\
             \x20   }}\n"
        ));
    }
    s
}

/// References `App\Service::process` — names the owner (type hint) and calls it.
fn reachable_file(i: usize) -> (Url, String) {
    let url = Url::parse(&format!("file:///synth/R{i}.php")).unwrap();
    let text = format!(
        "<?php\nnamespace App;\n\
         class R{i} {{\n\
         \x20   private {OWNER} $svc;\n\
         \x20   public function run(int $a): int {{\n\
         \x20       $this->svc->{HOT_METHOD}();\n\
         \x20       return $a + {i};\n\
         \x20   }}\n{}}}\n",
        filler()
    );
    (url, text)
}

/// Noise: defines and calls its *own* `process()` — text-matches the method
/// name but never names `Service`, so it cannot resolve to `Service::process`.
fn noise_file(i: usize) -> (Url, String) {
    let url = Url::parse(&format!("file:///synth/N{i}.php")).unwrap();
    let text = format!(
        "<?php\nnamespace App;\n\
         class N{i} {{\n\
         \x20   public function {HOT_METHOD}(): void {{}}\n\
         \x20   public function run(int $a): int {{\n\
         \x20       $this->{HOT_METHOD}();\n\
         \x20       return $a + {i};\n\
         \x20   }}\n{}}}\n",
        filler()
    );
    (url, text)
}

/// Returns the store plus the set of URLs that name the owner (the reachable
/// subset the pre-filter keeps).
fn build(n: usize) -> (DocumentStore, HashSet<String>) {
    let store = DocumentStore::new();
    let mut reachable = HashSet::new();
    let (su, st) = service_file();
    reachable.insert(su.as_str().to_string());
    store.ingest(su, &st);
    for i in 0..n.saturating_sub(1) {
        let (u, t) = if i % REACH_EVERY == 0 {
            let f = reachable_file(i);
            reachable.insert(f.0.as_str().to_string());
            f
        } else {
            noise_file(i)
        };
        store.ingest(u, &t);
    }
    store.mark_index_ready();
    (store, reachable)
}

fn arc_urls(urls: impl IntoIterator<Item = Url>) -> Vec<Arc<str>> {
    urls.into_iter().map(|u| Arc::from(u.as_str())).collect()
}

fn median_ms(mut s: Vec<Duration>) -> f64 {
    s.sort();
    s[s.len() / 2].as_secs_f64() * 1000.0
}

/// Median cold latency: a fresh store per rep so `analyze_file` is never a memo
/// hit. `select` picks the candidate subset from the freshly-built store.
fn cold_ms(
    n: usize,
    reps: usize,
    sym: &Name,
    select: impl Fn(&DocumentStore, &HashSet<String>) -> Vec<Arc<str>>,
) -> (usize, f64) {
    let mut samples = Vec::with_capacity(reps);
    let mut count = 0;
    for _ in 0..reps {
        let (store, reachable) = build(n);
        let files = select(&store, &reachable);
        count = files.len();
        let t = Instant::now();
        std::hint::black_box(store.session_references_to(sym, &files, None));
        samples.push(t.elapsed());
    }
    (count, median_ms(samples))
}

fn main() {
    let sym = Name::method(format!("App\\{OWNER}"), HOT_METHOD);
    let reps = 3usize;

    println!(
        "=== REACHABILITY PRE-FILTER: cold `{OWNER}::{HOT_METHOD}` references ===\n\
         (1 in {REACH_EVERY} files reference the owner; the rest share the method name)\n"
    );
    println!(
        "{:>7}  {:>10} {:>10}  {:>12} {:>12}  {:>8}",
        "files", "before_n", "after_n", "before_ms", "after_ms", "speedup"
    );
    for &n in &[100usize, 500, 1000, 3000] {
        let (before_n, before_ms) = cold_ms(n, reps, &sym, |s, _| {
            arc_urls(s.candidate_urls_for(HOT_METHOD))
        });
        let (after_n, after_ms) = cold_ms(n, reps, &sym, |s, reach| {
            arc_urls(
                s.candidate_urls_for(HOT_METHOD)
                    .into_iter()
                    .filter(|u| reach.contains(u.as_str())),
            )
        });
        println!(
            "{n:>7}  {before_n:>10} {after_n:>10}  {before_ms:>12.3} {after_ms:>12.3}  {:>7.2}x",
            before_ms / after_ms
        );
    }

    println!("\n=== WARM SWEEP: background warm_analysis_sweep, then first query ===");
    println!(
        "{:>7}  {:>10} {:>12} {:>12}",
        "files", "sweep_ms", "cold_ms", "warmed_ms"
    );
    for &n in &[500usize, 1000, 3000] {
        // Cold: first query pays per-candidate analyze_file.
        let (store, reachable) = build(n);
        let files: Vec<Arc<str>> = arc_urls(
            store
                .candidate_urls_for(HOT_METHOD)
                .into_iter()
                .filter(|u| reachable.contains(u.as_str())),
        );
        let t = Instant::now();
        std::hint::black_box(store.session_references_to(&sym, &files, None));
        let cold_ms = t.elapsed().as_secs_f64() * 1000.0;

        // Warmed: the sweep runs in the background after indexing; the first
        // user-visible query is then a memo hit.
        let (store, reachable) = build(n);
        let files: Vec<Arc<str>> = arc_urls(
            store
                .candidate_urls_for(HOT_METHOD)
                .into_iter()
                .filter(|u| reachable.contains(u.as_str())),
        );
        let t = Instant::now();
        let cancel = store.begin_warm_sweep();
        store.warm_analysis_sweep(&cancel);
        let sweep_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = Instant::now();
        std::hint::black_box(store.session_references_to(&sym, &files, None));
        let warmed_ms = t.elapsed().as_secs_f64() * 1000.0;
        println!("{n:>7}  {sweep_ms:>10.1} {cold_ms:>12.3} {warmed_ms:>12.3}");
    }

    println!("\n=== SESSION AXIS: repeated references after unrelated edits (N=1000) ===");
    let (store, reachable) = build(1000);
    let after: Vec<Arc<str>> = arc_urls(
        store
            .candidate_urls_for(HOT_METHOD)
            .into_iter()
            .filter(|u| reachable.contains(u.as_str())),
    );
    for _ in 0..3 {
        std::hint::black_box(store.session_references_to(&sym, &after, None));
    }
    println!("{:>5}  {:>10}  {:>13}", "iter", "edited", "references_ms");
    let mut session = Vec::new();
    for iter in 0..12usize {
        let victim = (iter * 7) % 999;
        let (u, t) = noise_file(victim);
        let _ = t;
        store.ingest(
            u,
            &format!(
                "<?php\nnamespace App;\nclass N{victim} {{ public function {HOT_METHOD}(): void {{}}\n\
                 public function run(): void {{ $this->{HOT_METHOD}(); /* edit {iter} */ }} }}\n"
            ),
        );
        let t0 = Instant::now();
        std::hint::black_box(store.session_references_to(&sym, &after, None));
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        session.push(ms);
        println!("{iter:>5}  {:>10}  {ms:>13.3}", format!("N{victim}"));
    }
    let (early, late) = (session[1], *session.last().unwrap());
    println!(
        "late/early = {:.2}x  →  {}",
        late / early,
        if late / early >= 1.5 {
            "DEGRADES"
        } else {
            "FLAT"
        }
    );
}
