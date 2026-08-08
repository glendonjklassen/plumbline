//! Classic Strong's (1890) dictionary entries and the concordance index.
//!
//! Ported from overlay `Strongs.hs`. The occurrence index is derived purely
//! from the tagged text — no external cross-reference dataset.

use crate::corpus::Corpus;
use crate::reference::VRef;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

/// One 1890 dictionary entry. Every field is optional — the source data leaves
/// gaps. Ported from `StrongsEntry`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrongsEntry {
    pub lemma: Option<String>,
    pub xlit: Option<String>,
    pub pron: Option<String>,
    #[serde(rename = "derivation")]
    pub deriv: Option<String>,
    #[serde(rename = "strongs_def")]
    pub def: Option<String>,
    #[serde(rename = "kjv_def")]
    pub kjv: Option<String>,
}

/// The whole dictionary: Strong's ref (`"H7225"`) → entry.
pub type StrongsDict = HashMap<String, StrongsEntry>;

/// Load the merged Hebrew+Greek dictionary from `strongs.json`.
pub fn load_strongs(path: impl AsRef<Path>) -> Result<StrongsDict, Error> {
    let path = path.as_ref();
    let raw = std::fs::read(path).map_err(|e| Error::Io { path: path.display().to_string(), source: e })?;
    serde_json::from_slice(&raw).map_err(|e| Error::Parse(format!("could not parse {}: {e}", path.display())))
}

/// Strong's ref → the verses containing it, in canonical (file) order.
/// Ported from `OccurrenceIx`.
#[derive(Debug, Clone, Default)]
pub struct OccurrenceIx {
    map: HashMap<String, Vec<VRef>>,
}

/// [`OccurrenceIx::build`] sliced. The web builds this on ONE worker thread
/// that also answers layout and taps, and it used to run whole on the reader's
/// first word click — every session, because the built index cannot outlive the
/// tab. Fed in slices, boot can warm it between yields.
/// Mirrors [`crate::search::SearchIxBuilder`].
#[derive(Debug, Default)]
pub struct OccurrenceIxBuilder {
    map: HashMap<String, Vec<VRef>>,
    /// Next canonical verse ordinal to fold in.
    next: usize,
}

impl OccurrenceIxBuilder {
    /// Fold in up to `n` more verses. Returns true while work remains.
    pub fn feed(&mut self, corpus: &Corpus, n: usize) -> bool {
        let end = (self.next + n).min(corpus.len());
        for i in self.next..end {
            let Some(v) = corpus.verse_at(i) else { continue };
            let refs: BTreeSet<&str> = v.tokens.iter().flat_map(|t| t.strongs.iter().map(String::as_str)).collect();
            let vr = v.vref();
            for r in refs {
                // Allocate the key String only on first sight of a code (~14k
                // distinct) rather than once per (verse, code) pair (~10^5–10^6).
                match self.map.get_mut(r) {
                    Some(postings) => postings.push(vr.clone()),
                    None => {
                        self.map.insert(r.to_string(), vec![vr.clone()]);
                    }
                }
            }
        }
        self.next = end;
        end < corpus.len()
    }

    pub fn finish(self) -> OccurrenceIx {
        OccurrenceIx { map: self.map }
    }
}

impl OccurrenceIx {
    /// Build the index in one fold over the corpus. Ported from
    /// `occurrenceIndex`: within a verse, each distinct Strong's ref counts
    /// once; postings stay in canonical order.
    pub fn build(corpus: &Corpus) -> Self {
        // One code path with the sliced builder, so the two cannot drift.
        let mut b = OccurrenceIxBuilder::default();
        b.feed(corpus, corpus.len());
        b.finish()
    }

    /// The verses carrying a Strong's ref, in canonical order.
    pub fn verses(&self, code: &str) -> &[VRef] {
        self.map.get(code).map(Vec::as_slice).unwrap_or(&[])
    }

    /// How many verses carry a Strong's ref.
    pub fn count(&self, code: &str) -> usize {
        self.map.get(code).map_or(0, Vec::len)
    }

    /// The verses containing BOTH codes — the intersection of their postings,
    /// kept in the first code's canonical order. Its length is the pair's
    /// co-occurrence count. Ported from `sharedVersesOf`.
    pub fn shared_verses(&self, a: &str, b: &str) -> Vec<VRef> {
        let bs: HashSet<&VRef> = self.map.get(b).into_iter().flatten().collect();
        self.map.get(a).into_iter().flatten().filter(|v| bs.contains(v)).cloned().collect()
    }

    /// Every indexed Strong's ref.
    pub fn codes(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}

/// Capitalized first words that are *not* a proper-noun tell — ordinary
/// English pronouns/interjections that open a definition sentence. Ported
/// verbatim from `nonNameCapitalizedWords`.
const NON_NAME_CAPITALIZED: &[&str] = &[
    "I", "THOU", "HE", "SHE", "WE", "YE", "THEY", "THIS", "THAT", "THESE", "THOSE", "O", "OH", "AH", "ALAS", "LO",
    "BEHOLD", "WOE", "YEA", "NAY", "AMEN",
];

/// Whether a Strong's entry names a proper noun (person/place/people/demonym)
/// rather than a common concept. Nothing tags part of speech, so this reads
/// two textual tells from the 1890 prose. Ported from `isProperNoun`.
///
/// Only ever decides which of two *display* tiers a keyness row falls into —
/// never whether a concept counts as key — so approximation is fine.
pub fn is_proper_noun(e: &StrongsEntry) -> bool {
    if let Some(sd) = &e.def {
        if let Some(w) = first_alpha_word(sd) {
            let upper = w.to_uppercase();
            if starts_upper(&w) && !NON_NAME_CAPITALIZED.contains(&upper.as_str()) {
                return true;
            }
        }
    }
    if let Some(kd) = &e.kjv {
        return name_like_list(kd);
    }
    false
}

/// First word of `t` reduced to its alphabetic characters, if it has any.
fn first_alpha_word(t: &str) -> Option<String> {
    let w = t.split_whitespace().next()?;
    let alpha: String = w.chars().filter(|c| c.is_alphabetic()).collect();
    if alpha.is_empty() {
        None
    } else {
        Some(alpha)
    }
}

fn starts_upper(w: &str) -> bool {
    w.chars().next().is_some_and(|c| c.is_uppercase())
}

/// The kjv-renderings field is nothing but capitalized words / an `,`-or-`or`
/// joined list of them.
fn name_like_list(raw: &str) -> bool {
    let cleaned = raw.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() {
        return false;
    }
    cleaned.split(',').all(segment_is_names)
}

fn segment_is_names(seg: &str) -> bool {
    let words: Vec<&str> = seg.split_whitespace().filter(|w| *w != "or").collect();
    !words.is_empty() && words.iter().all(|w| capitalized_word(w))
}

fn capitalized_word(w: &str) -> bool {
    let alpha: String = w.chars().filter(|c| c.is_alphabetic()).collect();
    match alpha.chars().next() {
        Some(c) => c.is_uppercase() && !NON_NAME_CAPITALIZED.contains(&alpha.to_uppercase().as_str()),
        None => false,
    }
}

#[cfg(test)]
mod tests {

    /// The sliced fold must equal the one-shot fold at every slice size —
    /// postings stay in canonical order and no verse is counted twice.
    #[test]
    fn sliced_occurrence_build_matches_the_one_shot_build() {
        const SAMPLE: &str = concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":3}"#,
            "\n",
            r#"{"b":"Gen","c":1,"v":1,"t":[["","In","",["H7225"],0],["","God","",["H430"],0]]}"#,
            "\n",
            r#"{"b":"Gen","c":1,"v":2,"t":[["","God","",["H430"],0],["","moved","",["H7363"],0]]}"#,
            "\n",
            r#"{"b":"Gen","c":1,"v":3,"t":[["","God","",["H430","H430"],0]]}"#,
        );
        let corpus = corpus::from_str(SAMPLE).unwrap();
        let whole = OccurrenceIx::build(&corpus);
        for n in 1..=corpus.len() + 2 {
            let mut b = OccurrenceIxBuilder::default();
            while b.feed(&corpus, n) {}
            let sliced = b.finish();
            for code in ["H430", "H7225", "H7363", "H9999"] {
                assert_eq!(whole.verses(code), sliced.verses(code), "slice {n} changed {code}");
            }
        }
        // A code repeated within one verse still counts that verse once.
        assert_eq!(whole.verses("H430").len(), 3);
    }

    use super::*;
    use crate::corpus;

    const SAMPLE: &str = concat!(
        r#"{"format":"x","tokenization":"kjv1769-tok2","verses":3}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","God","",["H430"],0],["","created","",["H1254"],0]],"v":1}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","God","",["H430"],0]],"v":2}"#,
        "\n",
        r#"{"b":"John","c":1,"t":[["","God","",["H430"],0],["","created","",["H1254"],0]],"v":1}"#,
    );

    #[test]
    fn occurrence_index_counts_and_shares() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = OccurrenceIx::build(&c);
        assert_eq!(ix.count("H430"), 3);
        assert_eq!(ix.count("H1254"), 2);
        // shared verses of H430 & H1254: Gen 1:1 and John 1:1
        let shared = ix.shared_verses("H430", "H1254");
        assert_eq!(shared, vec![VRef::new("Gen", 1, 1), VRef::new("John", 1, 1)]);
    }

    #[test]
    fn proper_noun_heuristic() {
        let name = StrongsEntry { def: Some("Nob, a place in Palestine".into()), ..Default::default() };
        assert!(is_proper_noun(&name));

        let common = StrongsEntry { def: Some("to drive (an animal, chariot)".into()), ..Default::default() };
        assert!(!is_proper_noun(&common));

        let pronoun = StrongsEntry { def: Some("I exist".into()), ..Default::default() };
        assert!(!is_proper_noun(&pronoun));

        // name detected via the kjv-renderings list when the def leads common.
        let via_kjv = StrongsEntry {
            def: Some("a Gadarene or inhabitant of Gadara".into()),
            kjv: Some("Gadarene".into()),
            ..Default::default()
        };
        assert!(is_proper_noun(&via_kjv));
    }

    /// The 1890 prose is not uniformly trimmed, and the first *word* is what the
    /// heuristic reads — not the first character. `split_whitespace` already
    /// skips the leading run, which is why [`first_alpha_word`] does not trim;
    /// a splitter that doesn't would hand back an empty first field and every
    /// indented entry would stop looking like a name.
    #[test]
    fn proper_noun_reads_past_leading_whitespace() {
        let indented = StrongsEntry { def: Some("  \n\tNob, a place in Palestine".into()), ..Default::default() };
        assert!(is_proper_noun(&indented));

        let indented_common =
            StrongsEntry { def: Some("   to drive (an animal, chariot)".into()), ..Default::default() };
        assert!(!is_proper_noun(&indented_common));
    }

    #[test]
    fn strongs_entry_json_field_names() {
        let json = r#"{"lemma":"רֵאשִׁית","xlit":"rêʼshîyth","strongs_def":"the first","kjv_def":"beginning"}"#;
        let e: StrongsEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.lemma.as_deref(), Some("רֵאשִׁית"));
        assert_eq!(e.def.as_deref(), Some("the first"));
        assert_eq!(e.kjv.as_deref(), Some("beginning"));
        assert_eq!(e.deriv, None);
    }
}
