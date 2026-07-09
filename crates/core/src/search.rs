//! Plain-text search over the canonical corpus, plus reference jumps and
//! query-by-Strong's-code.
//!
//! Ported from overlay `Search.hs`. The inverted index is one fold over the
//! corpus at startup. A single-word query is answered in four ranked tiers
//! (exact → morphological variants → other renderings of the same Strong's
//! lemma → near spellings); a multi-word query is a phrase match, falling back
//! to every-word-in-any-order. A query that reads as a reference (`John 3:16`,
//! `1 Cor 13`, `psalms`) becomes a jump. A bare Strong's code (`H430`) lists
//! every verse tagged with it.
//!
//! The morphology *form-predicate* path (`tense:aorist voice:passive`) needs
//! the optional morphology layer and so is answered here only with a "needs
//! the morphology layer" placeholder; `pure-rnd` will extend it.

use crate::canon;
use crate::corpus::Corpus;
use crate::reference::VRef;
use std::collections::{HashMap, HashSet};

/// Results shown at most; the total stays honest above the cap.
pub const HIT_CAP: usize = 200;

/// The inverted index, built once over the corpus. `word` alone answers the
/// exact and phrase tiers; the other three widen a single word without
/// re-reading the text. Ported from `SearchIx`.
#[derive(Debug, Clone, Default)]
pub struct SearchIx {
    /// lowercased word → ascending verse indices.
    word: HashMap<String, Vec<usize>>,
    /// stem → the indexed words sharing it.
    stems: HashMap<String, Vec<String>>,
    /// lowercased word → the Strong's lemmas that render it.
    word_lem: HashMap<String, Vec<String>>,
    /// Strong's lemma → ascending verse indices.
    lemma_ix: HashMap<String, Vec<usize>>,
}

impl SearchIx {
    /// Build the index in one fold over the corpus. Ported from
    /// `buildSearchIx`.
    pub fn build(corpus: &Corpus) -> Self {
        let mut word: HashMap<String, Vec<usize>> = HashMap::new();
        let mut lemma_ix: HashMap<String, Vec<usize>> = HashMap::new();
        let mut word_lem: HashMap<String, HashSet<String>> = HashMap::new();

        for (i, v) in corpus.verses().iter().enumerate() {
            let mut lemmas_here: HashSet<&str> = HashSet::new();
            for t in &v.tokens {
                let w = t.word.to_lowercase();
                word.entry(w.clone()).or_default().push(i);
                for s in &t.strongs {
                    lemmas_here.insert(s.as_str());
                    word_lem.entry(w.clone()).or_default().insert(s.clone());
                }
            }
            for s in lemmas_here {
                lemma_ix.entry(s.to_string()).or_default().push(i);
            }
        }

        // postings are already ascending (verses iterated in order); collapse
        // consecutive duplicates from a word/lemma recurring in one verse.
        for v in word.values_mut() {
            v.dedup();
        }
        for v in lemma_ix.values_mut() {
            v.dedup();
        }

        let mut stems: HashMap<String, Vec<String>> = HashMap::new();
        for w in word.keys() {
            stems.entry(stem_word(w)).or_default().push(w.clone());
        }

        let word_lem = word_lem
            .into_iter()
            .map(|(k, set)| {
                let mut v: Vec<String> = set.into_iter().collect();
                v.sort();
                (k, v)
            })
            .collect();

        SearchIx { word, stems, word_lem, lemma_ix }
    }

    /// Number of distinct indexed words (for a headless `--check`).
    pub fn distinct_words(&self) -> usize {
        self.word.len()
    }

    fn word_idxs(&self, w: &str) -> &[usize] {
        self.word.get(w).map(Vec::as_slice).unwrap_or(&[])
    }
    fn lemma_idxs(&self, lemma: &str) -> &[usize] {
        self.lemma_ix.get(lemma).map(Vec::as_slice).unwrap_or(&[])
    }
}

/// One search hit. Ported from `SearchHit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub vref: VRef,
    /// matched in a 1769 margin note, not the verse text.
    pub note: bool,
    /// why this verse widened past an exact match (`""` for exact/phrase):
    /// e.g. `"variant"`, `"also H430"`, `"≈ typo"`.
    pub why: String,
}

/// A query's answer. Ported from `SearchAnswer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchAnswer {
    /// The query is a reference: book id, chapter, and verse (when given).
    GoTo { book: String, chapter: u16, verse: Option<u16> },
    /// How the hits were found (shown to the reader), the honest total count,
    /// and the hits, capped at [`HIT_CAP`] in tier-then-canon order.
    Hits { how: String, total: usize, hits: Vec<SearchHit> },
}

/// Notes keyed by verse — the 1769 translators' margin notes, searched last.
pub type Notes = HashMap<VRef, Vec<String>>;

/// Answer a query against the corpus, its margin notes, and the index.
/// `None` means the query is blank. Ported from `runSearch`.
pub fn run_search(
    corpus: &Corpus,
    notes: &Notes,
    ix: &SearchIx,
    raw_query: &str,
) -> Option<SearchAnswer> {
    let q = raw_query.trim();
    if q.is_empty() {
        return None;
    }
    if let Some((book, chapter, verse)) = parse_ref_query(corpus, q) {
        return Some(SearchAnswer::GoTo { book, chapter, verse });
    }
    if let Some(fq) = parse_form_query(q) {
        return Some(form_search(corpus, ix, &fq));
    }

    let qws: Vec<String> = q
        .split_whitespace()
        .map(normalize_word)
        .filter(|w| !w.is_empty())
        .collect();
    if qws.is_empty() {
        return None;
    }

    let (how, rows) = if qws.len() == 1 {
        single_word(corpus, notes, ix, &qws[0])
    } else {
        multi_word(corpus, notes, ix, &qws)
    };

    let total = rows.len();
    let hits = rows
        .into_iter()
        .take(HIT_CAP)
        .map(|(i, note, why)| SearchHit {
            vref: corpus.verse_at(i).map(|v| v.vref()).unwrap(),
            note,
            why,
        })
        .collect();
    Some(SearchAnswer::Hits { how, total, hits })
}

/// The verse indices whose margin notes contain the whole normalized query.
fn note_idxs(corpus: &Corpus, notes: &Notes, needle: &str) -> Vec<usize> {
    let mut idxs: Vec<usize> = notes
        .iter()
        .filter(|(_, ns)| ns.iter().any(|n| n.to_lowercase().contains(needle)))
        .filter_map(|(r, _)| corpus.index_of(r))
        .collect();
    idxs.sort_unstable();
    idxs
}

type Rows = Vec<(usize, bool, String)>;

fn single_word(corpus: &Corpus, notes: &Notes, ix: &SearchIx, w: &str) -> (String, Rows) {
    let exact = ix.word_idxs(w).to_vec();
    let exact_set: HashSet<usize> = exact.iter().copied().collect();

    let note_only: Vec<usize> = note_idxs(corpus, notes, w)
        .into_iter()
        .filter(|i| !exact_set.contains(i))
        .collect();

    let mut seen: HashSet<usize> = exact_set;
    seen.extend(note_only.iter().copied());

    let variants = unique_by(&mut seen, variant_hits(ix, w));
    let renders = unique_by(&mut seen, lemma_hits(ix, w));

    let mut upper: Rows = Vec::new();
    upper.extend(exact.iter().map(|&i| (i, false, String::new())));
    upper.extend(note_only.iter().map(|&i| (i, true, String::new())));
    upper.extend(variants.iter().cloned().map(|(i, why)| (i, false, why)));
    upper.extend(renders.iter().cloned().map(|(i, why)| (i, false, why)));

    // Skip the full-vocabulary Levenshtein pass once the better tiers already
    // fill the cap — those near-spellings would be dropped anyway.
    let typos = if upper.len() >= HIT_CAP {
        Vec::new()
    } else {
        unique_by(&mut seen, fuzzy_hits(ix, w))
    };

    let label = if !exact.is_empty() || !note_only.is_empty() {
        "verses with the word"
    } else if !variants.is_empty() {
        "no exact match — word variants"
    } else if !renders.is_empty() {
        "no exact match — same original word"
    } else if !typos.is_empty() {
        "no exact match — near spellings"
    } else {
        "verses with the word"
    };

    let mut rows = upper;
    rows.extend(typos.into_iter().map(|(i, why)| (i, false, why)));
    (label.to_string(), rows)
}

/// Tier 2: words that stem to the same root as the query, and their verses.
fn variant_hits(ix: &SearchIx, w: &str) -> Vec<(usize, String)> {
    let mut hits: Vec<(usize, String)> = Vec::new();
    if let Some(words) = ix.stems.get(&stem_word(w)) {
        for v in words {
            if v != w {
                for &i in ix.word_idxs(v) {
                    hits.push((i, "variant".to_string()));
                }
            }
        }
    }
    hits.sort_by_key(|(i, _)| *i);
    hits
}

/// Tier 3: verses carrying a Strong's lemma the query renders.
fn lemma_hits(ix: &SearchIx, w: &str) -> Vec<(usize, String)> {
    let mut hits: Vec<(usize, String)> = Vec::new();
    if let Some(lemmas) = ix.word_lem.get(w) {
        for lemma in lemmas {
            for &i in ix.lemma_idxs(lemma) {
                hits.push((i, format!("also {lemma}")));
            }
        }
    }
    hits.sort_by_key(|(i, _)| *i);
    hits
}

/// Tier 4: vocabulary words within a small edit distance, nearest first.
fn fuzzy_hits(ix: &SearchIx, w: &str) -> Vec<(usize, String)> {
    let d = fuzzy_max(w.chars().count());
    if d < 1 {
        return Vec::new();
    }
    let wlen = w.chars().count() as isize;
    let mut near: Vec<(&String, usize)> = ix
        .word
        .keys()
        .filter(|v| v.as_str() != w)
        .filter(|v| ((v.chars().count() as isize) - wlen).abs() <= d as isize)
        .filter_map(|v| {
            let dist = levenshtein(w, v);
            if (1..=d).contains(&dist) {
                Some((v, dist))
            } else {
                None
            }
        })
        .collect();
    near.sort_by_key(|(_, dist)| *dist);

    let mut hits = Vec::new();
    for (v, _) in near {
        for &i in ix.word_idxs(v) {
            hits.push((i, "≈ typo".to_string()));
        }
    }
    hits
}

fn multi_word(corpus: &Corpus, notes: &Notes, ix: &SearchIx, qws: &[String]) -> (String, Rows) {
    let postings: Vec<&[usize]> = qws.iter().map(|w| ix.word_idxs(w)).collect();

    // phrase candidates via the rarest word, then confirm a consecutive run.
    let phrase_idxs: Vec<usize> = {
        let rarest = postings.iter().min_by_key(|p| p.len());
        match rarest {
            None => Vec::new(),
            Some(rare) => rare
                .iter()
                .copied()
                .filter(|&i| {
                    let verse_words: Vec<String> = corpus
                        .verse_at(i)
                        .map(|v| v.tokens.iter().map(|t| t.word.to_lowercase()).collect())
                        .unwrap_or_default();
                    consecutive_in(qws, &verse_words)
                })
                .collect(),
        }
    };

    let (label, text_idxs) = if !phrase_idxs.is_empty() {
        ("verses with the phrase", phrase_idxs)
    } else {
        ("no exact phrase — verses with every word", and_idxs(&postings))
    };

    let text_set: HashSet<usize> = text_idxs.iter().copied().collect();
    let needle = qws.join(" ");
    let note_only: Vec<usize> = note_idxs(corpus, notes, &needle)
        .into_iter()
        .filter(|i| !text_set.contains(i))
        .collect();

    let mut rows: Rows = text_idxs.into_iter().map(|i| (i, false, String::new())).collect();
    rows.extend(note_only.into_iter().map(|i| (i, true, String::new())));
    (label.to_string(), rows)
}

/// Intersect all postings (every-word-in-any-order), keeping ascending order.
fn and_idxs(postings: &[&[usize]]) -> Vec<usize> {
    let mut iter = postings.iter();
    let mut acc: Vec<usize> = match iter.next() {
        Some(first) => first.to_vec(),
        None => return Vec::new(),
    };
    for p in iter {
        acc = intersect_asc(&acc, p);
    }
    acc
}

/// Keep the first appearance of each index not already claimed by a better
/// tier (`seen`), extending `seen`. Ported from `uniqueBy`.
fn unique_by(seen: &mut HashSet<usize>, list: Vec<(usize, String)>) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, why) in list {
        if seen.insert(i) {
            out.push((i, why));
        }
    }
    out
}

/// The edit-distance ceiling for a query of this length. Ported from
/// `fuzzyMax`.
fn fuzzy_max(n: usize) -> usize {
    if n < 4 {
        0
    } else if n <= 6 {
        1
    } else {
        2
    }
}

fn consecutive_in(ws: &[String], vs: &[String]) -> bool {
    if ws.len() > vs.len() {
        return false;
    }
    vs.windows(ws.len()).any(|w| w == ws)
}

/// Intersect two ascending, deduplicated lists.
fn intersect_asc(xs: &[usize], ys: &[usize]) -> Vec<usize> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < xs.len() && j < ys.len() {
        match xs[i].cmp(&ys[j]) {
            std::cmp::Ordering::Equal => {
                out.push(xs[i]);
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    out
}

// ── normalization + stemming ─────────────────────────────────────────────────

/// Lowercase and strip punctuation the tokenizer would never leave inside a
/// word, so a query matches the index however typed or quoted. Ported from
/// `normalizeWord`.
pub fn normalize_word(w: &str) -> String {
    w.to_lowercase()
        .chars()
        .filter(|&c| c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '-')
        .collect()
}

/// A light inflectional stemmer over the 1769 English vocabulary — just enough
/// to fold a word onto the base it shares with its plurals and tenses.
/// Deliberately not Porter. Ported verbatim from `stemWord`; operates on an
/// already-[`normalize_word`]ed token.
pub fn stem_word(w: &str) -> String {
    verb(&plural(&possessive(w)))
}

fn possessive(t: &str) -> String {
    for suf in ["'s", "\u{2019}s", "'", "\u{2019}"] {
        if let Some(s) = t.strip_suffix(suf) {
            return s.to_string();
        }
    }
    t.to_string()
}

fn plural(t: &str) -> String {
    if let Some(s) = t.strip_suffix("sses") {
        return format!("{s}ss");
    }
    if let Some(s) = t.strip_suffix("ies") {
        if s.chars().count() >= 2 {
            return format!("{s}y");
        }
    }
    if t.ends_with("ss") {
        return t.to_string();
    }
    if let Some(s) = t.strip_suffix('s') {
        if s.chars().count() >= 3 {
            return s.to_string();
        }
    }
    t.to_string()
}

fn verb(t: &str) -> String {
    let peeled = if let Some(s) = t.strip_suffix("ing") {
        if keepable(s) {
            Some(s)
        } else {
            None
        }
    } else if let Some(s) = t.strip_suffix("ed") {
        if keepable(s) {
            Some(s)
        } else {
            None
        }
    } else {
        None
    };
    match peeled {
        Some(s) => undouble(s),
        None => t.to_string(),
    }
}

fn keepable(s: &str) -> bool {
    s.chars().count() >= 3 && s.chars().any(is_vowel)
}

/// `runn(ing)→run`, `hopp(ed)→hop`; l/s/z doubles (call, bless, buzz) stay.
fn undouble(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    if n >= 2 {
        let c = chars[n - 1];
        let d = chars[n - 2];
        if c == d && !matches!(c, 'l' | 's' | 'z') {
            return chars[..n - 1].iter().collect();
        }
    }
    s.to_string()
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

/// Classic Levenshtein edit distance between two short words, one DP row at a
/// time. Callers prune by length first. Ported from `levenshtein`.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let bs: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=bs.len()).collect();
    let mut cur = vec![0usize; bs.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in bs.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[bs.len()]
}

// ── form queries (Strong's code + morphology predicates) ─────────────────────

/// A query over the morphology layer: an optional Strong's code plus
/// `field:value` predicates. Ported from `FormQuery`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormQuery {
    pub strong: Option<String>,
    pub preds: Vec<(String, String)>,
}

/// The recognized `field:value` predicate fields → canonical morphology axis.
fn form_fields(f: &str) -> Option<&'static str> {
    Some(match f {
        "pos" => "pos",
        "binyan" | "stem" => "stem",
        "aspect" | "conj" | "tense" => "conj",
        "voice" => "voice",
        "mood" => "mood",
        "case" => "case",
        "person" => "person",
        "gender" => "gender",
        "number" => "number",
        _ => return None,
    })
}

/// Normalize a Strong's code like `H0430` → `H430`, or `None` if not a code.
fn strongs_code(t: &str) -> Option<String> {
    let mut chars = t.chars();
    let first = chars.next()?;
    let prefix = first.to_ascii_uppercase();
    if prefix != 'H' && prefix != 'G' {
        return None;
    }
    let digits: String = chars.collect();
    if digits.is_empty() || digits.len() > 5 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    Some(format!("{prefix}{n}"))
}

/// Recognize a form query: every token must be a Strong's code (at most one)
/// or a known `field:value` predicate. Ported from `parseFormQuery`.
pub fn parse_form_query(q: &str) -> Option<FormQuery> {
    let mut strongs: Vec<String> = Vec::new();
    let mut preds: Vec<(String, String)> = Vec::new();
    for t in q.split_whitespace() {
        if let Some(s) = strongs_code(t) {
            strongs.push(s);
        } else if let Some((f, rest)) = t.split_once(':') {
            let field = form_fields(&f.to_lowercase())?;
            let val = rest.to_lowercase();
            if val.is_empty() {
                return None;
            }
            preds.push((field.to_string(), val));
        } else {
            return None;
        }
    }
    match strongs.len() {
        0 if preds.is_empty() => None,
        0 => Some(FormQuery { strong: None, preds }),
        1 => Some(FormQuery { strong: strongs.into_iter().next(), preds }),
        _ => None, // two codes: not a query we can answer
    }
}

/// Execute a form query. A bare Strong's code answers straight from the lemma
/// index (query-by-Strong's). Predicate queries need the morphology layer,
/// which lives in the optional `pure-rnd` crate — here they return a
/// placeholder telling the reader to hydrate it. Ported from `formSearch`
/// (bare-code path; predicate path stubbed pending `pure-rnd`).
pub fn form_search(corpus: &Corpus, ix: &SearchIx, fq: &FormQuery) -> SearchAnswer {
    if fq.preds.is_empty() {
        if let Some(s) = &fq.strong {
            let idxs = ix.lemma_idxs(s);
            let hits = idxs
                .iter()
                .take(HIT_CAP)
                .map(|&i| SearchHit {
                    vref: corpus.verse_at(i).map(|v| v.vref()).unwrap(),
                    note: false,
                    why: String::new(),
                })
                .collect();
            return SearchAnswer::Hits {
                how: format!("verses tagged {s}"),
                total: idxs.len(),
                hits,
            };
        }
    }
    SearchAnswer::Hits {
        how: "form search needs the morphology layer — enable it in Full study mode".to_string(),
        total: 0,
        hits: Vec::new(),
    }
}

// ── reference queries ────────────────────────────────────────────────────────

/// Read a query as a verse reference: `<book>`, `<book> <ch>`, or
/// `<book> <ch>:<v>`. The book resolves case-insensitively against OSIS ids,
/// display names, or an unambiguous display-name prefix; chapter/verse are
/// validated against the corpus. Ported from `parseRefQuery`.
pub fn parse_ref_query(corpus: &Corpus, q: &str) -> Option<(String, u16, Option<u16>)> {
    let words: Vec<&str> = q.split_whitespace().collect();
    let (last, before) = words.split_last()?;

    if let Some((c, mv)) = parse_chapter_verse(last) {
        if !before.is_empty() {
            let bid = resolve_book(&before.join(" "))?;
            return validate(corpus, bid, c, mv);
        }
    }
    // whole query is a book name / prefix → chapter 1
    let bid = resolve_book(q)?;
    Some((bid, 1, None))
}

fn validate(corpus: &Corpus, bid: String, c: u16, mv: Option<u16>) -> Option<(String, u16, Option<u16>)> {
    let nc = corpus.chapter_count(&bid);
    if c < 1 || c > nc {
        return None;
    }
    match mv {
        None => Some((bid, c, None)),
        Some(v) => {
            let nv = corpus.chapter_verses(&bid, c).len() as u16;
            if v >= 1 && v <= nv {
                Some((bid, c, Some(v)))
            } else {
                None
            }
        }
    }
}

fn parse_chapter_verse(t: &str) -> Option<(u16, Option<u16>)> {
    match t.split_once(':') {
        None => t.parse::<u16>().ok().map(|c| (c, None)),
        Some((c, v)) => {
            let c = c.parse::<u16>().ok()?;
            let v = v.parse::<u16>().ok()?;
            Some((c, Some(v)))
        }
    }
}

fn resolve_book(t: &str) -> Option<String> {
    let needle = t.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    // exact: OSIS id or display name (case-insensitive)
    if let Some(b) = canon::BOOKS
        .iter()
        .find(|b| b.id.to_lowercase() == needle || b.name.to_lowercase() == needle)
    {
        return Some(b.id.to_string());
    }
    // unambiguous display-name prefix
    let prefixed: Vec<&str> = canon::BOOKS
        .iter()
        .filter(|b| b.name.to_lowercase().starts_with(&needle))
        .map(|b| b.id)
        .collect();
    if prefixed.len() == 1 {
        Some(prefixed[0].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus;

    const SAMPLE: &str = concat!(
        r#"{"format":"x","tokenization":"kjv1769-tok2","verses":5}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","In","",[],0],["","the","",[],0],["","beginning","",["H7225"],0],["","God","",["H430"],0],["","created","",["H1254"],0]],"v":1}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","And","",[],0],["","God","",["H430"],0],["","blessed","",["H1288"],0],["","them",".",[],0]],"v":2}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","God","",["H430"],0],["","blessing","",["H1288"],0]],"v":3}"#,
        "\n",
        r#"{"b":"Ps","c":23,"t":[["","The","",[],4],["","LORD","",["H3068"],2],["","is","",[],0],["","my","",[],0],["","shepherd",".",["H7462"],0]],"v":1}"#,
        "\n",
        r#"{"b":"John","c":3,"t":[["","For","",[],0],["","God","",["H2316"],0],["","so","",[],0],["","loved","",[],0]],"v":16}"#,
    );

    fn ix_of(c: &corpus::Corpus) -> SearchIx {
        SearchIx::build(c)
    }

    #[test]
    fn normalize_and_stem() {
        assert_eq!(normalize_word("God's,"), "god's");
        assert_eq!(stem_word("stars"), "star");
        assert_eq!(stem_word("blessed"), "bless");
        assert_eq!(stem_word("blessing"), "bless");
        assert_eq!(stem_word("running"), "run");
        // l/s/z doubles are preserved
        assert_eq!(stem_word("bless"), "bless");
        assert_eq!(levenshtein("beginning", "begining"), 1);
    }

    #[test]
    fn exact_word_search() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let ans = run_search(&c, &notes, &ix, "God").unwrap();
        match ans {
            SearchAnswer::Hits { how, hits, .. } => {
                assert_eq!(how, "verses with the word");
                // God in Gen 1:1, 1:2, 1:3, John 3:16
                let refs: Vec<_> = hits.iter().map(|h| h.vref.ref_key()).collect();
                assert!(refs.contains(&"Gen 1:1".to_string()));
                assert!(refs.contains(&"John 3:16".to_string()));
            }
            _ => panic!("expected hits"),
        }
    }

    #[test]
    fn variant_tier_finds_stems() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        // "bless" isn't a token surface, but "blessed"/"blessing" stem to it.
        let ans = run_search(&c, &notes, &ix, "bless").unwrap();
        match ans {
            SearchAnswer::Hits { how, hits, .. } => {
                assert!(how.contains("word variants"));
                assert!(hits.iter().all(|h| h.why == "variant"));
                assert_eq!(hits.len(), 2);
            }
            _ => panic!("expected hits"),
        }
    }

    #[test]
    fn phrase_search() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let ans = run_search(&c, &notes, &ix, "God created").unwrap();
        match ans {
            SearchAnswer::Hits { how, hits, .. } => {
                assert_eq!(how, "verses with the phrase");
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].vref.ref_key(), "Gen 1:1");
            }
            _ => panic!("expected phrase hit"),
        }
    }

    #[test]
    fn reference_jump() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        // Gen 1 has 3 verses in the sample; verse 2 is a valid reference.
        assert_eq!(
            run_search(&c, &notes, &ix, "Gen 1:2"),
            Some(SearchAnswer::GoTo { book: "Gen".into(), chapter: 1, verse: Some(2) })
        );
        // A bare book name jumps to its chapter 1.
        assert_eq!(
            run_search(&c, &notes, &ix, "psalms"),
            Some(SearchAnswer::GoTo { book: "Ps".into(), chapter: 1, verse: None })
        );
        // out-of-range verse is not a reference → falls through to text search
        match run_search(&c, &notes, &ix, "Gen 1:999").unwrap() {
            SearchAnswer::Hits { .. } => {}
            _ => panic!("expected fallthrough to text search"),
        }
    }

    #[test]
    fn bare_strongs_code() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let ans = run_search(&c, &notes, &ix, "H430").unwrap();
        match ans {
            SearchAnswer::Hits { how, total, .. } => {
                assert_eq!(how, "verses tagged H430");
                assert_eq!(total, 3); // Gen 1:1, 1:2, 1:3
            }
            _ => panic!("expected strongs hits"),
        }
        // normalization H0430 → H430
        assert_eq!(strongs_code("H0430"), Some("H430".to_string()));
    }

    #[test]
    fn notes_are_searched() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let mut notes = Notes::new();
        notes.insert(VRef::new("Gen", 1, 2), vec!["Heb. expansion of firmament".into()]);
        let ans = run_search(&c, &notes, &ix, "firmament").unwrap();
        match ans {
            SearchAnswer::Hits { hits, .. } => {
                assert!(hits.iter().any(|h| h.note && h.vref.ref_key() == "Gen 1:2"));
            }
            _ => panic!("expected note hit"),
        }
    }
}
