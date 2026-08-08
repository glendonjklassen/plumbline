//! The rendering lens: which English words a Strong's code is translated as,
//! and which codes a given English word translates — both derived from the
//! tagged corpus in a single fold.
//!
//! A lens for readers who don't read Greek/Hebrew on *where the translators
//! made a choice*: selecting "charity" shows that G26 (agape) is elsewhere
//! rendered "love"; selecting "love" reveals it can stand for either G25
//! (agape) or G5368 (phileo). New in Plumbline — there is no overlay
//! antecedent to port from.
//!
//! A **rendering** is a contiguous run of same-code tokens within one verse, so
//! a one-to-many translation like "suffereth long" (← G3114) stays a single
//! unit. An intervening untagged or translator-supplied ([`FLAG_ADDED`]) word
//! breaks the run; a multi-code token extends one run per code independently.
//!
//! [`Renderings::build`] is corpus-parametric — the same index serves any
//! tagged corpus, so the future Luther 1912 pack gets cross-translation
//! rendering comparison for free.

use crate::corpus::{Corpus, FLAG_ADDED};
use crate::reference::VRef;
use std::collections::HashMap;

/// One occurrence of a rendering: the verse and the inclusive token span
/// `[start, end]` of the contiguous same-code run that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderingOcc {
    pub vref: VRef,
    pub span: (u16, u16),
}

/// One rendering of a code, ready for display: a stable label (the most common
/// surface form), the occurrences, and — via [`count`](Rendering::count) — how
/// often the code is translated this way.
#[derive(Debug, Clone)]
struct Rendering {
    /// The most common raw surface form (e.g. `"suffereth long"`), for display.
    label: String,
    /// Every occurrence, in canonical (corpus) order.
    occs: Vec<RenderingOcc>,
}

/// A rendering as handed to callers: the display label, the occurrence count,
/// and a borrow of the occurrences (verse + token span).
#[derive(Debug, Clone, Copy)]
pub struct RenderingView<'a> {
    pub label: &'a str,
    pub count: usize,
    pub occs: &'a [RenderingOcc],
}

/// The rendering-lens indexes, both filled in one corpus fold.
///
/// `by_code` is the forward direction (code → normalized rendering → the
/// rendering), `by_word` the reverse (normalized surface word → code → how many
/// tagged tokens of that word carry the code). Immutable after [`build`] — the
/// corpus never changes at runtime — so it lives beside the other
/// corpus-derived indexes, not behind an authoring lock.
///
/// [`build`]: Renderings::build
#[derive(Debug, Clone, Default)]
pub struct Renderings {
    by_code: HashMap<String, HashMap<String, Rendering>>,
    by_word: HashMap<String, HashMap<String, usize>>,
}

/// Build-time accumulator for one (code, normalized-rendering) pair: raw
/// surface tallies (to pick the display label) plus the occurrences.
#[derive(Default)]
struct Bucket {
    surfaces: HashMap<String, usize>,
    occs: Vec<RenderingOcc>,
}

impl Renderings {
    /// Fold the corpus once, filling both directions. Modeled on
    /// [`OccurrenceIx::build`](crate::strongs::OccurrenceIx::build): one pass
    /// over `corpus.verses_iter()`, postings kept in canonical order.
    pub fn build(corpus: &Corpus) -> Renderings {
        // One code path with the sliced builder, so the two cannot drift.
        let mut b = RenderingsBuilder::default();
        b.feed(corpus, corpus.len());
        b.finish()
    }
}

/// [`Renderings::build`] sliced. The heaviest of the lazily-built indexes
/// (~196ms native, multiples of that in wasm on a phone) and it ran on the
/// reader's FIRST word click, every session — the built lens cannot outlive the
/// tab. Mirrors [`crate::search::SearchIxBuilder`].
#[derive(Default)]
pub struct RenderingsBuilder {
    by_code: HashMap<String, HashMap<String, Bucket>>,
    by_word: HashMap<String, HashMap<String, usize>>,
    /// Next canonical verse ordinal to fold in.
    next: usize,
}

impl RenderingsBuilder {
    /// Fold in up to `n` more verses. Returns true while work remains. Every
    /// run opened inside a verse is also closed inside it, so a slice boundary
    /// can never split one.
    pub fn feed(&mut self, corpus: &Corpus, n: usize) -> bool {
        let end = (self.next + n).min(corpus.len());
        for i in self.next..end {
            let Some(v) = corpus.verse_at(i) else { continue };
            let by_code = &mut self.by_code;
            let by_word = &mut self.by_word;
            let vr = v.vref();
            let last = v.tokens.len().saturating_sub(1) as u16;
            // Runs open on the current token, keyed by code → (start index,
            // surface built so far). Owned keys keep the borrow checker out of
            // the way; only a handful are ever open per verse.
            let mut open: HashMap<String, (u16, String)> = HashMap::new();

            for (i, t) in v.tokens.iter().enumerate() {
                let idx = i as u16;
                let added = t.has_flag(FLAG_ADDED);

                // Reverse index: each distinct code on a tagged, non-added
                // token counts once for the token's normalized surface word.
                if !added && !t.strongs.is_empty() {
                    let nword = normalize(&t.word);
                    if !nword.is_empty() {
                        let mut seen: Vec<&str> = Vec::new();
                        for code in &t.strongs {
                            if seen.contains(&code.as_str()) {
                                continue;
                            }
                            seen.push(code.as_str());
                            let counts = by_word.entry(nword.clone()).or_default();
                            *counts.entry(code.clone()).or_insert(0) += 1;
                        }
                    }
                }

                // Codes that keep a run alive on this token: none for an added
                // or untagged word (which therefore breaks every open run).
                let active: Vec<&str> = if added {
                    Vec::new()
                } else {
                    let mut s: Vec<&str> = Vec::new();
                    for c in &t.strongs {
                        if !s.contains(&c.as_str()) {
                            s.push(c.as_str());
                        }
                    }
                    s
                };

                // Close every open run whose code is absent here; its last
                // token was the previous one (idx - 1, always ≥ 0 since a run
                // can only be open once at least one token has passed).
                let to_close: Vec<String> = open.keys().filter(|k| !active.contains(&k.as_str())).cloned().collect();
                for code in to_close {
                    let (start, surface) = open.remove(&code).unwrap();
                    record(by_code, &code, surface, RenderingOcc { vref: vr.clone(), span: (start, idx - 1) });
                }

                // Extend the still-open runs and open new ones.
                for code in active {
                    match open.get_mut(code) {
                        Some((_, surface)) => {
                            surface.push(' ');
                            surface.push_str(&t.word);
                        }
                        None => {
                            open.insert(code.to_string(), (idx, t.word.clone()));
                        }
                    }
                }
            }

            // Close whatever is still open at verse end.
            for (code, (start, surface)) in open.drain() {
                record(by_code, &code, surface, RenderingOcc { vref: vr.clone(), span: (start, last) });
            }
        }
        self.next = end;
        end < corpus.len()
    }

    /// Everything the fold has seen, finished into a usable lens.
    pub fn finish(self) -> Renderings {
        // Pick each rendering's display label (most common surface, ties broken
        // lexicographically for determinism) and drop the tallies.
        let by_code = self
            .by_code
            .into_iter()
            .map(|(code, inner)| {
                let inner = inner
                    .into_iter()
                    .map(|(norm, b)| (norm, Rendering { label: pick_label(&b.surfaces), occs: b.occs }))
                    .collect();
                (code, inner)
            })
            .collect();

        Renderings { by_code, by_word: self.by_word }
    }
}

impl Renderings {
    /// Every distinct rendering of a code, most frequent first (ties by label).
    pub fn renderings(&self, code: &str) -> Vec<RenderingView<'_>> {
        let mut out: Vec<RenderingView<'_>> = self
            .by_code
            .get(code)
            .into_iter()
            .flatten()
            .map(|(_, r)| RenderingView { label: &r.label, count: r.occs.len(), occs: &r.occs })
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(b.label)));
        out
    }

    /// The codes a surface word translates, most frequent first (ties by code).
    /// The `word` is normalized with the same rule used at build time, so
    /// callers pass a raw surface word (`"Love"`, `"charity,"`) directly.
    pub fn word_codes(&self, word: &str) -> Vec<(&str, usize)> {
        let key = normalize(word);
        let mut out: Vec<(&str, usize)> =
            self.by_word.get(&key).into_iter().flatten().map(|(c, n)| (c.as_str(), *n)).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        out
    }

    /// The occurrences of one specific rendering of a code, in canonical order.
    /// `rendering` may be the display label or any equivalent surface — it is
    /// normalized before lookup — which lets a UI round-trip a chip's label
    /// through a link without carrying the internal key.
    pub fn rendering_occs(&self, code: &str, rendering: &str) -> &[RenderingOcc] {
        let key = normalize(rendering);
        self.by_code.get(code).and_then(|m| m.get(&key)).map(|r| r.occs.as_slice()).unwrap_or(&[])
    }
}

/// Record one closed run under its (code, normalized-rendering) bucket. A run
/// whose surface has no letters at all (pure punctuation) is dropped.
fn record(by_code: &mut HashMap<String, HashMap<String, Bucket>>, code: &str, surface: String, occ: RenderingOcc) {
    let norm = normalize(&surface);
    if norm.is_empty() {
        return;
    }
    let bucket = match by_code.get_mut(code) {
        Some(inner) => inner.entry(norm).or_default(),
        None => by_code.entry(code.to_string()).or_default().entry(norm).or_default(),
    };
    *bucket.surfaces.entry(surface).or_insert(0) += 1;
    bucket.occs.push(occ);
}

/// The label for a rendering: the most common raw surface form, ties broken by
/// the lexicographically smallest string so the choice is deterministic.
fn pick_label(surfaces: &HashMap<String, usize>) -> String {
    surfaces
        .iter()
        .max_by(|(sa, ca), (sb, cb)| ca.cmp(cb).then_with(|| sb.cmp(sa)))
        .map(|(s, _)| s.clone())
        .unwrap_or_default()
}

/// Normalize a surface for grouping: lowercase, letters only, per whitespace
/// word, rejoined with single spaces. So `"Charity,"` and `"charity"` group
/// together, while a multi-word rendering (`"suffereth long"`) keeps its shape.
/// Uses Unicode-aware case folding and `is_alphabetic`, so it carries over to
/// non-English corpora (the future German pack). This is the single source of
/// the rule — every build-time and query-time comparison goes through it.
pub fn normalize(s: &str) -> String {
    s.split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphabetic()).flat_map(|c| c.to_lowercase()).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;

    // One book (1Cor), ascending — enough to exercise every shape: a
    // one-to-many run, a repeated rendering across verses with mixed case, a
    // multi-code token, a FLAG_ADDED word breaking a run (and carrying a stray
    // code that must be ignored), and a word that maps to two codes.
    const SAMPLE: &str = concat!(
        r#"{"format":"x","tokenization":"kjv1769-tok2","verses":6}"#,
        "\n",
        r#"{"b":"1Cor","c":13,"v":1,"t":[["","charity",",",["G26"],0]]}"#,
        "\n",
        r#"{"b":"1Cor","c":13,"v":4,"t":[["","Charity","",["G26"],0],["","suffereth","",["G3114"],0],["","long","",["G3114"],0]]}"#,
        "\n",
        r#"{"b":"1Cor","c":13,"v":5,"t":[["","God","",["H430","H999"],0]]}"#,
        "\n",
        r#"{"b":"1Cor","c":13,"v":6,"t":[["","loved","",["G25"],0],["","the","",["G9999"],1],["","world","",["G25"],0]]}"#,
        "\n",
        r#"{"b":"1Cor","c":13,"v":7,"t":[["","love","",["G25"],0]]}"#,
        "\n",
        r#"{"b":"1Cor","c":13,"v":8,"t":[["","love","",["G5368"],0]]}"#,
    );

    fn build() -> Renderings {
        Renderings::build(&corpus::from_str(SAMPLE).unwrap())
    }

    /// Slicing must not change the answer. Boot feeds this a few hundred verses
    /// at a time; a slice boundary landing mid-verse (or a run left open across
    /// one) would silently produce different renderings from the one-shot build.
    /// Every slice size from 1 upward is checked against the whole-corpus fold.
    #[test]
    fn sliced_build_matches_the_one_shot_build() {
        let corpus = corpus::from_str(SAMPLE).unwrap();
        let whole = Renderings::build(&corpus);
        for n in 1..=corpus.len() + 2 {
            let mut b = RenderingsBuilder::default();
            while b.feed(&corpus, n) {}
            let sliced = b.finish();
            for code in ["G26", "G3114", "G25", "G5368", "H430", "H999"] {
                let a: Vec<_> = whole.renderings(code).iter().map(|r| (r.label, r.count)).collect();
                let c: Vec<_> = sliced.renderings(code).iter().map(|r| (r.label, r.count)).collect();
                assert_eq!(a, c, "slice size {n} changed {code}");
            }
            for word in ["god", "love", "charity"] {
                assert_eq!(whole.word_codes(word), sliced.word_codes(word), "slice {n}, word {word}");
            }
        }
    }

    #[test]
    fn contiguous_run_is_one_rendering() {
        let r = build();
        let g3114 = r.renderings("G3114");
        assert_eq!(g3114.len(), 1);
        assert_eq!(g3114[0].label, "suffereth long");
        assert_eq!(g3114[0].count, 1);
        // Inclusive span across both tokens of the run, in 1Cor 13:4.
        assert_eq!(g3114[0].occs[0].span, (1, 2));
        assert_eq!(g3114[0].occs[0].vref, VRef::new("1Cor", 13, 4));
    }

    #[test]
    fn case_and_punctuation_fold_together() {
        let r = build();
        // "charity," (13:1) and "Charity" (13:4) are one rendering, count 2.
        let g26 = r.renderings("G26");
        assert_eq!(g26.len(), 1, "Charity/charity, must group");
        assert_eq!(g26[0].count, 2);
        assert_eq!(normalize(g26[0].label), "charity");
    }

    #[test]
    fn multi_code_token_extends_every_code() {
        let r = build();
        for code in ["H430", "H999"] {
            let rs = r.renderings(code);
            assert_eq!(rs.len(), 1, "{code} rendered once");
            assert_eq!(normalize(rs[0].label), "god");
            assert_eq!(rs[0].occs[0].span, (0, 0));
        }
    }

    #[test]
    fn added_or_untagged_word_breaks_the_run() {
        let r = build();
        // G25 renders "loved" and "world" as SEPARATE runs (the FLAG_ADDED
        // "the" between them breaks the run), plus "love" from 13:7.
        let mut labels: Vec<&str> = r.renderings("G25").iter().map(|v| v.label).collect();
        labels.sort();
        assert_eq!(labels, ["love", "loved", "world"]);
        // Each is a single-token span.
        for v in r.renderings("G25") {
            assert_eq!(v.count, 1);
            assert_eq!(v.occs[0].span.0, v.occs[0].span.1);
        }
    }

    #[test]
    fn flag_added_token_is_ignored_even_with_a_code() {
        let r = build();
        // The added "the" carried a stray G9999 — it must not index a run…
        assert!(r.renderings("G9999").is_empty());
        // …nor appear in the reverse index for "the".
        assert!(r.word_codes("the").is_empty());
    }

    #[test]
    fn reverse_index_reveals_the_split() {
        let r = build();
        // "love" stands for both G25 (13:7) and G5368 (13:8).
        assert_eq!(r.word_codes("love"), vec![("G25", 1), ("G5368", 1)]);
        // Query normalization: caller may pass a raw surface form.
        assert_eq!(r.word_codes("Love"), r.word_codes("love"));
    }

    #[test]
    fn rendering_occs_filters_by_rendering() {
        let r = build();
        // Look up by the display label; normalization handles the round-trip.
        let occs = r.rendering_occs("G25", "loved");
        assert_eq!(occs.len(), 1);
        assert_eq!(occs[0].vref, VRef::new("1Cor", 13, 6));
        assert_eq!(occs[0].span, (0, 0));
        assert!(r.rendering_occs("G25", "nope").is_empty());
    }
}
