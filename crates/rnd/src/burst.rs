//! Leitwort / burst discovery: which Strong's concepts are *bursty* —
//! deliberately repeated and packed into one narrative stretch (Buber's
//! *Leitwort*: `qara` "call" through creation, `dabar` "word" through
//! Jeremiah) — rather than scattered evenly across the canon.
//!
//! Ported from overlay `Burst.hs`. For each concept with enough occurrences, a
//! scan statistic finds its densest window of occurrences; a Poisson upper-tail
//! probability (via the regularized incomplete gamma, Numerical Recipes §6.2)
//! scores how surprising that packing is under uniform scatter; the tail is
//! Bonferroni-corrected within a concept and Benjamini–Hochberg–gated across
//! concepts. Pure corpus statistics — no ML data.

use std::collections::HashMap;

use plumbline_core::canon;
use plumbline_core::corpus::Corpus;
use plumbline_core::reference::{VRef, OT_NT_DIVIDE};

/// Discovery parameters. Defaults chosen against the real corpus (see the
/// overlay concept-engine notes): occur in ≥8 verses but ≤10% of the testament,
/// cluster ≥4 in the window, at a 1% false-discovery rate.
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

/// `log Γ(x)` for `x > 0` (Lanczos).
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
                Burst { strongs: s.to_string(), score, p_value: p_adj, n, win_start: start, win_end: end, win_count: k, win_span: w as usize },
                p_adj,
            ));
        }
    }

    let mut sig = benjamini_hochberg(bp.fdr_alpha, &scored);
    sig.sort_by(|a, b| b.score.total_cmp(&a.score));
    sig
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
}
