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
    parse_strongs(&raw).map_err(|e| Error::Parse(format!("could not parse {}: {e}", path.display())))
}

/// Parse a dictionary from its JSON bytes — the one way in, so every dictionary
/// the app opens (a file, a shell's bundled bytes, a localized one) gets the
/// same finishing: [`fill_greek_xlit`].
pub fn parse_strongs(raw: &[u8]) -> Result<StrongsDict, serde_json::Error> {
    let mut dict: StrongsDict = serde_json::from_slice(raw)?;
    fill_greek_xlit(&mut dict);
    Ok(dict)
}

/// Give every Greek entry a transliteration. The 1890 data carries one for each
/// Hebrew entry and for no Greek one, so the Greek half is derived from the lemma
/// ([`romanize_greek`]) once at load — and only where the file has none, so a
/// dictionary that ships its own keeps it.
pub fn fill_greek_xlit(dict: &mut StrongsDict) {
    for (code, e) in dict.iter_mut() {
        if e.xlit.is_none() && code.starts_with('G') {
            if let Some(l) = &e.lemma {
                let r = romanize_greek(l);
                if !r.is_empty() {
                    e.xlit = Some(r);
                }
            }
        }
    }
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

// ── derivation families ──────────────────────────────────────────────────────

/// Codes grouped by the root their 1890 derivation prose leads to — the
/// dictionary's own "same stem". δαιμόνιον (G1140) is "neuter of a derivative
/// of G1142", δαιμονίζομαι (G1139) "middle voice from G1142", so both sit under
/// δαίμων with it; a reader studying "devils" is owed "devil" and "possessed
/// with devils" too, the way the surface lens answers "ruler" for "rulers".
///
/// A step follows the ONE same-language code a derivation names ("from
/// G1142"). Nothing after `compare` counts — that is a cross-reference, not a
/// parent. A compound naming two ("from G1223 and G906") is its own root,
/// unless every part already leads to one root (G1141, "from G1140 and G1142",
/// joins δαίμων): following a compound up to its parts would file the devil
/// (διάβολος) under "to throw" (βάλλω) with every other -βάλλω word. A
/// cross-language origin ("of Hebrew origin (H03091)") is never followed — the
/// bridge tier owns the testaments. Probable derivations ("probably from") are
/// followed: this groups for exploration, and every line shows its own word.
#[derive(Debug, Clone, Default)]
pub struct Families {
    /// Every code → the root it resolves to (itself, when it is one).
    root_of: HashMap<String, String>,
    /// Root → every member, `root` included, in code order.
    members: HashMap<String, Vec<String>>,
}

impl Families {
    /// Resolve every entry's root. One pass over the dictionary, memoized, so a
    /// chain is walked once however many codes hang off it.
    pub fn build(dict: &StrongsDict) -> Families {
        let refs: HashMap<&str, Vec<String>> = dict
            .iter()
            .map(|(code, e)| (code.as_str(), e.deriv.as_deref().map(|d| derivation_refs(code, d)).unwrap_or_default()))
            .collect();
        let mut root_of: HashMap<String, String> = HashMap::with_capacity(dict.len());
        let mut visiting: Vec<String> = Vec::new();
        for code in dict.keys() {
            root(code, &refs, &mut root_of, &mut visiting);
        }
        let mut members: HashMap<String, Vec<String>> = HashMap::new();
        for (code, r) in &root_of {
            members.entry(r.clone()).or_default().push(code.clone());
        }
        for v in members.values_mut() {
            v.sort_by_key(|c| code_order(c));
        }
        Families { root_of, members }
    }

    /// Every code sharing `code`'s root, `code` included, in code order. Just
    /// `[code]` for one with no relatives, or one the dictionary does not have.
    pub fn family(&self, code: &str) -> Vec<String> {
        self.root_of.get(code).and_then(|r| self.members.get(r)).cloned().unwrap_or_else(|| vec![code.to_string()])
    }
}

/// The root `code` resolves to: itself when its derivation names no usable
/// parent, else its parent's root. `visiting` breaks a cycle (two entries
/// deriving from each other) by making the first one met the root.
fn root(
    code: &str,
    refs: &HashMap<&str, Vec<String>>,
    memo: &mut HashMap<String, String>,
    visiting: &mut Vec<String>,
) -> String {
    if let Some(r) = memo.get(code) {
        return r.clone();
    }
    if visiting.iter().any(|v| v == code) {
        return code.to_string();
    }
    visiting.push(code.to_string());
    // Only parents the dictionary has: a reference to a missing entry cannot be
    // walked, and the code stands as its own root.
    let parents: Vec<&str> = refs
        .get(code)
        .map(|v| v.iter().map(String::as_str).filter(|p| refs.contains_key(p)).collect())
        .unwrap_or_default();
    let r = match parents.as_slice() {
        [] => code.to_string(),
        [one] => root(one, refs, memo, visiting),
        many => {
            let mut roots = many.iter().map(|p| root(p, refs, memo, visiting));
            let first = roots.next().unwrap_or_else(|| code.to_string());
            if roots.all(|r| r == first) {
                first
            } else {
                code.to_string()
            }
        }
    };
    visiting.pop();
    memo.insert(code.to_string(), r.clone());
    r
}

/// The same-language codes a derivation names, in order, none twice, stopping
/// at the first `compare`. Zero-padded forms (`H03091`) are normalized to the
/// dictionary's keys. `code` itself is never listed.
pub fn derivation_refs(code: &str, deriv: &str) -> Vec<String> {
    let lang = code.chars().next().unwrap_or('?');
    let lower = deriv.to_ascii_lowercase();
    let scan = match lower.find("compare") {
        Some(i) => &deriv[..i],
        None => deriv,
    };
    let mut out: Vec<String> = Vec::new();
    let bytes = scan.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let boundary = i == 0 || !(bytes[i - 1] as char).is_ascii_alphanumeric();
        if boundary && (c == 'G' || c == 'H') {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && (j == bytes.len() || !(bytes[j] as char).is_ascii_alphanumeric()) {
                let digits = scan[i + 1..j].trim_start_matches('0');
                if !digits.is_empty() && c == lang {
                    let r = format!("{c}{digits}");
                    if r != code && !out.contains(&r) {
                        out.push(r);
                    }
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Code order: language letter, then the number — so G25 sorts before G5368.
fn code_order(code: &str) -> (char, u32) {
    let mut ch = code.chars();
    let l = ch.next().unwrap_or('?');
    (l, ch.as_str().parse().unwrap_or(u32::MAX))
}

// ── Greek transliteration ────────────────────────────────────────────────────

/// A Greek lemma romanized, for a reader who does not read the script — the
/// SBL general-purpose scheme: ē and ō for eta and omega; th, ph, ch, ps; a
/// rough breathing as h (before the whole diphthong when it sits on the second
/// vowel: οὗτος → houtos); gamma before a velar as n (ἄγγελος → angelos); upsilon
/// as u inside a diphthong and y alone (ψυχή → psychē); ῥ as rh. Accents, iota
/// subscripts and diaereses are dropped. Anything that is not a Greek letter
/// passes through unchanged.
///
/// Recognisable, not scholarly: it is the word beside the letters, so someone
/// who cannot read δαίμων can still say "daimōn".
pub fn romanize_greek(lemma: &str) -> String {
    let letters: Vec<(char, Option<(char, bool)>)> = lemma.chars().map(|c| (c, greek_base(c))).collect();
    let lower_at = |i: usize| -> Option<char> {
        letters.get(i).and_then(|(_, g)| g.map(|(b, _)| b.to_lowercase().next().unwrap_or(b)))
    };
    let mut out: Vec<String> = Vec::with_capacity(letters.len());
    for (i, (orig, g)) in letters.iter().enumerate() {
        let Some((base, rough)) = *g else {
            out.push(orig.to_string());
            continue;
        };
        let lower = base.to_lowercase().next().unwrap_or(base);
        let upper = base.is_uppercase();
        let prev = if i > 0 { lower_at(i - 1) } else { None };
        let next = lower_at(i + 1);
        let s = match lower {
            'α' => "a",
            'β' => "b",
            'γ' => match next {
                Some('γ' | 'κ' | 'ξ' | 'χ') => "n",
                _ => "g",
            },
            'δ' => "d",
            'ε' => "e",
            'ζ' => "z",
            'η' => "ē",
            'θ' => "th",
            'ι' => "i",
            'κ' => "k",
            'λ' => "l",
            'μ' => "m",
            'ν' => "n",
            'ξ' => "x",
            'ο' => "o",
            'π' => "p",
            'ρ' => {
                if rough {
                    "rh"
                } else {
                    "r"
                }
            }
            'σ' | 'ς' | 'ϲ' => "s",
            'τ' => "t",
            // u as either half of a diphthong (αυ ευ ηυ ου, and υι: υἱός →
            // huios), y on its own.
            'υ' => match (prev, next) {
                (Some('α' | 'ε' | 'η' | 'ο' | 'ω'), _) | (_, Some('ι')) => "u",
                _ => "y",
            },
            'φ' => "ph",
            'χ' => "ch",
            'ψ' => "ps",
            'ω' => "ō",
            'ϝ' => "w",
            'ϛ' => "st",
            'ϟ' => "q",
            'ϡ' => "ss",
            _ => {
                out.push(orig.to_string());
                continue;
            }
        };
        let mut s = if upper { capitalize(s) } else { s.to_string() };
        let is_vowel = matches!(lower, 'α' | 'ε' | 'η' | 'ι' | 'ο' | 'υ' | 'ω');
        if rough && is_vowel {
            // On the second vowel of a diphthong the breathing belongs to the pair.
            let diphthong =
                matches!((prev, lower), (Some('α' | 'ε' | 'ο' | 'υ'), 'ι') | (Some('α' | 'ε' | 'η' | 'ο'), 'υ'));
            if diphthong && !out.is_empty() {
                let first = out.pop().unwrap_or_default();
                out.push(aspirate(&first));
            } else {
                s = aspirate(&s);
            }
        }
        out.push(s);
    }
    out.concat()
}

/// `h` in front, keeping the word's own capital: Ἑβραῖος → Hebraios.
fn aspirate(s: &str) -> String {
    match s.chars().next() {
        Some(c) if c.is_uppercase() => format!("H{}", s.to_lowercase()),
        _ => format!("h{s}"),
    }
}

fn capitalize(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(c) => c.to_uppercase().chain(ch).collect(),
        None => String::new(),
    }
}

/// The letter under a Greek character and whether it carries a rough breathing:
/// `(base, rough)` for any Greek letter, marked or plain; `None` for anything
/// else. The marked forms are the table below; a plain letter is itself.
fn greek_base(c: char) -> Option<(char, bool)> {
    if let Ok(i) = GREEK_MARKED.binary_search_by_key(&c, |(k, _, _)| *k) {
        let (_, base, rough) = GREEK_MARKED[i];
        return Some((base, rough));
    }
    (('Ͱ'..='Ͽ').contains(&c) && c.is_alphabetic()).then_some((c, false))
}

/// Every precomposed Greek letter carrying a diacritic (the basic and extended
/// blocks), with the letter under it and whether the mark is a rough breathing —
/// Unicode's own canonical decompositions, tabulated once so the core needs no
/// normalization crate. Sorted by code point for the binary search above.
const GREEK_MARKED: &[(char, char, bool)] = &[
    ('\u{0386}', 'Α', false),
    ('\u{0388}', 'Ε', false),
    ('\u{0389}', 'Η', false),
    ('\u{038a}', 'Ι', false),
    ('\u{038c}', 'Ο', false),
    ('\u{038e}', 'Υ', false),
    ('\u{038f}', 'Ω', false),
    ('\u{0390}', 'ι', false),
    ('\u{03aa}', 'Ι', false),
    ('\u{03ab}', 'Υ', false),
    ('\u{03ac}', 'α', false),
    ('\u{03ad}', 'ε', false),
    ('\u{03ae}', 'η', false),
    ('\u{03af}', 'ι', false),
    ('\u{03b0}', 'υ', false),
    ('\u{03ca}', 'ι', false),
    ('\u{03cb}', 'υ', false),
    ('\u{03cc}', 'ο', false),
    ('\u{03cd}', 'υ', false),
    ('\u{03ce}', 'ω', false),
    ('\u{03d3}', 'ϒ', false),
    ('\u{03d4}', 'ϒ', false),
    ('\u{1f00}', 'α', false),
    ('\u{1f01}', 'α', true),
    ('\u{1f02}', 'α', false),
    ('\u{1f03}', 'α', true),
    ('\u{1f04}', 'α', false),
    ('\u{1f05}', 'α', true),
    ('\u{1f06}', 'α', false),
    ('\u{1f07}', 'α', true),
    ('\u{1f08}', 'Α', false),
    ('\u{1f09}', 'Α', true),
    ('\u{1f0a}', 'Α', false),
    ('\u{1f0b}', 'Α', true),
    ('\u{1f0c}', 'Α', false),
    ('\u{1f0d}', 'Α', true),
    ('\u{1f0e}', 'Α', false),
    ('\u{1f0f}', 'Α', true),
    ('\u{1f10}', 'ε', false),
    ('\u{1f11}', 'ε', true),
    ('\u{1f12}', 'ε', false),
    ('\u{1f13}', 'ε', true),
    ('\u{1f14}', 'ε', false),
    ('\u{1f15}', 'ε', true),
    ('\u{1f18}', 'Ε', false),
    ('\u{1f19}', 'Ε', true),
    ('\u{1f1a}', 'Ε', false),
    ('\u{1f1b}', 'Ε', true),
    ('\u{1f1c}', 'Ε', false),
    ('\u{1f1d}', 'Ε', true),
    ('\u{1f20}', 'η', false),
    ('\u{1f21}', 'η', true),
    ('\u{1f22}', 'η', false),
    ('\u{1f23}', 'η', true),
    ('\u{1f24}', 'η', false),
    ('\u{1f25}', 'η', true),
    ('\u{1f26}', 'η', false),
    ('\u{1f27}', 'η', true),
    ('\u{1f28}', 'Η', false),
    ('\u{1f29}', 'Η', true),
    ('\u{1f2a}', 'Η', false),
    ('\u{1f2b}', 'Η', true),
    ('\u{1f2c}', 'Η', false),
    ('\u{1f2d}', 'Η', true),
    ('\u{1f2e}', 'Η', false),
    ('\u{1f2f}', 'Η', true),
    ('\u{1f30}', 'ι', false),
    ('\u{1f31}', 'ι', true),
    ('\u{1f32}', 'ι', false),
    ('\u{1f33}', 'ι', true),
    ('\u{1f34}', 'ι', false),
    ('\u{1f35}', 'ι', true),
    ('\u{1f36}', 'ι', false),
    ('\u{1f37}', 'ι', true),
    ('\u{1f38}', 'Ι', false),
    ('\u{1f39}', 'Ι', true),
    ('\u{1f3a}', 'Ι', false),
    ('\u{1f3b}', 'Ι', true),
    ('\u{1f3c}', 'Ι', false),
    ('\u{1f3d}', 'Ι', true),
    ('\u{1f3e}', 'Ι', false),
    ('\u{1f3f}', 'Ι', true),
    ('\u{1f40}', 'ο', false),
    ('\u{1f41}', 'ο', true),
    ('\u{1f42}', 'ο', false),
    ('\u{1f43}', 'ο', true),
    ('\u{1f44}', 'ο', false),
    ('\u{1f45}', 'ο', true),
    ('\u{1f48}', 'Ο', false),
    ('\u{1f49}', 'Ο', true),
    ('\u{1f4a}', 'Ο', false),
    ('\u{1f4b}', 'Ο', true),
    ('\u{1f4c}', 'Ο', false),
    ('\u{1f4d}', 'Ο', true),
    ('\u{1f50}', 'υ', false),
    ('\u{1f51}', 'υ', true),
    ('\u{1f52}', 'υ', false),
    ('\u{1f53}', 'υ', true),
    ('\u{1f54}', 'υ', false),
    ('\u{1f55}', 'υ', true),
    ('\u{1f56}', 'υ', false),
    ('\u{1f57}', 'υ', true),
    ('\u{1f59}', 'Υ', true),
    ('\u{1f5b}', 'Υ', true),
    ('\u{1f5d}', 'Υ', true),
    ('\u{1f5f}', 'Υ', true),
    ('\u{1f60}', 'ω', false),
    ('\u{1f61}', 'ω', true),
    ('\u{1f62}', 'ω', false),
    ('\u{1f63}', 'ω', true),
    ('\u{1f64}', 'ω', false),
    ('\u{1f65}', 'ω', true),
    ('\u{1f66}', 'ω', false),
    ('\u{1f67}', 'ω', true),
    ('\u{1f68}', 'Ω', false),
    ('\u{1f69}', 'Ω', true),
    ('\u{1f6a}', 'Ω', false),
    ('\u{1f6b}', 'Ω', true),
    ('\u{1f6c}', 'Ω', false),
    ('\u{1f6d}', 'Ω', true),
    ('\u{1f6e}', 'Ω', false),
    ('\u{1f6f}', 'Ω', true),
    ('\u{1f70}', 'α', false),
    ('\u{1f71}', 'α', false),
    ('\u{1f72}', 'ε', false),
    ('\u{1f73}', 'ε', false),
    ('\u{1f74}', 'η', false),
    ('\u{1f75}', 'η', false),
    ('\u{1f76}', 'ι', false),
    ('\u{1f77}', 'ι', false),
    ('\u{1f78}', 'ο', false),
    ('\u{1f79}', 'ο', false),
    ('\u{1f7a}', 'υ', false),
    ('\u{1f7b}', 'υ', false),
    ('\u{1f7c}', 'ω', false),
    ('\u{1f7d}', 'ω', false),
    ('\u{1f80}', 'α', false),
    ('\u{1f81}', 'α', true),
    ('\u{1f82}', 'α', false),
    ('\u{1f83}', 'α', true),
    ('\u{1f84}', 'α', false),
    ('\u{1f85}', 'α', true),
    ('\u{1f86}', 'α', false),
    ('\u{1f87}', 'α', true),
    ('\u{1f88}', 'Α', false),
    ('\u{1f89}', 'Α', true),
    ('\u{1f8a}', 'Α', false),
    ('\u{1f8b}', 'Α', true),
    ('\u{1f8c}', 'Α', false),
    ('\u{1f8d}', 'Α', true),
    ('\u{1f8e}', 'Α', false),
    ('\u{1f8f}', 'Α', true),
    ('\u{1f90}', 'η', false),
    ('\u{1f91}', 'η', true),
    ('\u{1f92}', 'η', false),
    ('\u{1f93}', 'η', true),
    ('\u{1f94}', 'η', false),
    ('\u{1f95}', 'η', true),
    ('\u{1f96}', 'η', false),
    ('\u{1f97}', 'η', true),
    ('\u{1f98}', 'Η', false),
    ('\u{1f99}', 'Η', true),
    ('\u{1f9a}', 'Η', false),
    ('\u{1f9b}', 'Η', true),
    ('\u{1f9c}', 'Η', false),
    ('\u{1f9d}', 'Η', true),
    ('\u{1f9e}', 'Η', false),
    ('\u{1f9f}', 'Η', true),
    ('\u{1fa0}', 'ω', false),
    ('\u{1fa1}', 'ω', true),
    ('\u{1fa2}', 'ω', false),
    ('\u{1fa3}', 'ω', true),
    ('\u{1fa4}', 'ω', false),
    ('\u{1fa5}', 'ω', true),
    ('\u{1fa6}', 'ω', false),
    ('\u{1fa7}', 'ω', true),
    ('\u{1fa8}', 'Ω', false),
    ('\u{1fa9}', 'Ω', true),
    ('\u{1faa}', 'Ω', false),
    ('\u{1fab}', 'Ω', true),
    ('\u{1fac}', 'Ω', false),
    ('\u{1fad}', 'Ω', true),
    ('\u{1fae}', 'Ω', false),
    ('\u{1faf}', 'Ω', true),
    ('\u{1fb0}', 'α', false),
    ('\u{1fb1}', 'α', false),
    ('\u{1fb2}', 'α', false),
    ('\u{1fb3}', 'α', false),
    ('\u{1fb4}', 'α', false),
    ('\u{1fb6}', 'α', false),
    ('\u{1fb7}', 'α', false),
    ('\u{1fb8}', 'Α', false),
    ('\u{1fb9}', 'Α', false),
    ('\u{1fba}', 'Α', false),
    ('\u{1fbb}', 'Α', false),
    ('\u{1fbc}', 'Α', false),
    ('\u{1fbe}', 'ι', false),
    ('\u{1fc2}', 'η', false),
    ('\u{1fc3}', 'η', false),
    ('\u{1fc4}', 'η', false),
    ('\u{1fc6}', 'η', false),
    ('\u{1fc7}', 'η', false),
    ('\u{1fc8}', 'Ε', false),
    ('\u{1fc9}', 'Ε', false),
    ('\u{1fca}', 'Η', false),
    ('\u{1fcb}', 'Η', false),
    ('\u{1fcc}', 'Η', false),
    ('\u{1fd0}', 'ι', false),
    ('\u{1fd1}', 'ι', false),
    ('\u{1fd2}', 'ι', false),
    ('\u{1fd3}', 'ι', false),
    ('\u{1fd6}', 'ι', false),
    ('\u{1fd7}', 'ι', false),
    ('\u{1fd8}', 'Ι', false),
    ('\u{1fd9}', 'Ι', false),
    ('\u{1fda}', 'Ι', false),
    ('\u{1fdb}', 'Ι', false),
    ('\u{1fe0}', 'υ', false),
    ('\u{1fe1}', 'υ', false),
    ('\u{1fe2}', 'υ', false),
    ('\u{1fe3}', 'υ', false),
    ('\u{1fe4}', 'ρ', false),
    ('\u{1fe5}', 'ρ', true),
    ('\u{1fe6}', 'υ', false),
    ('\u{1fe7}', 'υ', false),
    ('\u{1fe8}', 'Υ', false),
    ('\u{1fe9}', 'Υ', false),
    ('\u{1fea}', 'Υ', false),
    ('\u{1feb}', 'Υ', false),
    ('\u{1fec}', 'Ρ', true),
    ('\u{1ff2}', 'ω', false),
    ('\u{1ff3}', 'ω', false),
    ('\u{1ff4}', 'ω', false),
    ('\u{1ff6}', 'ω', false),
    ('\u{1ff7}', 'ω', false),
    ('\u{1ff8}', 'Ο', false),
    ('\u{1ff9}', 'Ο', false),
    ('\u{1ffa}', 'Ω', false),
    ('\u{1ffb}', 'Ω', false),
    ('\u{1ffc}', 'Ω', false),
];

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
    fn dict(entries: &[(&str, &str)]) -> StrongsDict {
        entries
            .iter()
            .map(|(c, d)| (c.to_string(), StrongsEntry { deriv: Some(d.to_string()), ..Default::default() }))
            .collect()
    }

    #[test]
    fn derivation_refs_read_the_prose() {
        // Ordinary, zero-padded, self-reference dropped, dedupe, and the compare tail cut.
        assert_eq!(derivation_refs("G1140", "neuter of a derivative of G1142 (δαίμων);"), ["G1142"]);
        assert_eq!(derivation_refs("H3091", "from H03068 (יְהֹוָה) and H3467 (יָשַׁע); H3091"), ["H3068", "H3467"]);
        assert_eq!(
            derivation_refs("H2616", "a primitive root; (compare H2603 (חָנַן)); (compare H1288)"),
            Vec::<String>::new()
        );
        assert_eq!(derivation_refs("G25", "perhaps from (much) (or compare G5689);"), Vec::<String>::new());
        // Cross-language origins are not derivations for this purpose.
        assert_eq!(derivation_refs("G2424", "of Hebrew origin (H03091);"), Vec::<String>::new());
        // A letter glued to a word is not a code.
        assert_eq!(derivation_refs("G1", "from HEBREW G12x and G7;"), ["G7"]);
    }

    #[test]
    fn a_family_is_everything_under_one_root() {
        let d = dict(&[
            ("G1139", "middle voice from G1142 (δαίμων);"),
            ("G1140", "neuter of a derivative of G1142 (δαίμων);"),
            ("G1141", "from G1140 (δαιμόνιον) and G1142 (δαίμων);"),
            ("G1142", "from (to distribute fortunes);"),
            ("G1225", "from G1223 (διά) and G906 (βάλλω);"),
            ("G1228", "from G1225 (διαβάλλω);"),
            ("G1223", "a primary preposition;"),
            ("G906", "a primary verb;"),
            ("G2424", "of Hebrew origin (H03091);"),
            ("H3091", "from H3068 and H3467;"),
            ("H2616", "a primitive root; (compare H2603 (חָנַן));"),
            ("H2617", "from H2616 (חָסַד);"),
            ("H2603", "a primitive root;"),
        ]);
        let f = Families::build(&d);
        // δαίμων's family, the compound of two members included.
        assert_eq!(f.family("G1140"), ["G1139", "G1140", "G1141", "G1142"]);
        assert_eq!(f.family("G1142"), f.family("G1139"));
        // A compound of two UNRELATED parts is its own root: the devil is not
        // filed under "to throw".
        assert_eq!(f.family("G1228"), ["G1225", "G1228"]);
        assert_eq!(f.family("G906"), ["G906"]);
        assert_eq!(f.family("G1223"), ["G1223"]);
        // The testaments stay apart, and so does a compared entry.
        assert_eq!(f.family("G2424"), ["G2424"]);
        assert_eq!(f.family("H2617"), ["H2616", "H2617"]);
        assert_eq!(f.family("H2603"), ["H2603"]);
        // Not in the dictionary at all: still an answer.
        assert_eq!(f.family("G9999"), ["G9999"]);
    }

    #[test]
    fn a_derivation_cycle_terminates() {
        let d = dict(&[("G1", "from G2;"), ("G2", "from G1;"), ("G3", "from G9;")]);
        let f = Families::build(&d);
        let fam = f.family("G1");
        assert_eq!(fam, ["G1", "G2"]);
        assert_eq!(f.family("G2"), fam);
        // A parent the dictionary lacks cannot be walked.
        assert_eq!(f.family("G3"), ["G3"]);
    }
    #[test]
    fn greek_is_romanized_the_sbl_way() {
        for (lemma, want) in [
            ("δαιμόνιον", "daimonion"),
            ("δαίμων", "daimōn"),
            ("δαιμονίζομαι", "daimonizomai"),
            ("ἀγάπη", "agapē"),
            ("Ἰησοῦς", "Iēsous"),
            ("Χριστός", "Christos"),
            ("ἁμαρτία", "hamartia"),
            ("οὗτος", "houtos"),
            ("υἱός", "huios"),
            ("ἄγγελος", "angelos"),
            ("εὐαγγέλιον", "euangelion"),
            ("ψυχή", "psychē"),
            ("ῥῆμα", "rhēma"),
            ("Ἑβραῖος", "Hebraios"),
            ("ζῷον", "zōon"),
            ("Μωϋσῆς", "Mōusēs"),
            ("θεός", "theos"),
            ("ἐγκράτεια", "enkrateia"),
            ("Ῥώμη", "Rhōmē"),
            // Not Greek: left alone, so a lemma with a note or a space keeps it.
            ("ἀλλ’ ἤ", "all’ ē"),
            ("abc", "abc"),
        ] {
            assert_eq!(romanize_greek(lemma), want, "{lemma}");
        }
    }

    #[test]
    fn parsing_fills_greek_transliterations_and_keeps_the_rest() {
        let raw = r#"{
          "G26":{"lemma":"ἀγάπη"},
          "G1":{"lemma":"Α","xlit":"shipped"},
          "G2":{"strongs_def":"no lemma"},
          "H1":{"lemma":"אָב","xlit":"ʼâb"},
          "H2":{"lemma":"אֵם"}
        }"#;
        let d = parse_strongs(raw.as_bytes()).unwrap();
        assert_eq!(d["G26"].xlit.as_deref(), Some("agapē"));
        assert_eq!(d["G1"].xlit.as_deref(), Some("shipped"), "a shipped transliteration is kept");
        assert_eq!(d["G2"].xlit, None);
        assert_eq!(d["H1"].xlit.as_deref(), Some("ʼâb"));
        assert_eq!(d["H2"].xlit, None, "Hebrew is never derived: the data carries its own");
    }
}
