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
//! the morphology layer" placeholder; `plumbline-rnd` will extend it.

use crate::canon;
use crate::corpus::{Corpus, Verse};
use crate::reference::VRef;
use std::collections::{HashMap, HashSet};

/// Results shown at most; the total stays honest above the cap.
pub const HIT_CAP: usize = 200;

/// Matthew's position in canon order — where the New Testament begins.
/// `canon`'s own test pins `book_order("Matt") == 39`.
const NT_FIRST_ORDER: usize = 39;

/// Where a search looks — the search screen's scope chips. Every scope is a
/// CONTIGUOUS run of canonical verse indices (the corpus is in canon order),
/// so filtering is one range test per posting and the honest `total` counts
/// only what the scope covers. A reference query ignores the scope on
/// purpose: "John 3" is navigation, not filtering.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SearchScope {
    #[default]
    All,
    /// One book, by OSIS id.
    Book(String),
    /// One chapter of one book.
    Chapter(String, u16),
    /// Genesis–Malachi.
    OldTestament,
    /// Matthew–Revelation.
    NewTestament,
    /// A contiguous span of chapters, INCLUSIVE at both ends — "John 3–8",
    /// "Genesis 1 – Deuteronomy 34".
    ///
    /// The shells' range picker and their canon presets (Law, Gospels,
    /// Letters…) both land here: a preset is a span over a
    /// [`crate::reference::CANON_SEGMENTS`] row, so there is no second list of
    /// groupings to drift from the canon strip's.
    Span { from_book: String, from_chapter: u16, to_book: String, to_chapter: u16 },
}

impl SearchScope {
    /// The wire token the scoped FFI endpoints take:
    /// `all` | `ot` | `nt` | `book:<osis>` | `chapter:<osis>:<ch>` |
    /// `span:<osis>:<ch>:<osis>:<ch>`.
    pub fn token(&self) -> String {
        match self {
            SearchScope::All => "all".to_string(),
            SearchScope::OldTestament => "ot".to_string(),
            SearchScope::NewTestament => "nt".to_string(),
            SearchScope::Book(b) => format!("book:{b}"),
            SearchScope::Chapter(b, c) => format!("chapter:{b}:{c}"),
            SearchScope::Span { from_book, from_chapter, to_book, to_chapter } => {
                format!("span:{from_book}:{from_chapter}:{to_book}:{to_chapter}")
            }
        }
    }

    /// Parse a wire token; `None` for anything unrecognized (the FFI layer
    /// treats that as `All` rather than failing the whole query).
    pub fn parse(t: &str) -> Option<SearchScope> {
        match t {
            "all" => return Some(SearchScope::All),
            "ot" => return Some(SearchScope::OldTestament),
            "nt" => return Some(SearchScope::NewTestament),
            _ => {}
        }
        if let Some(b) = t.strip_prefix("book:") {
            return (!b.is_empty()).then(|| SearchScope::Book(b.to_string()));
        }
        if let Some(rest) = t.strip_prefix("chapter:") {
            let (b, c) = rest.rsplit_once(':')?;
            let ch: u16 = c.parse().ok()?;
            return (!b.is_empty() && ch >= 1).then(|| SearchScope::Chapter(b.to_string(), ch));
        }
        if let Some(rest) = t.strip_prefix("span:") {
            // Four fields, book:chapter twice. An OSIS id never contains a
            // colon, so splitting is unambiguous.
            let parts: Vec<&str> = rest.split(':').collect();
            let [b1, c1, b2, c2] = parts[..] else { return None };
            let (from_chapter, to_chapter) = (c1.parse().ok()?, c2.parse().ok()?);
            if b1.is_empty() || b2.is_empty() || from_chapter < 1 || to_chapter < 1 {
                return None;
            }
            return Some(SearchScope::Span {
                from_book: b1.to_string(),
                from_chapter,
                to_book: b2.to_string(),
                to_chapter,
            });
        }
        None
    }

    /// The verse-index range this scope covers in this corpus. `None` means
    /// "no filter" (`All`); an unknown book or chapter resolves to the EMPTY
    /// range — a scope the corpus can't locate must not quietly widen to
    /// everything.
    fn resolve(&self, corpus: &Corpus) -> Option<std::ops::Range<usize>> {
        match self {
            SearchScope::All => None,
            // Matthew opens the New Testament, and every corpus this app ships
            // sits at the KJV's own verse addresses (manifest §Languages). A
            // corpus without Matthew falls forward to the first NT book it does
            // have, so the split never lands mid-testament.
            SearchScope::OldTestament => Some(0..nt_start(corpus)),
            SearchScope::NewTestament => Some(nt_start(corpus)..corpus.len()),
            SearchScope::Book(b) => Some(corpus.book_range(b).unwrap_or(0..0)),
            SearchScope::Chapter(b, c) => Some(corpus.chapter_range(b, *c).unwrap_or(0..0)),
            SearchScope::Span { from_book, from_chapter, to_book, to_chapter } => {
                let a = chapter_bounds(corpus, from_book, *from_chapter);
                let z = chapter_bounds(corpus, to_book, *to_chapter);
                Some(match (a, z) {
                    // REVERSED ENDS ARE NORMALIZED, not refused: a reader who
                    // picks the far end first means the span between them, and
                    // an empty result would read as "no matches" rather than
                    // "you filled the boxes in the other order".
                    (Some(a), Some(z)) => {
                        if a.start <= z.start {
                            a.start..z.end
                        } else {
                            z.start..a.end
                        }
                    }
                    // An end this corpus cannot place empties the span, for the
                    // reason every unresolvable scope does: it must not widen.
                    _ => 0..0,
                })
            }
        }
    }
}

/// One chapter's verse-index range, with the chapter CLAMPED into the book.
///
/// Clamped rather than refused because the ends of a span come from a picker
/// and from canon presets: "to the end of Revelation" is naturally expressed as
/// a chapter number at or past the last one, and a preset built against a
/// corpus with different chapter counts should still mean "that book's end".
fn chapter_bounds(corpus: &Corpus, book: &str, chapter: u16) -> Option<std::ops::Range<usize>> {
    let last = corpus.chapter_count(book);
    corpus.chapter_range(book, chapter.clamp(1, last.max(1)))
}

/// Where the New Testament starts in this corpus.
fn nt_start(corpus: &Corpus) -> usize {
    canon::book_ids()
        .skip(NT_FIRST_ORDER)
        .find_map(|b| corpus.book_range(b).map(|r| r.start))
        .unwrap_or_else(|| corpus.len())
}

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
    /// Pre-lowercased margin notes (verse index → joined note text), attached
    /// via [`SearchIx::attach_notes`] so per-keystroke queries don't
    /// re-lowercase ~7k notes. `None` = fall back to scanning the Notes map.
    notes_lc: Option<Vec<(usize, String)>>,
}

/// Resumable [`SearchIx`] construction: the same fold, a slice of verses at a
/// time.
///
/// The whole-corpus fold takes seconds on a phone (~4.6 s),
/// and it runs on the one thread that also answers layout and taps — so a
/// reader who turned a page mid-build waited it out. Feeding it in slices lets
/// the shell yield between them.
#[derive(Debug, Default)]
pub struct SearchIxBuilder {
    word: HashMap<String, Vec<usize>>,
    lemma_ix: HashMap<String, Vec<usize>>,
    word_lem: HashMap<String, HashSet<String>>,
    /// Next canonical verse ordinal to fold in.
    next: usize,
}

impl SearchIxBuilder {
    /// Fold in up to `n` more verses. Returns true while work remains.
    pub fn feed(&mut self, corpus: &Corpus, n: usize) -> bool {
        let end = (self.next + n).min(corpus.len());
        for i in self.next..end {
            let Some(v) = corpus.verse_at(i) else { continue };
            self.fold(i, v);
        }
        self.next = end;
        end < corpus.len()
    }

    /// Everything the fold has seen, finished into a usable index.
    pub fn finish(self) -> SearchIx {
        SearchIx::finalize(self.word, self.lemma_ix, self.word_lem)
    }
}

impl SearchIx {
    /// Build the index in one fold over the corpus. Ported from
    /// `buildSearchIx`; slice it with [`SearchIxBuilder`] where blocking the
    /// thread for seconds is not acceptable.
    pub fn build(corpus: &Corpus) -> Self {
        let mut b = SearchIxBuilder::default();
        while b.feed(corpus, 4096) {}
        b.finish()
    }
}

impl SearchIxBuilder {
    /// Fold ONE verse (at canonical index `i`) into the partial tables.
    fn fold(&mut self, i: usize, v: &Verse) {
        let mut lemmas_here: HashSet<&str> = HashSet::new();
        for t in &v.tokens {
            // `fold_word`, not `to_lowercase`: an Arabic key must lose its
            // vowelling here or no reader will ever type the key that is in the
            // index. It is a no-op on Latin, so the KJV's index — and the
            // `.idxcache` the web manifest hashes — is unchanged.
            let w = fold_word(&t.word);
            // Clone the key only on first sight of a distinct word (~13k)
            // rather than per token (~1.6M).
            match self.word.get_mut(&w) {
                Some(idxs) => idxs.push(i),
                None => {
                    self.word.insert(w.clone(), vec![i]);
                }
            }
            if !t.strongs.is_empty() {
                // One probe on the common path (word already seen); the key
                // clone stays gated to first sight, as with `word`.
                let lems = match self.word_lem.get_mut(&w) {
                    Some(lems) => lems,
                    None => self.word_lem.entry(w.clone()).or_default(),
                };
                for s in &t.strongs {
                    lemmas_here.insert(s.as_str());
                    if !lems.contains(s.as_str()) {
                        lems.insert(s.clone());
                    }
                }
            }
        }
        for s in lemmas_here {
            self.lemma_ix.entry(s.to_string()).or_default().push(i);
        }
    }
}

impl SearchIx {
    /// Turn the folded tables into the finished index: dedup the postings,
    /// derive the stem map, and order each word's lemmas.
    fn finalize(
        mut word: HashMap<String, Vec<usize>>,
        mut lemma_ix: HashMap<String, Vec<usize>>,
        word_lem: HashMap<String, HashSet<String>>,
    ) -> Self {
        // postings are already ascending (verses folded in order); collapse
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

        SearchIx { word, stems, word_lem, lemma_ix, notes_lc: None }
    }

    /// Attach the margin notes for fast note search: pre-lowercase each
    /// verse's notes once here instead of on every keystroke.
    pub fn attach_notes(&mut self, corpus: &Corpus, notes: &Notes) {
        let mut lc: Vec<(usize, String)> =
            notes.iter().filter_map(|(r, ns)| corpus.index_of(r).map(|i| (i, ns.join("\n").to_lowercase()))).collect();
        lc.sort_unstable_by_key(|(i, _)| *i);
        self.notes_lc = Some(lc);
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
pub fn run_search(corpus: &Corpus, notes: &Notes, ix: &SearchIx, raw_query: &str) -> Option<SearchAnswer> {
    run_search_scoped(corpus, notes, ix, raw_query, &SearchScope::All)
}

/// [`run_search`] narrowed to a [`SearchScope`]. A reference query still
/// answers `GoTo` whatever the scope — the reader typed an address, and
/// refusing to take them there because a chip is set would read as broken.
pub fn run_search_scoped(
    corpus: &Corpus,
    notes: &Notes,
    ix: &SearchIx,
    raw_query: &str,
    scope: &SearchScope,
) -> Option<SearchAnswer> {
    let q = raw_query.trim();
    if q.is_empty() {
        return None;
    }
    if let Some((book, chapter, verse)) = parse_ref_query(corpus, q) {
        return Some(SearchAnswer::GoTo { book, chapter, verse });
    }
    let range = scope.resolve(corpus);
    if let Some(fq) = parse_form_query(q) {
        return Some(form_search_scoped(corpus, ix, &fq, range));
    }

    let qws: Vec<String> = q.split_whitespace().map(normalize_word).filter(|w| !w.is_empty()).collect();
    if qws.is_empty() {
        return None;
    }

    let (how, rows) = if qws.len() == 1 {
        single_word(corpus, notes, ix, &qws[0], range)
    } else {
        multi_word(corpus, notes, ix, &qws, range)
    };

    let total = rows.total;
    let hits = rows
        .kept
        .into_iter()
        .filter_map(|(i, note, why)| {
            // Graceful on an index/corpus disagreement instead of panicking.
            corpus.verse_at(i).map(|v| SearchHit { vref: v.vref(), note, why: why.render() })
        })
        .collect();
    Some(SearchAnswer::Hits { how: how.to_string(), total, hits })
}

/// The verse indices whose margin notes contain the whole normalized query.
fn note_idxs(corpus: &Corpus, notes: &Notes, ix: &SearchIx, needle: &str) -> Vec<usize> {
    // Fast path: the pre-lowercased notes attached to the index.
    if let Some(lc) = &ix.notes_lc {
        return lc.iter().filter(|(_, text)| text.contains(needle)).map(|(i, _)| *i).collect();
    }
    let mut idxs: Vec<usize> = notes
        .iter()
        .filter(|(_, ns)| ns.iter().any(|n| n.to_lowercase().contains(needle)))
        .filter_map(|(r, _)| corpus.index_of(r))
        .collect();
    idxs.sort_unstable();
    idxs
}

/// Why a verse widened past an exact match, held as a BORROW of the index until
/// the row is known to be one of the [`HIT_CAP`] that get shown.
///
/// The tiers used to hand back a freshly formatted `String` per POSTING — for a
/// query that stems onto "the" that is 24,127 copies of `"variant"`, and for
/// "god" 19,100 copies of `"also H…"` — and `run_search` then dropped all but
/// 200 of them. Only a reason that reaches the reader is ever built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why<'a> {
    /// An exact word, phrase, or note match: the reader needs no reason.
    Plain,
    Variant,
    /// Another rendering of a Strong's lemma the query renders.
    Also(&'a str),
    Typo,
}

impl Why<'_> {
    /// The reason as [`SearchHit::why`] carries it.
    fn render(self) -> String {
        match self {
            Why::Plain => String::new(),
            Why::Variant => "variant".to_string(),
            Why::Also(lemma) => format!("also {lemma}"),
            Why::Typo => "≈ typo".to_string(),
        }
    }
}

/// Which verse indices the answer already holds.
///
/// A bitset over the corpus rather than a `HashSet`: the tiers offer tens of
/// thousands of postings for a common word, the whole Bible is 31,102 verses, so
/// dedup costs a shift and a mask instead of a hash. An index past the end of
/// the corpus — an index/corpus disagreement, which `run_search` already handles
/// gracefully rather than panicking — is passed through and dropped later by
/// `verse_at`.
struct Seen(Vec<u64>);

impl Seen {
    fn new(verses: usize) -> Self {
        Seen(vec![0u64; verses / 64 + 1])
    }
    /// True the FIRST time an index is offered.
    fn insert(&mut self, i: usize) -> bool {
        let bit = 1u64 << (i % 64);
        match self.0.get_mut(i / 64) {
            Some(slot) => {
                let fresh = *slot & bit == 0;
                *slot |= bit;
                fresh
            }
            None => true,
        }
    }
}

/// The answer under construction: the shown rows in tier-then-canon order, and
/// the honest count of every distinct verse any tier reached.
///
/// The count and the rows are separated ON PURPOSE. `total` is what the reader
/// is told ("200 of 24,135") so it has to see every posting, but a row past the
/// cap can never be shown — so it is counted and thrown away instead of being
/// built, sorted, and copied twice on the way to being thrown away.
struct Rows<'a> {
    seen: Seen,
    /// Up to [`HIT_CAP`] rows, in the order they were offered.
    kept: Vec<(usize, bool, Why<'a>)>,
    /// Distinct verses across every tier — uncapped, and shown as the total.
    total: usize,
    /// The scope's verse-index range; `None` is the whole corpus. Checked at
    /// the mouth of `push` so every tier — words, notes, variants, lemmas,
    /// typos — is filtered in one place and `total` never counts a verse the
    /// scope excludes.
    scope: Option<std::ops::Range<usize>>,
}

impl<'a> Rows<'a> {
    fn new(corpus: &Corpus, scope: Option<std::ops::Range<usize>>) -> Self {
        Rows { seen: Seen::new(corpus.len()), kept: Vec::new(), total: 0, scope }
    }

    /// Offer one verse. False when the scope excludes it or a better tier
    /// already claimed it.
    fn push(&mut self, i: usize, note: bool, why: Why<'a>) -> bool {
        if let Some(r) = &self.scope {
            if !r.contains(&i) {
                return false;
            }
        }
        if !self.seen.insert(i) {
            return false;
        }
        self.total += 1;
        if self.kept.len() < HIT_CAP {
            self.kept.push((i, note, why));
        }
        true
    }

    /// Offer a tier's verses, answering how many of them were new — which is
    /// what the tier labels are chosen from.
    fn push_all(&mut self, idxs: impl IntoIterator<Item = usize>, note: bool, why: Why<'a>) -> usize {
        idxs.into_iter().filter(|&i| self.push(i, note, why)).count()
    }
}

fn single_word<'a>(
    corpus: &Corpus,
    notes: &Notes,
    ix: &'a SearchIx,
    w: &str,
    scope: Option<std::ops::Range<usize>>,
) -> (&'static str, Rows<'a>) {
    let mut rows = Rows::new(corpus, scope);

    // Tier 1, then the margin notes. The postings are already deduplicated and
    // ascending, so offering them in order IS the canon order.
    let exact = rows.push_all(ix.word_idxs(w).iter().copied(), false, Why::Plain);
    let note_only = rows.push_all(note_idxs(corpus, notes, ix, w), true, Why::Plain);

    let variants = rows.push_all(variant_idxs(ix, w), false, Why::Variant);

    let mut renders = 0;
    for (i, lemma) in rendering_idxs(ix, w) {
        if rows.push(i, false, Why::Also(lemma)) {
            renders += 1;
        }
    }

    // Skip the full-vocabulary Levenshtein pass once the better tiers already
    // fill the cap — those near-spellings would be dropped anyway.
    let typos = if rows.total >= HIT_CAP { 0 } else { rows.push_all(fuzzy_idxs(ix, w), false, Why::Typo) };

    let label = if exact > 0 || note_only > 0 {
        "verses with the word"
    } else if variants > 0 {
        "no exact match — word variants"
    } else if renders > 0 {
        "no exact match — same original word"
    } else if typos > 0 {
        "no exact match — near spellings"
    } else {
        "verses with the word"
    };
    (label, rows)
}

/// Tier 2: the verses of every word that stems to the same root as the query,
/// in canon order.
fn variant_idxs(ix: &SearchIx, w: &str) -> Vec<usize> {
    let mut idxs: Vec<usize> = Vec::new();
    if let Some(words) = ix.stems.get(&stem_word(w)) {
        for v in words {
            if v != w {
                idxs.extend_from_slice(ix.word_idxs(v));
            }
        }
    }
    idxs.sort_unstable();
    idxs
}

/// Tier 3: verses carrying a Strong's lemma the query renders, each with the
/// lemma that put it there, in canon order.
fn rendering_idxs<'a>(ix: &'a SearchIx, w: &str) -> Vec<(usize, &'a str)> {
    let mut hits: Vec<(usize, &str)> = Vec::new();
    if let Some(lemmas) = ix.word_lem.get(w) {
        for lemma in lemmas {
            for &i in ix.lemma_idxs(lemma) {
                hits.push((i, lemma.as_str()));
            }
        }
    }
    // STABLE, so a verse tagged with two of the query's lemmas keeps the first
    // of them — which is the one the reader is shown.
    hits.sort_by_key(|(i, _)| *i);
    hits
}

/// Tier 4: the verses of every vocabulary word within a small edit distance,
/// nearest word first.
fn fuzzy_idxs(ix: &SearchIx, w: &str) -> Vec<usize> {
    let mut near = near_words(ix, w);
    // Deterministic: HashMap iteration order varies per process, so break
    // distance ties by the word itself.
    near.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));

    let mut idxs = Vec::new();
    for (v, _) in near {
        idxs.extend_from_slice(ix.word_idxs(v));
    }
    idxs
}

/// The indexed words within [`fuzzy_max`] edits of the query, unordered.
///
/// ONE pass over the vocabulary with ONE set of scratch buffers. The buffers are
/// the reason this is not written as an iterator chain: measured against the
/// data pack, the three `Vec`s that a per-candidate [`levenshtein`] allocates
/// were ~38k allocations on a single keystroke, which cost more than the
/// arithmetic they held.
fn near_words<'a>(ix: &'a SearchIx, w: &str) -> Vec<(&'a String, usize)> {
    let q: Vec<char> = w.chars().collect();
    let d = fuzzy_max(q.len());
    if d < 1 {
        return Vec::new();
    }
    let mut buf = Lev::default();
    let mut near = Vec::new();
    for v in ix.word.keys() {
        if v.as_str() == w {
            continue;
        }
        if let Some(dist) = levenshtein_within(&q, v, d, &mut buf) {
            if dist >= 1 {
                near.push((v, dist));
            }
        }
    }
    near
}

fn multi_word<'a>(
    corpus: &Corpus,
    notes: &Notes,
    ix: &SearchIx,
    qws: &[String],
    scope: Option<std::ops::Range<usize>>,
) -> (&'static str, Rows<'a>) {
    let postings: Vec<&[usize]> = qws.iter().map(|w| ix.word_idxs(w)).collect();

    // Intersect every word's postings first (a phrase hit needs all of them),
    // then confirm a consecutive run comparing tokens in place. For common
    // bigrams ("of the") this replaces rebuilding ~500k lowercased Strings per
    // keystroke with an allocation-free scan of a few hundred candidates.
    // Narrowed BEFORE the phrase confirmation, so a scoped query neither
    // decodes verses it will discard nor lets a phrase outside the scope
    // choose the label for hits inside it.
    let in_scope = |i: &usize| scope.as_ref().is_none_or(|r| r.contains(i));
    let every_word: Vec<usize> = and_idxs(&postings).into_iter().filter(in_scope).collect();
    let phrase_idxs: Vec<usize> =
        every_word.iter().copied().filter(|&i| corpus.verse_at(i).is_some_and(|v| phrase_in_verse(qws, v))).collect();

    let (label, text_idxs) = if !phrase_idxs.is_empty() {
        ("verses with the phrase", phrase_idxs)
    } else {
        ("no exact phrase — verses with every word", every_word)
    };

    let mut rows = Rows::new(corpus, scope);
    rows.push_all(text_idxs, false, Why::Plain);
    let needle = qws.join(" ");
    rows.push_all(note_idxs(corpus, notes, ix, &needle), true, Why::Plain);
    (label, rows)
}

/// Intersect all postings (every-word-in-any-order), keeping ascending order.
fn and_idxs(postings: &[&[usize]]) -> Vec<usize> {
    // Smallest posting first: the initial `to_vec` copies the shortest list and
    // the accumulator stays minimal across a 3+ word query, so a query like
    // "the beginning" is bounded by the rare word, not "the". Intersection is
    // order-independent and `intersect_asc` always emits ascending, so both the
    // result and its order are unchanged.
    let mut ordered: Vec<&[usize]> = postings.to_vec();
    ordered.sort_by_key(|p| p.len());
    let mut iter = ordered.iter();
    let mut acc: Vec<usize> = match iter.next() {
        Some(first) => first.to_vec(),
        None => return Vec::new(),
    };
    for p in iter {
        acc = intersect_asc(&acc, p);
    }
    acc
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

/// Case-insensitive equality of a corpus word against an already-lowercased
/// query word, without allocating.
fn word_eq_lower(word: &str, lower: &str) -> bool {
    word.chars().flat_map(char::to_lowercase).eq(lower.chars())
}

/// Do the query words appear as a consecutive token run in this verse?
fn phrase_in_verse(qws: &[String], v: &crate::corpus::Verse) -> bool {
    let n = qws.len();
    if n == 0 || n > v.tokens.len() {
        return false;
    }
    (0..=v.tokens.len() - n)
        .any(|start| qws.iter().enumerate().all(|(k, qw)| word_eq_lower(&v.tokens[start + k].word, qw)))
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
/// An Arabic diacritic or the tatweel — dropped before a word is indexed or
/// looked up.
///
/// SPELLED OUT AS RANGES because `char::is_alphanumeric` will not do it. Every
/// one of these marks carries the Unicode `Other_Alphabetic` property, so
/// `is_alphanumeric` answers TRUE for a bare fatha and the existing filter
/// keeps the whole vowelling. (Python's `isalpha` is category-only and answers
/// false, which makes this an easy thing to check wrong.) `plumbline-core` is
/// dependency-light pure Rust that has to build for wasm, so there is no
/// character-category table to consult — and the corpus is Arabic, so a stated
/// range is both enough and honest about its scope.
pub(crate) fn is_arabic_mark(c: char) -> bool {
    matches!(c,
        '\u{0610}'..='\u{061A}'    // Quranic honorifics
        | '\u{064B}'..='\u{065F}'  // tashkeel: fathatan … wavy hamza below
        | '\u{0640}'               // tatweel, a justification stretch and not a letter
        | '\u{0670}'               // superscript alef
        | '\u{06D6}'..='\u{06ED}'  // Quranic annotation marks
    )
}

/// A GURMUKHI OR DEVANAGARI COMBINING MARK, and the reason it needs naming.
///
/// The mirror image of [`is_arabic_mark`], and the same trap read the other
/// way. There, every mark carries `Other_Alphabetic`, so `normalize_word`'s
/// `is_alphanumeric` filter KEPT a vowelling it should have dropped. Here the
/// VIRAMA — the halant that binds a conjunct, U+094D and U+0A4D — carries
/// neither `Alphabetic` nor `Other_Alphabetic`, so the same filter reads it as
/// punctuation and DELETES IT: परमेश्वर indexed as परमेशवर, ਪਰਮੇਸ਼ੁਰ as
/// ਪਰਮੇਸੁਰ, अन्त and अनत folded onto one key.
///
/// It is applied to both sides, so nothing goes missing from a search — which
/// is exactly why it would never have been noticed. It is still wrong: the
/// virama is part of the word, it is what a keyboard produces for every
/// conjunct in the language, and collapsing it merges words a reader means to
/// tell apart.
///
/// The dependent vowel signs are named here too. They are already `Alphabetic`
/// and survive the filter on their own, but stating the whole mark range rather
/// than the one codepoint that misbehaves is what keeps the next Indic script
/// from rediscovering this: Bengali, Odia and Tamil all have a virama, and none
/// of them is alphanumeric either.
///
/// THE RANGES ARE NOT THE SAME SHAPE IN THE TWO SCRIPTS, and that cost a
/// debugging round: Devanagari's marks run from U+093A so its nukta at U+093C
/// falls inside, while Gurmukhi's matras start at U+0A3E and its nukta at
/// U+0A3C sits two codepoints BELOW them. A range copied across from
/// Devanagari's shape keeps every Hindi nukta and deletes every Punjabi one —
/// including the ਸ਼ of ਪਰਮੇਸ਼ੁਰ, 61% of them.
pub(crate) fn is_indic_mark(c: char) -> bool {
    matches!(c,
        '\u{0900}'..='\u{0903}'   // Devanagari: candrabindu, anusvara, visarga
        | '\u{093A}'..='\u{094F}' // Devanagari: the nukta, matras and the virama
        | '\u{0951}'..='\u{0957}' // Devanagari: accents and extra matras
        | '\u{0962}'..='\u{0963}' // Devanagari: vocalic-l matras
        | '\u{0A01}'..='\u{0A03}' // Gurmukhi: adak bindi, bindi, visarga
        | '\u{0A3C}'..='\u{0A4D}' // Gurmukhi: the nukta, matras, udaat, the virama
        | '\u{0A70}'..='\u{0A71}' // Gurmukhi: tippi and addak
        | '\u{0A75}'              // Gurmukhi: yakash
    )
}

/// A mark that belongs to the letter before it rather than standing on its own.
///
/// The union of the two script predicates, for callers outside search that need
/// the same question answered — [`crate::memory::blank_out`] is the one, and it
/// needs it for a reason worth stating: masking is `is_alphanumeric` too, so
/// every mark these two name comes through a mask VERBATIM. A blanked Hindi
/// word kept its viramas and a blanked Arabic one kept its tashkeel, hanging
/// off an underscore.
pub(crate) fn is_combining_mark(c: char) -> bool {
    is_arabic_mark(c) || is_indic_mark(c)
}

/// The nukta — the subscript dot, U+093C in Devanagari and U+0A3C in Gurmukhi.
///
/// Named separately from [`is_indic_mark`] because the two rules pull opposite
/// ways: every mark in that range is KEPT, and this one is sometimes dropped.
/// See [`fold_indic`] for when.
fn is_indic_nukta(c: char) -> bool {
    matches!(c, '\u{093C}' | '\u{0A3C}')
}

/// A precomposed nukta letter, as its base and its nukta.
///
/// Unicode encodes ਸ਼ and ज़ twice: as a base plus U+0A3C/U+093C, and as single
/// codepoints (U+0A36, U+095B). Both corpora use the DECOMPOSED form
/// exclusively — `check-indic.py` proves NFD is a no-op over each — but a
/// reader's keyboard is not bound by that, and a query typed as U+0A36 must
/// find a word indexed as U+0A38 U+0A3C or Punjabi search fails on its
/// commonest letter.
///
/// The obvious fix, running the text through NFD, is the ONE THING THAT MUST
/// NOT HAPPEN HERE: these codepoints are on Unicode's composition exclusion
/// list, the Punjabi source repo warns in capitals against normalising
/// Gurmukhi, and `build-indic.py` ships the bytes untouched. So the six
/// Gurmukhi and eleven Devanagari mappings are spelled out, applied to search
/// keys only, and the corpus on disk is never touched.
fn indic_decompose(c: char) -> Option<(char, char)> {
    let base = match c {
        // Devanagari
        '\u{0929}' => '\u{0928}', // ऩ
        '\u{0931}' => '\u{0930}', // ऱ
        '\u{0934}' => '\u{0933}', // ऴ
        '\u{0958}' => '\u{0915}', // क़
        '\u{0959}' => '\u{0916}', // ख़
        '\u{095A}' => '\u{0917}', // ग़
        '\u{095B}' => '\u{091C}', // ज़
        '\u{095C}' => '\u{0921}', // ड़
        '\u{095D}' => '\u{0922}', // ढ़
        '\u{095E}' => '\u{092B}', // फ़
        '\u{095F}' => '\u{092F}', // य़
        // Gurmukhi
        '\u{0A33}' => '\u{0A32}', // ਲ਼
        '\u{0A36}' => '\u{0A38}', // ਸ਼
        '\u{0A59}' => '\u{0A16}', // ਖ਼
        '\u{0A5A}' => '\u{0A17}', // ਗ਼
        '\u{0A5B}' => '\u{0A1C}', // ਜ਼
        '\u{0A5E}' => '\u{0A2B}', // ਫ਼
        _ => return None,
    };
    Some((base, if c < '\u{0A00}' { '\u{093C}' } else { '\u{0A3C}' }))
}

/// The letters whose nukta a reader will not reliably type — and, just as
/// important, the ones whose nukta they always will.
///
/// THE DISTINCTION IS MEASURED, NOT ASSUMED, and getting it wrong in the
/// generous direction is what a first pass at this does. "Drop the nukta from
/// both sides, the way Arabic drops its tashkeel" looks like the same fix and
/// is not, because the nukta does two unrelated jobs:
///
///   - On क ख ग ज फ य and ਖ ਗ ਜ ਫ it writes the PERSO-ARABIC sounds of
///     borrowed words — ज़रूर, ਫ਼ਿਲਿਪੁੱਸ. Layouts differ on whether the dot is
///     reachable and readers habitually leave it off, so folding it is Arabic's
///     alef fold in another script: either spelling finds the word.
///   - On ड ढ and ਸ ਲ it writes NATIVE LETTERS. ड़ and ढ़ are the Hindi
///     retroflex flaps of बड़ा and पढ़ना; ਸ਼ is Punjabi sha, the letter in
///     ਪਰਮੇਸ਼ੁਰ and ਵਿਸ਼ਵਾਸ. Nobody types ड for ड़.
///
/// Counted over the two shipped corpora: ड़ and ढ़ are 96.9% of Hindi's 15,899
/// nuktas, and ਸ਼ alone is 61.4% of Punjabi's 31,312. A blanket fold would
/// therefore merge ਸ with ਸ਼ and ड with ड़ through the whole Bible — never a
/// missed hit, since it applies to both sides, and never a distinction the
/// reader could make either.
fn nukta_is_optional(base: char) -> bool {
    matches!(
        base,
        '\u{0915}' | '\u{0916}' | '\u{0917}' | '\u{091C}' | '\u{092B}' | '\u{092F}' // क ख ग ज फ य
        | '\u{0A16}' | '\u{0A17}' | '\u{0A1C}' | '\u{0A2B}' // ਖ ਗ ਜ ਫ
    )
}

/// One pass over a Devanagari or Gurmukhi word: precomposed letters split, and
/// the optional nuktas dropped. See [`nukta_is_optional`] and
/// [`indic_decompose`].
///
/// A string pass rather than a `char` map, because whether a nukta survives
/// depends on the letter BEFORE it.
fn fold_indic(w: &str) -> String {
    let mut out = String::with_capacity(w.len());
    let mut prev: Option<char> = None;
    for c in w.chars() {
        if let Some((base, nukta)) = indic_decompose(c) {
            out.push(base);
            if !nukta_is_optional(base) {
                out.push(nukta);
            }
            prev = Some(base);
            continue;
        }
        // A nukta after a base that does not need one is dropped; `prev` stays
        // the base, so a doubled nukta drops too.
        if is_indic_nukta(c) && prev.is_some_and(nukta_is_optional) {
            continue;
        }
        out.push(c);
        prev = Some(c);
    }
    out
}

/// The letters an Arabic reader will not distinguish when they type.
///
/// The Van Dyck writes ٱ (alef wasla) throughout and nobody has that key; آ أ إ
/// are the same alef under different hamza; ى and ي, ة and ه are routinely
/// typed for one another. This is the normalization every Arabic search does —
/// Lucene's `ArabicNormalizationFilter` is the same list, plus the wasla, which
/// it omits and this text needs more than any of the others.
fn fold_arabic(c: char) -> char {
    match c {
        '\u{0622}' | '\u{0623}' | '\u{0625}' | '\u{0671}' => '\u{0627}', // آ أ إ ٱ → ا
        '\u{0649}' => '\u{064A}',                                        // ى → ي
        '\u{0629}' => '\u{0647}',                                        // ة → ه
        c => c,
    }
}

/// Case and script folding, applied to a word BEFORE it becomes an index key
/// and again to every query term.
///
/// BOTH SIDES OR NEITHER. The index is keyed by this and [`normalize_word`]
/// starts from it, because folding only the query is worse than folding
/// nothing: the reader's typed word loses its tashkeel, every key in the index
/// still has it, and Arabic search returns nothing at all rather than too much.
///
/// A NO-OP ON EVERY LATIN WORD, which is what makes it safe to put on the
/// indexing path. `is_arabic_mark` and `fold_arabic` cannot fire outside the
/// Arabic block, and `fold_indic` is not entered at all unless the word has a
/// Devanagari or Gurmukhi codepoint in it — so English, German and Spanish keys
/// come out of this exactly as `to_lowercase` left them, and their `.idxcache`
/// files, which the web manifest hashes, stay byte-identical.
pub fn fold_word(w: &str) -> String {
    let lower = w.to_lowercase();
    if lower.is_ascii() {
        return lower;
    }
    let folded: String = lower.chars().filter(|&c| !is_arabic_mark(c)).map(fold_arabic).collect();
    // The Indic pass is skipped outright for anything that has no Indic
    // codepoint in it, which is every Latin and Arabic word in the app.
    if folded.chars().any(|c| ('\u{0900}'..='\u{0A7F}').contains(&c)) {
        return fold_indic(&folded);
    }
    folded
}

/// Lowercase and fold, then strip the punctuation a tokenizer would never leave
/// inside a word.
///
/// `is_indic_mark` is an EXCEPTION TO THE FILTER, not another fold: those marks
/// are kept, because `is_alphanumeric` is false for a virama and this filter
/// would otherwise delete the join out of every conjunct in both Indic corpora.
pub fn normalize_word(w: &str) -> String {
    fold_word(w)
        .chars()
        .filter(|&c| c.is_alphanumeric() || is_indic_mark(c) || c == '\'' || c == '\u{2019}' || c == '-')
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
    // "ed" is only tried when there was no "ing" to peel; an "ing" stem that
    // isn't keepable does NOT fall through, it leaves the word whole.
    let peeled = t.strip_suffix("ing").or_else(|| t.strip_suffix("ed")).filter(|s| keepable(s));
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

/// The three buffers [`levenshtein_within`] would otherwise allocate per call:
/// the candidate's chars and the two DP rows. Held by the caller and reused
/// across a whole vocabulary pass.
#[derive(Default)]
struct Lev {
    b: Vec<char>,
    prev: Vec<usize>,
    cur: Vec<usize>,
}

/// [`levenshtein`] between an already-decoded query and a candidate, answered
/// only when the distance is at most `max`.
///
/// Three prunes, cheapest first, because the fuzzy tier asks this about all
/// 12,829 indexed words on one keystroke and keeps a handful — so what matters
/// is how fast a NON-match is rejected:
///
/// 1. the candidate's length, decided while its chars are being decoded, so an
///    over-long word stops being read partway;
/// 2. a row whose cheapest cell already exceeds `max` — no later row can be
///    lower, since each row's minimum is non-decreasing;
/// 3. the final cell.
///
/// For distances inside `max` the answer is the same number [`levenshtein`]
/// gives; the tier only ever asks about those.
fn levenshtein_within(a: &[char], b: &str, max: usize, buf: &mut Lev) -> Option<usize> {
    buf.b.clear();
    for c in b.chars() {
        buf.b.push(c);
        if buf.b.len() > a.len() + max {
            return None;
        }
    }
    let n = buf.b.len();
    if n + max < a.len() {
        return None;
    }

    buf.prev.clear();
    buf.prev.extend(0..=n);
    buf.cur.clear();
    buf.cur.resize(n + 1, 0);
    for (i, &ca) in a.iter().enumerate() {
        buf.cur[0] = i + 1;
        let mut best = buf.cur[0];
        for j in 0..n {
            let cost = usize::from(ca != buf.b[j]);
            let v = (buf.prev[j + 1] + 1).min(buf.cur[j] + 1).min(buf.prev[j] + cost);
            buf.cur[j + 1] = v;
            best = best.min(v);
        }
        if best > max {
            return None;
        }
        std::mem::swap(&mut buf.prev, &mut buf.cur);
    }
    let d = buf.prev[n];
    (d <= max).then_some(d)
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
/// which lives in the optional `plumbline-rnd` crate — here they return a
/// placeholder telling the reader to hydrate it. Ported from `formSearch`
/// (bare-code path; predicate path stubbed pending `plumbline-rnd`).
pub fn form_search(corpus: &Corpus, ix: &SearchIx, fq: &FormQuery) -> SearchAnswer {
    form_search_scoped(corpus, ix, fq, None)
}

/// [`form_search`] narrowed to a scope's verse-index range.
fn form_search_scoped(
    corpus: &Corpus,
    ix: &SearchIx,
    fq: &FormQuery,
    scope: Option<std::ops::Range<usize>>,
) -> SearchAnswer {
    if fq.preds.is_empty() {
        if let Some(s) = &fq.strong {
            // Count the whole scope, show the first cap of it — the same
            // honest-total-over-capped-rows shape the word tiers use.
            let idxs: Vec<usize> =
                ix.lemma_idxs(s).iter().copied().filter(|i| scope.as_ref().is_none_or(|r| r.contains(i))).collect();
            let hits = idxs
                .iter()
                .take(HIT_CAP)
                .filter_map(|&i| {
                    corpus.verse_at(i).map(|v| SearchHit { vref: v.vref(), note: false, why: String::new() })
                })
                .collect();
            return SearchAnswer::Hits { how: format!("verses tagged {s}"), total: idxs.len(), hits };
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
    if let Some(b) = canon::BOOKS.iter().find(|b| b.id.to_lowercase() == needle || b.name.to_lowercase() == needle) {
        return Some(b.id.to_string());
    }
    // unambiguous display-name prefix
    let prefixed: Vec<&str> =
        canon::BOOKS.iter().filter(|b| b.name.to_lowercase().starts_with(&needle)).map(|b| b.id).collect();
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

    /// The Arabic reader can find the word they are looking at.
    ///
    /// FAILS AGAINST THE BUG IT DESCRIBES without touching a shell: before
    /// `fold_word`, `normalize_word` kept every mark — `char::is_alphanumeric`
    /// is TRUE for a fatha, because the tashkeel carry `Other_Alphabetic` — so
    /// "ٱلْبَدْءِ" normalized to itself and the "البدء" a reader can actually
    /// type matched nothing. Each assertion below is one keystroke a real
    /// keyboard produces against one spelling the Van Dyck prints.
    #[test]
    fn arabic_folds_to_what_a_reader_can_type() {
        // Gen 1:1 "the beginning", vowelled, opening on an alef wasla nobody
        // has a key for.
        assert_eq!(normalize_word("ٱلْبَدْءِ"), "البدء");
        // The four alefs collapse onto the bare one.
        for spelling in ["إبراهيم", "أبراهيم", "آبراهيم", "ٱبراهيم"] {
            assert_eq!(normalize_word(spelling), "ابراهيم", "{spelling}");
        }
        // Alef maqsura → ya, ta marbuta → ha.
        assert_eq!(normalize_word("موسى"), "موسي");
        assert_eq!(normalize_word("كلمة"), "كلمه");
        // The tatweel is a typographic stretch, not a letter.
        assert_eq!(normalize_word("رحـــمة"), "رحمه");
        // The INDEX side folds identically, which is the half that makes the
        // query side worth anything.
        assert_eq!(fold_word("ٱلْبَدْءِ"), "البدء");
    }

    /// Folding is invisible on LATIN — the property that lets it sit on the
    /// indexing path.
    ///
    /// The `.idxcache` is the biggest file the web pack ships and the manifest
    /// hashes it, so a fold that perturbed one English key would re-mint every
    /// pack URL and re-download the whole corpus on a release that changed no
    /// data (CLAUDE.md §Data pack). Checked over the vocabulary of every Latin
    /// catalogue rather than a handful of words.
    ///
    /// THE SKIP IS ON SCRIPT, not on direction, and it used to be on direction
    /// because Arabic was the only script that folded. Punjabi and Hindi read
    /// left to right and fold their nukta, so an `is_rtl()` skip would have run
    /// this over two catalogues it was never about and failed on a fold working
    /// exactly as intended.
    #[test]
    fn folding_leaves_latin_keys_exactly_as_lowercasing_did() {
        for lang in crate::i18n::Lang::ALL {
            if lang.script() != crate::i18n::Script::Latin {
                continue;
            }
            for value in crate::i18n::resolved(lang).values() {
                for word in value.split_whitespace() {
                    assert_eq!(fold_word(word), word.to_lowercase(), "{lang:?} {word:?}");
                }
            }
        }
    }

    /// The Punjabi and Hindi reader can find the word they are looking at.
    ///
    /// THREE FIXES, and two of them pull against each other — which is why the
    /// obvious one-line version of this ("strip the marks, like Arabic") is
    /// wrong in both directions at once.
    ///
    ///   - Drop `is_indic_mark` from `normalize_word`'s filter and every
    ///     conjunct loses its join, because `char::is_alphanumeric` is FALSE for
    ///     a virama: परमेश्वर indexed as परमेशवर. It applies to both sides so no
    ///     search breaks — it just quietly merges अन्त with अनत.
    ///   - Fold EVERY nukta and ਸ਼ becomes ਸ, ड़ becomes ड. Those are 61% and
    ///     97% of the nuktas in the two corpora and they are native letters, not
    ///     optional dots (`nukta_is_optional`).
    ///   - Fold NO nukta and ज़रूर stops matching the जरूर a reader types,
    ///     because most layouts do not give them that dot.
    ///
    /// The words are the corpora's own: ਪਰਮੇਸ਼ੁਰ and परमेश्वर are the commonest
    /// nouns in either Bible, and ਫ਼ਿਲਿਪੁੱਸ is Philip in Acts 8:37.
    #[test]
    fn indic_keeps_its_letters_and_forgives_only_the_optional_dot() {
        // The virama survives: these are the words as printed.
        assert_eq!(normalize_word("परमेश्वर"), "परमेश्वर");
        assert_eq!(normalize_word("अर्थात्"), "अर्थात्", "a word may END in a virama");
        assert_ne!(normalize_word("अन्त"), normalize_word("अनत"), "a virama tells two words apart");

        // The NATIVE nukta survives. ਸ਼ is not ਸ and ड़ is not ड.
        assert_eq!(normalize_word("ਪਰਮੇਸ਼ੁਰ"), "ਪਰਮੇਸ਼ੁਰ");
        assert_ne!(fold_word("ਵਿਸ਼ਵਾਸ"), fold_word("ਵਿਸਵਾਸ"));
        assert_ne!(fold_word("बड़ा"), fold_word("बडा"));

        // The BORROWED nukta folds, on the index side and the query side alike.
        assert_eq!(fold_word("ज़रूर"), fold_word("जरूर"));
        assert_eq!(normalize_word("ਫ਼ਿਲਿਪੁੱਸ"), normalize_word("ਫਿਲਿਪੁੱਸ"));

        // A precomposed letter finds the decomposed one the corpus is written
        // in — the reason `indic_decompose` exists. Both directions, because
        // the query may be typed either way.
        assert_eq!(fold_word("\u{0A36}ਬਦ"), fold_word("\u{0A38}\u{0A3C}ਬਦ"), "precomposed ਸ਼");
        assert_eq!(fold_word("ब\u{095C}ा"), fold_word("ब\u{0921}\u{093C}ा"), "precomposed ड़");
        assert_eq!(fold_word("\u{095B}रूर"), fold_word("जरूर"), "precomposed ज़ still folds");

        // The hyphen inside a reduplication is not punctuation to peel — it is
        // how both languages write "चलते-चलते", and `build-indic.py` keeps it in
        // the word for the same reason.
        assert_eq!(normalize_word("चलते-चलते"), "चलते-चलते");
        // A zero-width joiner is neither letter nor mark and comes out.
        assert_eq!(normalize_word("क\u{200D}ख"), "कख");
    }

    /// A suffix that *looks* verbal but leaves nothing keepable behind must
    /// leave the word whole — "king" is not the "-ing" of "k". The peel and the
    /// keepable test are one expression in [`verb`], and a rewrite that peels
    /// first and asks afterwards silently folds every such word onto a stub.
    #[test]
    fn unkeepable_verb_stems_leave_the_word_whole() {
        for w in ["king", "ring", "thing", "bed", "red", "seed"] {
            assert_eq!(stem_word(w), w, "{w} was peeled to a stub");
        }
    }

    /// The scope chips, over the sample's Gen / Ps / John. "God" is in all
    /// three books, so every narrowing has something to drop.
    #[test]
    fn a_scope_narrows_the_hits_and_the_total() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let refs = |scope: SearchScope| match run_search_scoped(&c, &notes, &ix, "God", &scope) {
            Some(SearchAnswer::Hits { total, hits, .. }) => {
                let r: Vec<String> = hits.iter().map(|h| h.vref.ref_key()).collect();
                // The total is the scope's own count, not the corpus-wide one:
                // "4 results" under a chapter chip that shows 3 is the bug this
                // pins.
                assert_eq!(total, r.len(), "total disagrees with the rows for {}", scope.token());
                r
            }
            _ => panic!("expected hits"),
        };

        assert_eq!(refs(SearchScope::All).len(), 4);
        assert_eq!(refs(SearchScope::Book("Gen".into())), ["Gen 1:1", "Gen 1:2", "Gen 1:3"]);
        assert_eq!(refs(SearchScope::Book("John".into())), ["John 3:16"]);
        assert_eq!(refs(SearchScope::Chapter("Gen".into(), 1)).len(), 3);
        assert_eq!(refs(SearchScope::NewTestament), ["John 3:16"]);
        assert_eq!(refs(SearchScope::OldTestament).len(), 3);
        // A chapter the corpus does not have is EMPTY, never everything: a
        // scope that fails to resolve must not silently widen.
        assert!(refs(SearchScope::Chapter("Gen".into(), 99)).is_empty());
        assert!(refs(SearchScope::Book("Nope".into())).is_empty());
    }

    /// A scope filters the widening tiers too — not just the exact one — and a
    /// tier label describes what the reader can actually see. "blessing" is an
    /// exact hit in Gen 1:3 and a stem variant of "blessed" in Gen 1:2.
    #[test]
    fn a_scope_filters_every_tier_and_the_label_follows() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let ask = |q: &str, scope: SearchScope| match run_search_scoped(&c, &notes, &ix, q, &scope) {
            Some(SearchAnswer::Hits { how, total, hits }) => {
                (how, total, hits.iter().map(|h| h.vref.ref_key()).collect::<Vec<_>>())
            }
            _ => panic!("expected hits"),
        };

        // Unscoped: the exact verse leads, the variant follows.
        let (how, _, refs) = ask("blessing", SearchScope::All);
        assert_eq!(how, "verses with the word");
        assert_eq!(refs, ["Gen 1:3", "Gen 1:2"]);

        // Scoped to the verse-2 chapter of a book that has only the VARIANT:
        // scoping to John drops both, and the answer is empty rather than
        // falling back to the corpus.
        let (_, total, refs) = ask("blessing", SearchScope::Book("John".into()));
        assert_eq!((total, refs.len()), (0, 0));

        // A form query (bare Strong's code) is scoped by the same range —
        // H430 tags Gen 1:1, 1:2 and 1:3.
        let (how, total, refs) = ask("H430", SearchScope::Chapter("Gen".into(), 1));
        assert_eq!(how, "verses tagged H430");
        assert_eq!((total, refs.len()), (3, 3));
        let (_, total, _) = ask("H430", SearchScope::NewTestament);
        assert_eq!(total, 0);
    }

    /// A reference query is NAVIGATION and outranks the scope: a reader who
    /// types "John 3" while a Genesis chip is set means "take me there".
    #[test]
    fn a_reference_query_ignores_the_scope() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let ans = run_search_scoped(&c, &notes, &ix, "John 3", &SearchScope::Book("Gen".into()));
        match ans {
            Some(SearchAnswer::GoTo { book, chapter, verse }) => {
                assert_eq!((book.as_str(), chapter, verse), ("John", 3, None));
            }
            other => panic!("expected a goto, got {other:?}"),
        }
    }

    /// A phrase OUTSIDE the scope must not decide the answer inside it.
    ///
    /// The scope has to narrow the multi-word candidates BEFORE the phrase
    /// confirmation picks the tier. Filtering only as rows are pushed leaves
    /// the phrase tier chosen by a verse the reader cannot see, and its
    /// every-word hits — the ones inside the scope — are then never offered at
    /// all: a scoped search that answers "no results" over a verse holding
    /// every word.
    #[test]
    fn a_phrase_outside_the_scope_does_not_silence_the_hits_inside_it() {
        let c = corpus::from_str(concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","the","",[],0],["","word","",[],0],["","of","",[],0],["","God","",[],0]],"v":1}"#,
            "\n",
            r#"{"b":"John","c":1,"t":[["","the","",[],0],["","word","",[],0],["","which","",[],0],["","came","",[],0],["","of","",[],0],["","him","",[],0]],"v":1}"#,
        ))
        .unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let ask = |scope: SearchScope| match run_search_scoped(&c, &notes, &ix, "word of", &scope) {
            Some(SearchAnswer::Hits { how, total, hits }) => {
                (how, total, hits.iter().map(|h| h.vref.ref_key()).collect::<Vec<_>>())
            }
            other => panic!("expected hits, got {other:?}"),
        };

        // Unscoped, Genesis has the phrase and wins the tier.
        let (how, _, refs) = ask(SearchScope::All);
        assert_eq!(how, "verses with the phrase");
        assert_eq!(refs, ["Gen 1:1"]);

        // Scoped to John, whose verse holds both words apart: the answer is
        // John's verse under the every-word label, NOT an empty phrase answer.
        let (how, total, refs) = ask(SearchScope::Book("John".into()));
        assert_eq!(how, "no exact phrase — verses with every word");
        assert_eq!((total, refs), (1, vec!["John 1:1".to_string()]));
    }

    /// A SPAN — the range picker's scope, and what a canon preset resolves to.
    ///
    /// The sample runs Gen 1 · Ps 23 · John 3, so a span can include a middle
    /// book, stop short of one, or be given backwards.
    #[test]
    fn a_span_covers_from_one_chapter_to_another_inclusive() {
        let c = corpus::from_str(SAMPLE).unwrap();
        let ix = ix_of(&c);
        let notes = Notes::new();
        let span = |fb: &str, fc: u16, tb: &str, tc: u16| SearchScope::Span {
            from_book: fb.into(),
            from_chapter: fc,
            to_book: tb.into(),
            to_chapter: tc,
        };
        let refs = |scope: SearchScope| match run_search_scoped(&c, &notes, &ix, "God", &scope) {
            Some(SearchAnswer::Hits { total, hits, .. }) => {
                let r: Vec<String> = hits.iter().map(|h| h.vref.ref_key()).collect();
                assert_eq!(total, r.len(), "total disagrees with the rows for {}", scope.token());
                r
            }
            other => panic!("expected hits, got {other:?}"),
        };

        // Genesis through Psalms: takes Genesis's three, stops before John.
        assert_eq!(refs(span("Gen", 1, "Ps", 23)).len(), 3);
        // The whole canon, spelled as a span.
        assert_eq!(refs(span("Gen", 1, "John", 3)).len(), 4);
        // One book in the middle of the span contributes nothing but is
        // crossed: Psalms has no "God" in this sample.
        assert_eq!(refs(span("Ps", 23, "John", 3)), ["John 3:16"]);

        // BACKWARDS ENDS mean the same span. A reader who fills the far end in
        // first has not asked for nothing.
        assert_eq!(refs(span("John", 3, "Gen", 1)).len(), 4);

        // A chapter past the end of its book clamps to that book's end, which
        // is how "to the end of Revelation" and a preset built against another
        // corpus both arrive.
        assert_eq!(refs(span("Gen", 1, "Gen", 999)).len(), 3);

        // A book this corpus does not have empties the span rather than
        // widening it, like every other unresolvable scope.
        assert!(refs(span("Gen", 1, "Nope", 1)).is_empty());
    }

    /// The testament split is a constant; canon owns the truth of it.
    #[test]
    fn nt_first_order_is_matthew() {
        assert_eq!(canon::book_order("Matt"), Some(NT_FIRST_ORDER));
        assert_eq!(canon::book_ids().nth(NT_FIRST_ORDER), Some("Matt"));
    }

    /// The wire tokens the shells and the FFI pass around round-trip, and
    /// junk parses as `None` (the FFI layer reads that as "no filter").
    #[test]
    fn scope_tokens_roundtrip() {
        for s in [
            SearchScope::All,
            SearchScope::OldTestament,
            SearchScope::NewTestament,
            SearchScope::Book("1Cor".into()),
            SearchScope::Chapter("Ps".into(), 119),
            SearchScope::Span { from_book: "Matt".into(), from_chapter: 1, to_book: "John".into(), to_chapter: 21 },
        ] {
            assert_eq!(SearchScope::parse(&s.token()), Some(s));
        }
        for junk in [
            "",
            "book:",
            "chapter:Gen",
            "chapter:Gen:0",
            "chapter:Gen:x",
            "nonsense",
            "span:Gen",
            "span:Gen:1:John",
            "span:Gen:1:John:x",
            "span:Gen:0:John:3",
            "span::1:John:3",
        ] {
            assert_eq!(SearchScope::parse(junk), None, "{junk} parsed");
        }
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

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::corpus;

    fn tiny() -> Corpus {
        corpus::from_str(concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":3}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","paste","",[],0],["","sat","",[],0]],"v":1}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","caste","",[],0],["","flew","",[],0]],"v":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","the","",[],0],["","paste","",[],0],["","sat","",[],0]],"v":3}"#,
        ))
        .unwrap()
    }

    /// Equal-distance typo candidates must come back in a stable order
    /// (word-alphabetical), not HashMap order.
    #[test]
    fn fuzzy_tier_breaks_distance_ties_deterministically() {
        let c = tiny();
        let ix = SearchIx::build(&c);
        // "haste" is distance 1 from both "caste" and "paste" — the tie breaks
        // alphabetically, so caste's verse (index 1) precedes paste's (0, 2).
        let order = fuzzy_idxs(&ix, "haste");
        assert_eq!(order, vec![1, 0, 2]);
    }

    /// The reworked phrase path still finds phrases and still falls back to
    /// every-word matching.
    #[test]
    fn phrase_and_fallback_survive_the_intersection_rework() {
        let c = tiny();
        let ix = SearchIx::build(&c);
        let notes = Notes::new();

        let Some(SearchAnswer::Hits { how, total, .. }) = run_search(&c, &notes, &ix, "paste sat") else {
            panic!("expected hits");
        };
        assert_eq!(how, "verses with the phrase");
        assert_eq!(total, 2); // v1 and v3 contain "paste sat" consecutively

        let Some(SearchAnswer::Hits { how, total, .. }) = run_search(&c, &notes, &ix, "sat the") else {
            panic!("expected hits");
        };
        assert!(how.starts_with("no exact phrase"));
        assert_eq!(total, 1); // only v3 has both words
    }

    // ── F-12: the tiers stopped materializing what they were about to throw
    // away. `total` still counts every posting; only the rows the reader will
    // see are built. These pin the OBSERVABLE half of that — the hits, their
    // order, their reasons, and the total — because the rewrite is only allowed
    // to be faster.

    /// One verse per tier, so the answer names the whole ladder in one list:
    /// exact, then the margin note, then the stem variant, then the other
    /// rendering of the same Strong's lemma, then the near spelling.
    ///
    /// "alphas" is BOTH a stem variant of the query and one edit away from it,
    /// and "alpha" itself carries H1 — so this is also the dedup test: a verse
    /// a better tier already claimed must not come back under a worse reason.
    fn tiered() -> Corpus {
        corpus::from_str(concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":6}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","alpha","",["H1"],0]],"v":1}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","alpha","",[],0]],"v":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","alphas","",[],0]],"v":3}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","bravo","",["H1"],0]],"v":4}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","alpna","",[],0]],"v":5}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[["","delta","",[],0]],"v":6}"#,
        ))
        .unwrap()
    }

    #[test]
    fn every_tier_keeps_its_place_and_its_reason() {
        let c = tiered();
        let mut ix = SearchIx::build(&c);
        let mut notes = Notes::new();
        notes.insert(VRef::new("Gen", 1, 6), vec!["Heb. alpha".to_string()]);
        ix.attach_notes(&c, &notes);

        let Some(SearchAnswer::Hits { how, total, hits }) = run_search(&c, &notes, &ix, "alpha") else {
            panic!("expected hits");
        };
        assert_eq!(how, "verses with the word");
        assert_eq!(total, 6, "every tier's verse is counted once");
        let got: Vec<(String, bool, String)> = hits.iter().map(|h| (h.vref.ref_key(), h.note, h.why.clone())).collect();
        assert_eq!(
            got,
            vec![
                ("Gen 1:1".to_string(), false, String::new()),
                ("Gen 1:2".to_string(), false, String::new()),
                ("Gen 1:6".to_string(), true, String::new()),
                ("Gen 1:3".to_string(), false, "variant".to_string()),
                ("Gen 1:4".to_string(), false, "also H1".to_string()),
                ("Gen 1:5".to_string(), false, "≈ typo".to_string()),
            ]
        );
    }

    /// A query with its last letter missing is BOTH a prefix of a real word and
    /// a real typo, and the near-spelling tier is the only tier that answers it.
    ///
    /// Skipping that tier for prefix-shaped queries was the tempting half of
    /// F-12 and is deliberately not done, because those two cases are the same
    /// string: measured against the data pack, "lovingkindnes" is a strict
    /// prefix of "lovingkindness" and loses all 26 of its hits under such a
    /// rule. This is that case in miniature, so the rule cannot be added back
    /// without the test saying what it costs.
    #[test]
    fn a_prefix_query_still_gets_its_near_spellings() {
        let c = tiered();
        let ix = SearchIx::build(&c);
        let notes = Notes::new();
        let Some(SearchAnswer::Hits { how, hits, .. }) = run_search(&c, &notes, &ix, "alph") else {
            panic!("expected hits");
        };
        assert_eq!(how, "no exact match — near spellings");
        assert!(hits.iter().all(|h| h.why == "≈ typo"));
        let refs: Vec<String> = hits.iter().map(|h| h.vref.ref_key()).collect();
        assert_eq!(refs, vec!["Gen 1:1", "Gen 1:2"], "\"alph\" is one edit from the indexed \"alpha\"");
    }

    /// The cap bounds the ROWS, never the count: 265 verses match, 200 come
    /// back, and the reader is told 265. They also come back in canon order
    /// from the top — a cap applied at the wrong end would return the tail.
    #[test]
    fn the_cap_truncates_the_rows_and_not_the_total() {
        let mut lines = vec![r#"{"format":"x","tokenization":"kjv1769-tok2","verses":265}"#.to_string()];
        for v in 1..=260 {
            lines.push(format!(r#"{{"b":"Gen","c":1,"t":[["","omega","",[],0]],"v":{v}}}"#));
        }
        for v in 261..=265 {
            lines.push(format!(r#"{{"b":"Gen","c":1,"t":[["","omegas","",[],0]],"v":{v}}}"#));
        }
        let c = corpus::from_str(&lines.join("\n")).unwrap();
        let ix = SearchIx::build(&c);
        let notes = Notes::new();

        let Some(SearchAnswer::Hits { total, hits, .. }) = run_search(&c, &notes, &ix, "omega") else {
            panic!("expected hits");
        };
        assert_eq!(total, 265, "260 exact plus 5 variants, all counted");
        assert_eq!(hits.len(), HIT_CAP);
        assert_eq!(hits[0].vref.ref_key(), "Gen 1:1");
        assert_eq!(hits[HIT_CAP - 1].vref.ref_key(), format!("Gen 1:{HIT_CAP}"));
    }

    /// The budgeted edit distance is the plain one, decided early. Every pair of
    /// strings over a three-letter alphabet up to length four, against every
    /// budget: it must answer exactly when the plain distance is inside the
    /// budget, and answer the same number. An early exit that reads one cell
    /// instead of the row's cheapest rejects real matches, and the fuzzy tier
    /// would quietly lose them.
    #[test]
    fn the_budgeted_edit_distance_agrees_with_the_plain_one() {
        let mut words: Vec<String> = vec![String::new()];
        for _ in 0..4 {
            let grown: Vec<String> = words
                .iter()
                .filter(|w| w.chars().count() < 4)
                .flat_map(|w| "abc".chars().map(move |ch| format!("{w}{ch}")))
                .collect();
            words.extend(grown);
        }
        words.sort();
        words.dedup();
        assert_eq!(words.len(), 121, "the empty string plus 3 + 9 + 27 + 81");

        let mut buf = Lev::default();
        let mut checked = 0usize;
        for a in &words {
            let ac: Vec<char> = a.chars().collect();
            for b in &words {
                let plain = levenshtein(a, b);
                for max in 0..=3 {
                    let want = (plain <= max).then_some(plain);
                    assert_eq!(levenshtein_within(&ac, b, max, &mut buf), want, "{a:?} vs {b:?} within {max}");
                    checked += 1;
                }
            }
        }
        assert_eq!(checked, 121 * 121 * 4);
    }

    /// Where a keystroke's search time actually goes on the real corpus, per
    /// tier and per query shape — the numbers behind F-12. Needs a data pack;
    /// skips itself without one.
    /// `cargo test --release -p plumbline-core -- --ignored --nocapture search_query_profile`
    #[test]
    #[ignore]
    fn search_query_profile() {
        use std::time::Instant;
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(c) = crate::corpus::load_corpus(repo.join("data/kjv.jsonl")) else {
            println!("no data pack; skipping");
            return;
        };
        let notes = crate::notes::load_notes(repo.join("data/kjv-notes.jsonl")).unwrap_or_default();
        let t = Instant::now();
        let mut ix = SearchIx::build(&c);
        let built = t.elapsed().as_millis();
        ix.attach_notes(&c, &notes);
        println!("index: {} verses, {} words, built {built}ms", c.len(), ix.distinct_words());

        // Each query names the shape it stands for; the fuzzy column is the
        // full-vocabulary Levenshtein pass measured on its own.
        let queries: &[(&str, &str)] = &[
            ("god", "3 chars, common, no fuzzy tier at all"),
            ("thes", "4 chars, one edit from the commonest words"),
            ("begi", "4 chars, a prefix mid-typing"),
            ("shephe", "6 chars, a prefix mid-typing"),
            ("beginnig", "8 chars, a real misspelling"),
            ("lovingkindnes", "13 chars, a long misspelling"),
            ("in the beginning", "4-word phrase"),
        ];
        for (q, shape) in queries {
            // Median of three: one keystroke's cost, not the machine's noise.
            let mut runs: Vec<u128> = (0..3)
                .map(|_| {
                    let t = Instant::now();
                    let a = run_search(&c, &notes, &ix, q);
                    let e = t.elapsed().as_micros();
                    std::hint::black_box(a);
                    e
                })
                .collect();
            runs.sort_unstable();
            let total = match run_search(&c, &notes, &ix, q) {
                Some(SearchAnswer::Hits { total, .. }) => total,
                _ => 0,
            };
            let qw = normalize_word(q);
            let fz = if q.split_whitespace().count() == 1 {
                let t = Instant::now();
                let nv = variant_idxs(&ix, &qw).len();
                let tv = t.elapsed().as_micros();
                let t = Instant::now();
                let nl = rendering_idxs(&ix, &qw).len();
                let tl = t.elapsed().as_micros();
                let t = Instant::now();
                let nf = fuzzy_idxs(&ix, &qw).len();
                let tf = t.elapsed().as_micros();
                format!(
                    "variant {:.1}ms/{nv} lemma {:.1}ms/{nl} fuzzy {:.1}ms/{nf}",
                    tv as f64 / 1000.0,
                    tl as f64 / 1000.0,
                    tf as f64 / 1000.0
                )
            } else {
                "one tier only (phrase)".to_string()
            };
            println!("  {q:<17} {:>7.1}ms  total {total:<7} {fz}", runs[1] as f64 / 1000.0);
            println!("  {:<17}   {shape}", "");
        }
    }

    /// attach_notes serves the same rows as the fallback scan.
    #[test]
    fn attached_notes_match_the_fallback_path() {
        let c = tiny();
        let mut ix = SearchIx::build(&c);
        let mut notes = Notes::new();
        notes.insert(crate::VRef::new("Gen", 1, 2), vec!["Heb. Winged Mouse".to_string()]);

        let slow = note_idxs(&c, &notes, &SearchIx::build(&c), "winged");
        ix.attach_notes(&c, &notes);
        let fast = note_idxs(&c, &notes, &ix, "winged");
        assert_eq!(slow, fast);
        assert_eq!(fast, vec![1]);
    }
}
