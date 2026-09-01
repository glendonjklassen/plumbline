//! Leitwort / burst discovery: which Strong's concepts are bursty — repeated and
//! packed into one narrative stretch (Buber's *Leitwort*) — rather than scattered
//! evenly across the canon.
//!
//! Ported from overlay `Burst.hs`. For each concept with enough occurrences, a
//! scan statistic finds its densest window; a Poisson upper-tail probability (via
//! the regularized incomplete gamma, Numerical Recipes §6.2) scores how
//! surprising that packing is under uniform scatter, Bonferroni-corrected within
//! a concept and Benjamini–Hochberg-gated across concepts. Pure corpus
//! statistics, no ML data.

use std::collections::HashMap;

use plumbline_core::canon;
use plumbline_core::corpus::Corpus;
use plumbline_core::reference::{VRef, OT_NT_DIVIDE};

/// Discovery parameters. Defaults tuned against the real corpus: occur in ≥8
/// verses but ≤10% of the testament, cluster ≥4 in the window, 1% FDR.
#[derive(Debug, Clone, Copy)]
pub struct BurstParams {
    pub min_occ: usize,
    pub min_k: usize,
    pub max_rate: f64,
    pub fdr_alpha: f64,
}

impl Default for BurstParams {
    fn default() -> BurstParams {
        BurstParams { min_occ: 8, min_k: 4, max_rate: 0.10, fdr_alpha: 0.01 }
    }
}

/// One concept's discovered burst: its code, strength (`-log10` of the
/// multiple-testing-adjusted tail probability), that probability, its
/// corpus-wide occurrence-verse count, and the localized densest window.
#[derive(Debug, Clone, PartialEq)]
pub struct Burst {
    pub strongs: String,
    pub score: f64,
    pub p_value: f64,
    pub n: usize,
    pub win_start: VRef,
    pub win_end: VRef,
    pub win_count: usize,
    pub win_span: usize,
}

/// A compact human label for a burst's span (`"Genesis 1"`, `"Genesis 1–2"`,
/// `"Genesis 50 – Exodus 1"`), using a book-id → display-name function.
pub fn span_label(name: impl Fn(&str) -> String, start: &VRef, end: &VRef) -> String {
    if start.book == end.book && start.chapter == end.chapter {
        format!("{} {}", name(&start.book), start.chapter)
    } else if start.book == end.book {
        format!("{} {}–{}", name(&start.book), start.chapter, end.chapter)
    } else {
        format!("{} {} – {} {}", name(&start.book), start.chapter, name(&end.book), end.chapter)
    }
}

// ── numerical core (Numerical Recipes §6.1–6.2) ────────────────────────────────

/// `log Γ(x)` for `x > 0` (Lanczos approximation).
pub fn gammaln(xx: f64) -> f64 {
    let cof = [
        76.18009172947146,
        -86.50532032941677,
        24.01409824083091,
        -1.231739572450155,
        0.1208650973866179e-2,
        -0.5395239384953e-5,
    ];
    let tmp0 = xx + 5.5;
    let tmp = tmp0 - (xx + 0.5) * tmp0.ln();
    let mut ser = 1.000000000190015;
    for (j, c) in cof.iter().enumerate() {
        ser += c / (xx + (j as f64 + 1.0));
    }
    -tmp + (2.5066282746310005 * ser / xx).ln()
}

fn gamma_series(a: f64, x: f64) -> f64 {
    let gln = gammaln(a);
    let eps = 3.0e-14;
    let itmax = 400;
    let mut ap = a;
    let mut del = 1.0 / a;
    let mut s = 1.0 / a;
    for _ in 0..itmax {
        ap += 1.0;
        del *= x / ap;
        s += del;
        if del.abs() < s.abs() * eps {
            break;
        }
    }
    s * (-x + a * x.ln() - gln).exp()
}

fn gamma_cont_frac(a: f64, x: f64) -> f64 {
    let gln = gammaln(a);
    let fpmin = 1.0e-300;
    let eps = 3.0e-14;
    let itmax = 400;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / fpmin;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=itmax {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < fpmin {
            d = fpmin;
        }
        c = b + an / c;
        if c.abs() < fpmin {
            c = fpmin;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < eps {
            break;
        }
    }
    (-x + a * x.ln() - gln).exp() * h
}

/// The regularized lower incomplete gamma `P(a, x)` (series for `x < a+1`, else
/// the continued-fraction complement).
pub fn reg_gamma_p(a: f64, x: f64) -> f64 {
    if x < 0.0 || a <= 0.0 || x == 0.0 {
        0.0
    } else if x < a + 1.0 {
        gamma_series(a, x)
    } else {
        1.0 - gamma_cont_frac(a, x)
    }
}

/// `P(Poisson(λ) ≥ k)`, the exact upper tail.
pub fn poisson_upper_p(k: usize, lam: f64) -> f64 {
    if k == 0 {
        1.0
    } else if lam <= 0.0 {
        0.0
    } else {
        reg_gamma_p(k as f64, lam)
    }
}

/// Over all windows holding exactly `k` of the sorted occurrence positions, the
/// smallest span (inclusive verse positions) and the index where it starts.
fn min_span_k(pos: &[i64], k: usize) -> (i64, usize) {
    let n = pos.len();
    let span_at = |i: usize| pos[i + k - 1] - pos[i] + 1;
    let mut best = span_at(0);
    let mut besti = 0;
    let mut i = 1;
    while i + k <= n {
        let s = span_at(i);
        if s < best {
            best = s;
            besti = i;
        }
        i += 1;
    }
    (best, besti)
}

/// The single densest window among sorted occurrence positions, scored by the
/// Poisson upper-tail probability. Returns `(p, k, w, i0, i1)` or `None` when
/// too rare / too common / evenly spread.
pub fn scan_burst(bp: &BurstParams, big_n: i64, pos: &[i64]) -> Option<(f64, usize, i64, usize, usize)> {
    let n = pos.len();
    if n < bp.min_occ || big_n <= 0 || n as f64 > bp.max_rate * big_n as f64 {
        return None;
    }
    let p = n as f64 / big_n as f64;
    let mut best: Option<(f64, usize, i64, usize, usize)> = None;
    for k in bp.min_k..=n {
        let (w, i0) = min_span_k(pos, k);
        let lam = p * w as f64;
        if (k as f64) > lam {
            let pval = poisson_upper_p(k, lam);
            let cand = (pval, k, w, i0, i0 + k - 1);
            // Smallest tail wins; ties keep the earlier (smaller-k) candidate.
            if best.as_ref().is_none_or(|b| pval < b.0) {
                best = Some(cand);
            }
        }
    }
    best
}

/// Benjamini–Hochberg: given `(item, p)` pairs and FDR `alpha`, the items called
/// significant.
pub fn benjamini_hochberg<T: Clone>(alpha: f64, items: &[(T, f64)]) -> Vec<T> {
    let m = items.len();
    if m == 0 {
        return Vec::new();
    }
    // Indices sorted by ascending p-value; the largest rank clearing α·r/m is
    // the cutoff, and everything up to it is significant.
    let mut order: Vec<usize> = (0..m).collect();
    order.sort_by(|&a, &b| items[a].1.total_cmp(&items[b].1));
    let mut cutoff_rank = 0usize;
    for (rank0, &i) in order.iter().enumerate() {
        let r = rank0 + 1;
        if items[i].1 <= alpha * r as f64 / m as f64 {
            cutoff_rank = r;
        }
    }
    order.into_iter().take(cutoff_rank).map(|i| items[i].0.clone()).collect()
}

fn is_nt(book: &str) -> bool {
    canon::book_order(book).is_some_and(|o| o >= OT_NT_DIVIDE)
}

/// Discover the corpus's leitwörter, strongest burst first.
pub fn discover_leitworter(bp: &BurstParams, corpus: &Corpus) -> Vec<Burst> {
    let vs: Vec<&plumbline_core::corpus::Verse> = corpus.verses_iter().collect();
    let total = vs.len();
    let nt_start = vs.iter().position(|v| is_nt(&v.book)).unwrap_or(total);
    let (n_ot, n_nt) = (nt_start as i64, (total - nt_start) as i64);

    // concept → sorted absolute verse indices it occurs in
    let mut positions: HashMap<&str, Vec<i64>> = HashMap::new();
    for (i, v) in vs.iter().enumerate() {
        let mut codes: Vec<&str> = v.tokens.iter().flat_map(|t| t.strongs.iter().map(String::as_str)).collect();
        codes.sort_unstable();
        codes.dedup();
        for s in codes {
            positions.entry(s).or_default().push(i as i64);
        }
    }

    let mut scored: Vec<(Burst, f64)> = Vec::new();
    for (s, pv) in &positions {
        let n = pv.len();
        if n == 0 {
            continue;
        }
        let first_nt = pv[0] as usize >= nt_start;
        let last_nt = pv[n - 1] as usize >= nt_start;
        if first_nt != last_nt {
            continue; // a mixed OT/NT run is a tagging anomaly — skip
        }
        let big_n = if first_nt { n_nt } else { n_ot };
        if let Some((raw_p, k, w, i0, i1)) = scan_burst(bp, big_n, pv) {
            let levels = (n.saturating_sub(bp.min_k) + 1).max(1) as f64;
            let p_adj = (raw_p * levels).min(1.0);
            let score = -(p_adj.max(1.0e-300).log10());
            let start = vs[pv[i0] as usize].vref();
            let end = vs[pv[i1] as usize].vref();
            scored.push((
                Burst {
                    strongs: s.to_string(),
                    score,
                    p_value: p_adj,
                    n,
                    win_start: start,
                    win_end: end,
                    win_count: k,
                    win_span: w as usize,
                },
                p_adj,
            ));
        }
    }

    let mut sig = benjamini_hochberg(bp.fdr_alpha, &scored);
    sig.sort_by(|a, b| b.score.total_cmp(&a.score));
    sig
}

/// The same discovery, one bite at a time — for the boot warm.
///
/// [`discover_leitworter`] does the whole thing in one call on the only thread
/// that answers layout, taps and word studies: 83 ms on a desktop against a
/// ~300 ms warm-chunk budget, which on a phone is the entire budget for a phase.
/// Two cursored stages — positions (walk `budget` verses) then scoring (scan
/// `budget` codes) — then the cheap Benjamini-Hochberg gate and sort.
///
/// This scores codes in sorted order rather than `HashMap` order, so its output
/// is deterministic where `discover_leitworter`'s is not; the two agree code for
/// code but can order exact score ties differently. Callers key these by code, so
/// nothing downstream can tell.
pub struct LeitwortBuilder {
    bp: BurstParams,
    stage: u8,
    /// Verse index in stage 1, code index in stage 2.
    cursor: usize,
    /// Absolute index of the first NT verse, filled as stage 1 walks.
    nt_start: Option<usize>,
    positions: HashMap<String, Vec<i64>>,
    /// The codes to score, sorted — materialised when stage 1 finishes.
    codes: Vec<String>,
    scored: Vec<(Burst, f64)>,
    out: Option<Vec<Burst>>,
}

impl LeitwortBuilder {
    pub fn new(bp: &BurstParams) -> LeitwortBuilder {
        LeitwortBuilder {
            bp: *bp,
            stage: 1,
            cursor: 0,
            nt_start: None,
            positions: HashMap::new(),
            codes: Vec::new(),
            scored: Vec::new(),
            out: None,
        }
    }

    /// Do up to `budget` units of work. `true` while any remain.
    pub fn step(&mut self, corpus: &Corpus, budget: usize) -> bool {
        let budget = budget.max(1);
        let total = corpus.len();
        match self.stage {
            1 => {
                let end = (self.cursor + budget).min(total);
                for i in self.cursor..end {
                    let Some(v) = corpus.verse_at(i) else { continue };
                    if self.nt_start.is_none() && is_nt(&v.book) {
                        self.nt_start = Some(i);
                    }
                    let mut codes: Vec<&str> =
                        v.tokens.iter().flat_map(|t| t.strongs.iter().map(String::as_str)).collect();
                    codes.sort_unstable();
                    codes.dedup();
                    for s in codes {
                        self.positions.entry(s.to_string()).or_default().push(i as i64);
                    }
                }
                self.cursor = end;
                if end < total {
                    return true;
                }
                self.codes = self.positions.keys().cloned().collect();
                self.codes.sort_unstable();
                self.stage = 2;
                self.cursor = 0;
                true
            }
            2 => {
                let nt_start = self.nt_start.unwrap_or(total);
                let (n_ot, n_nt) = (nt_start as i64, (total - nt_start) as i64);
                let end = (self.cursor + budget).min(self.codes.len());
                for ci in self.cursor..end {
                    let s = &self.codes[ci];
                    let Some(pv) = self.positions.get(s) else { continue };
                    let n = pv.len();
                    if n == 0 {
                        continue;
                    }
                    let first_nt = pv[0] as usize >= nt_start;
                    let last_nt = pv[n - 1] as usize >= nt_start;
                    if first_nt != last_nt {
                        continue; // a mixed OT/NT run is a tagging anomaly — skip
                    }
                    let big_n = if first_nt { n_nt } else { n_ot };
                    if let Some((raw_p, k, w, i0, i1)) = scan_burst(&self.bp, big_n, pv) {
                        let levels = (n.saturating_sub(self.bp.min_k) + 1).max(1) as f64;
                        let p_adj = (raw_p * levels).min(1.0);
                        let score = -(p_adj.max(1.0e-300).log10());
                        let (Some(a), Some(b)) = (corpus.verse_at(pv[i0] as usize), corpus.verse_at(pv[i1] as usize))
                        else {
                            continue;
                        };
                        self.scored.push((
                            Burst {
                                strongs: s.clone(),
                                score,
                                p_value: p_adj,
                                n,
                                win_start: a.vref(),
                                win_end: b.vref(),
                                win_count: k,
                                win_span: w as usize,
                            },
                            p_adj,
                        ));
                    }
                }
                self.cursor = end;
                if end < self.codes.len() {
                    return true;
                }
                let mut sig = benjamini_hochberg(self.bp.fdr_alpha, &self.scored);
                sig.sort_by(|a, b| b.score.total_cmp(&a.score));
                self.out = Some(sig);
                // The positions map is the big allocation; release it now rather
                // than holding it until the caller collects.
                self.positions = HashMap::new();
                self.codes = Vec::new();
                self.scored = Vec::new();
                self.stage = 3;
                false
            }
            _ => false,
        }
    }

    /// The finished discoveries, once [`step`](Self::step) has returned `false`.
    pub fn take(&mut self) -> Option<Vec<Burst>> {
        self.out.take()
    }
}

/// Book (OSIS id) → its discovered leitwörter (window-opening book), strongest
/// first.
pub fn leitwort_by_book(bursts: &[Burst]) -> HashMap<String, Vec<Burst>> {
    let mut m: HashMap<String, Vec<Burst>> = HashMap::new();
    for b in bursts {
        m.entry(b.win_start.book.clone()).or_default().push(b.clone());
    }
    for list in m.values_mut() {
        list.sort_by(|a, b| b.score.total_cmp(&a.score));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gamma_and_poisson_are_sane() {
        // Γ(5) = 4! = 24 → gammaln(5) = ln 24.
        assert!((gammaln(5.0) - 24.0f64.ln()).abs() < 1e-6);
        // P(Poisson(λ) ≥ k): monotone, bounded.
        assert!((poisson_upper_p(0, 3.0) - 1.0).abs() < 1e-12);
        let p = poisson_upper_p(10, 2.0);
        assert!(p > 0.0 && p < 1e-3, "10 events when 2 expected is very unlikely, got {p}");
        // A tight cluster is far more surprising than a loose one.
        assert!(poisson_upper_p(6, 1.0) < poisson_upper_p(6, 5.0));
    }

    #[test]
    fn scan_finds_the_dense_window() {
        // 6 occurrences: 5 packed at 0..5, one far away at 900, over 1000 slots.
        let pos = [0i64, 1, 2, 3, 4, 900];
        let bp = BurstParams { min_occ: 5, min_k: 3, max_rate: 0.9, fdr_alpha: 0.05 };
        let (p, k, w, i0, i1) = scan_burst(&bp, 1000, &pos).unwrap();
        assert!(k >= 3 && w <= 6, "should localize the tight 0..4 window (k={k}, w={w})");
        assert!(i0 == 0 && i1 == k - 1);
        assert!(p < 0.01, "a packed window over 1000 slots is surprising, got {p}");
    }

    #[test]
    fn benjamini_hochberg_gates() {
        let items = vec![("a", 0.001), ("b", 0.6), ("c", 0.8), ("d", 0.9)];
        let sig = benjamini_hochberg(0.05, &items);
        assert_eq!(sig, vec!["a"]); // only the tiny p-value survives
    }

    /// A corpus with an OT and an NT half, one code packed into a run of verses
    /// and one spread thin. Far too small for the discovery rules to call anything
    /// significant: it exists to drive the builder's cursor through both stages.
    const BURSTY: &str = concat!(
        r#"{"format":"x","tokenization":"kjv1769-tok2","verses":8}"#,
        "\n",
        r#"{"b":"Gen","c":1,"v":1,"t":[["","a","",["H1"],0],["","b","",["H2"],0]]}"#,
        "\n",
        r#"{"b":"Gen","c":1,"v":2,"t":[["","a","",["H1"],0]]}"#,
        "\n",
        r#"{"b":"Gen","c":1,"v":3,"t":[["","a","",["H1"],0],["","c","",["H3"],0]]}"#,
        "\n",
        r#"{"b":"Gen","c":1,"v":4,"t":[["","a","",["H1"],0]]}"#,
        "\n",
        r#"{"b":"Gen","c":1,"v":5,"t":[["","d","",["H4"],0]]}"#,
        "\n",
        r#"{"b":"John","c":1,"v":1,"t":[["","e","",["G5"],0],["","f","",["G6"],0]]}"#,
        "\n",
        r#"{"b":"John","c":1,"v":2,"t":[["","e","",["G5"],0]]}"#,
        "\n",
        r#"{"b":"John","c":1,"v":3,"t":[["","g","",["G7"],0]]}"#,
    );

    /// Slicing is a scheduling change; it must not change the answer. Run against
    /// the real corpus, because the interesting disagreement — an exact score tie
    /// coming out in a different order — cannot happen in a five-verse fixture.
    /// Compared code for code rather than as a sequence, since only the builder's
    /// tie order is deterministic (see [`LeitwortBuilder`]).
    ///
    /// `cargo test -p plumbline-rnd -- --ignored sliced_leitwort`
    #[test]
    #[ignore]
    fn sliced_leitwort_discovery_matches_the_one_shot_version() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(corpus) = plumbline_core::corpus::load_corpus(repo.join("data/kjv.jsonl")) else {
            println!("no data pack; skipping");
            return;
        };
        let bp = BurstParams::default();
        let want: HashMap<String, Burst> =
            discover_leitworter(&bp, &corpus).into_iter().map(|b| (b.strongs.clone(), b)).collect();

        let mut b = LeitwortBuilder::new(&bp);
        let mut steps = 0;
        while b.step(&corpus, 2048) {
            steps += 1;
            assert!(steps < 100_000, "the builder never finished");
        }
        let got: HashMap<String, Burst> = b
            .take()
            .expect("finished builder yields its discoveries")
            .into_iter()
            .map(|b| (b.strongs.clone(), b))
            .collect();

        assert!(want.len() > 100, "fixture check: the real corpus should yield many leitwörter, got {}", want.len());
        assert!(steps > 2, "the whole thing ran in one step, so nothing was sliced");
        assert_eq!(got.len(), want.len(), "different number of discoveries");
        for (code, w) in &want {
            let g = got.get(code).unwrap_or_else(|| panic!("{code} missing from the sliced discovery"));
            assert_eq!(g, w, "{code} came out different");
        }
    }

    /// The warm loop relies on both halves of this: nothing before the end, and
    /// the result only once.
    #[test]
    fn a_leitwort_builder_yields_nothing_until_it_is_done() {
        let corpus = plumbline_core::corpus::from_str(BURSTY).unwrap();
        let mut b = LeitwortBuilder::new(&BurstParams::default());
        assert!(b.take().is_none(), "nothing before the first step");
        b.step(&corpus, 1);
        assert!(b.take().is_none(), "nothing mid-build");
        let mut guard = 0;
        while b.step(&corpus, 1) {
            guard += 1;
            assert!(guard < 10_000, "step(1) never finished");
        }
        assert!(b.take().is_some());
        assert!(b.take().is_none(), "taken once only");
    }

    /// The worst single leitwort slice, next to the one-shot cost it replaces.
    /// `cargo test --release -p plumbline-rnd -- --ignored --nocapture leitwort_slice_profile`
    #[test]
    #[ignore]
    fn leitwort_slice_profile() {
        use std::time::Instant;
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(corpus) = plumbline_core::corpus::load_corpus(repo.join("data/kjv.jsonl")) else {
            println!("no data pack; skipping");
            return;
        };
        let bp = BurstParams::default();

        let t = Instant::now();
        let _ = discover_leitworter(&bp, &corpus);
        let whole = t.elapsed().as_micros();

        let mut b = LeitwortBuilder::new(&bp);
        let (mut worst, mut steps) = (0u128, 0usize);
        loop {
            let t = Instant::now();
            let more = b.step(&corpus, 2048);
            worst = worst.max(t.elapsed().as_micros());
            steps += 1;
            if !more {
                break;
            }
        }
        println!(
            "leitwort: one shot {:.1}ms | sliced: {steps} slices, worst {:.1}ms",
            whole as f64 / 1000.0,
            worst as f64 / 1000.0
        );
    }
}
