//! Canonical tokenized text: types, JSON codec, loading, and rendering.
//!
//! Ported from overlay `Corpus.hs`. A verse is a sequence of word tokens;
//! weave/thread spans address words by their index in this sequence, so the
//! token JSON layout and the tokenizer that produced it are frozen (see
//! [`crate::canon::TOKENIZATION_VERSION`]).

use crate::reference::VRef;
use crate::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

// ── token flag bits (stored in JSON, frozen) ────────────────────────────────

/// KJV italics: word supplied by the translators.
pub const FLAG_ADDED: u32 = 1;
/// Divine name (LORD), traditionally small caps.
pub const FLAG_DIVINE: u32 = 2;
/// Part of a canonical superscription (psalm titles).
pub const FLAG_TITLE: u32 = 4;
/// A 1769 paragraph mark (¶) preceded this word.
pub const FLAG_PARA: u32 = 8;

/// One word token: leading punctuation, the word, trailing punctuation, its
/// normalized Strong's refs, and the flag bitfield.
///
/// The on-disk form is the **frozen positional array** `[pre, word, post,
/// [strongs], flags]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub pre: String,
    pub word: String,
    pub post: String,
    pub strongs: Vec<String>,
    pub flags: u32,
}

impl Token {
    /// Whether the given flag bit is set.
    pub fn has_flag(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    /// The token rendered with its surrounding punctuation: pre + word + post.
    pub fn render(&self) -> String {
        format!("{}{}{}", self.pre, self.word, self.post)
    }
}

// Frozen JSON layout: [pre, word, post, [strongs], flags].
type TokenRepr = (String, String, String, Vec<String>, u32);

impl Serialize for Token {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (&self.pre, &self.word, &self.post, &self.strongs, &self.flags).serialize(s)
    }
}

impl<'de> Deserialize<'de> for Token {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (pre, word, post, strongs, flags) = TokenRepr::deserialize(d)?;
        Ok(Token { pre, word, post, strongs, flags })
    }
}

/// One verse: OSIS book id, chapter, verse, and its token stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verse {
    #[serde(rename = "b")]
    pub book: String,
    #[serde(rename = "c")]
    pub chapter: u16,
    #[serde(rename = "v")]
    pub verse: u16,
    #[serde(rename = "t")]
    pub tokens: Vec<Token>,
}

impl Verse {
    /// This verse's address.
    pub fn vref(&self) -> VRef {
        VRef::new(self.book.clone(), self.chapter, self.verse)
    }

    /// Verse text without any superscription (psalm titles excluded).
    pub fn body(&self) -> String {
        render_tokens(self.tokens.iter().filter(|t| !t.has_flag(FLAG_TITLE)))
    }

    /// Superscription text (psalm titles), empty for most verses.
    pub fn title(&self) -> String {
        render_tokens(self.tokens.iter().filter(|t| t.has_flag(FLAG_TITLE)))
    }
}

/// Render a run of tokens space-separated (each token keeps its own
/// punctuation). Ported from `renderTokens`.
pub fn render_tokens<'a, I: IntoIterator<Item = &'a Token>>(tokens: I) -> String {
    tokens.into_iter().map(Token::render).collect::<Vec<_>>().join(" ")
}

/// One chapter's storage: its position in canonical verse order, and either
/// the decoded verses or the byte range of the cache that holds them.
#[derive(Debug, Clone)]
struct ChapterSlot {
    /// Canonical ordinal of this chapter's first verse.
    start_ord: usize,
    /// Byte range within [`Corpus::raw`], for the lazy path.
    span: Option<(usize, usize)>,
    /// Decoded verses; filled on first access when `span` is set.
    cell: OnceLock<Vec<Verse>>,
}

impl ChapterSlot {
    /// The chapter's verses, decoding them on first use. A decode failure
    /// yields an empty chapter rather than a panic — a torn cache degrades to
    /// "this chapter is blank", and the reader can still move.
    fn verses(&self, raw: &[u8]) -> &[Verse] {
        self.cell.get_or_init(|| match self.span {
            Some((off, len)) => bincode::deserialize(&raw[off..off + len]).unwrap_or_default(),
            None => Vec::new(),
        })
    }
}

/// The loaded corpus plus the indices every lookup rides on.
///
/// **Verses decode per chapter, on demand.** Opening the corpus reads only the
/// chapter directory (~1,200 entries); a chapter's ~800 tokens are turned into
/// `Verse`/`Token` structs the first time something asks for them, and stay
/// decoded after that. Materializing the whole canon up front cost ~8 s on a
/// 2026 flagship phone (measured 2026-07-26) — millions of small allocations
/// through wasm, before a single word was on screen — and the reader needs one
/// chapter. The whole-corpus consumers (search, renderings, concept, Strong's
/// occurrences) walk [`Corpus::verses_iter`], which decodes as it goes; they
/// run in the background after first paint.
#[derive(Debug, Clone)]
pub struct Corpus {
    /// The cache bytes chapters decode from; empty on the JSONL path, where
    /// every slot is already filled.
    raw: Vec<u8>,
    /// Chapter slots in canonical order.
    slots: Vec<ChapterSlot>,
    /// book id → highest chapter number.
    chapters: HashMap<String, u16>,
    /// (book id, chapter) → slot index.
    chapter_ix: HashMap<String, HashMap<u16, usize>>,
    /// Total verses across every chapter (known from the directory alone).
    total: usize,
    tok_version: String,
}

impl Corpus {
    /// Every verse in canonical order, decoding chapter by chapter as it goes.
    /// Walking the whole corpus therefore materializes it — that is the price
    /// of a full-text index, and it is paid in the background, not at open.
    pub fn verses_iter(&self) -> impl Iterator<Item = &Verse> {
        self.slots.iter().flat_map(|s| s.verses(&self.raw).iter())
    }

    /// The verse at a canonical index, if in range.
    pub fn verse_at(&self, i: usize) -> Option<&Verse> {
        // Slots are ordered by start_ord, so the owning chapter is a binary
        // search away — no scan, and only that chapter decodes.
        let slot = match self.slots.binary_search_by(|s| s.start_ord.cmp(&i)) {
            Ok(k) => &self.slots[k],
            Err(0) => return None,
            Err(k) => &self.slots[k - 1],
        };
        slot.verses(&self.raw).get(i - slot.start_ord)
    }

    /// The canonical index of a verse address.
    pub fn index_of(&self, r: &VRef) -> Option<usize> {
        let slot = self.slot_of(&r.book, r.chapter)?;
        let at = slot.verses(&self.raw).iter().position(|v| v.verse == r.verse)?;
        Some(slot.start_ord + at)
    }

    /// A verse by address.
    pub fn verse(&self, r: &VRef) -> Option<&Verse> {
        self.slot_of(&r.book, r.chapter)?.verses(&self.raw).iter().find(|v| v.verse == r.verse)
    }

    /// The tokenization version stamped in the file header.
    pub fn tokenization_version(&self) -> &str {
        &self.tok_version
    }

    /// Number of chapters in a book (1 if unknown).
    pub fn chapter_count(&self, book: &str) -> u16 {
        self.chapters.get(book).copied().unwrap_or(1)
    }

    /// The verses of one chapter, in order (empty if the chapter doesn't
    /// exist). This is the reader's path — one chapter's worth of decoding.
    pub fn chapter_verses(&self, book: &str, chapter: u16) -> &[Verse] {
        match self.slot_of(book, chapter) {
            Some(slot) => slot.verses(&self.raw),
            None => &[],
        }
    }

    /// Total verse count — from the directory, without decoding anything.
    pub fn len(&self) -> usize {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    fn slot_of(&self, book: &str, chapter: u16) -> Option<&ChapterSlot> {
        let k = *self.chapter_ix.get(book)?.get(&chapter)?;
        self.slots.get(k)
    }
}

/// Load and validate the corpus from a `kjv.jsonl` file. Ported from
/// `loadCorpus`: parses the header line, decodes each verse line, checks the
/// stream is in canonical order and that the count matches the header.
pub fn load_corpus(path: impl AsRef<Path>) -> Result<Corpus, Error> {
    let path = path.as_ref();
    let stamp = source_stamp(path);

    // Fast path: a valid cache built from this exact source (same length,
    // mtime, and tokenization) — skip re-parsing the ~19 MB of JSONL.
    match stamp {
        Some((len, mtime)) => {
            if let Some(c) = load_cache(cache_path(path), Some((len, mtime))) {
                return Ok(c);
            }
        }
        // No source file at all: the cache IS the corpus. The web ships its
        // pack that way — the raw JSONL would be 2.5 MB of download that
        // nothing ever reads, since the cache supersedes it. Accepted on the
        // tokenization stamp, which is what actually has to match.
        None => {
            if let Some(c) = load_cache(cache_path(path), None) {
                return Ok(c);
            }
        }
    }

    // Slow path: parse the JSONL, then write the cache (best-effort — a failed
    // or torn cache write just means the next launch re-parses).
    let raw = std::fs::read_to_string(path).map_err(|e| Error::Io { path: path.display().to_string(), source: e })?;
    let corpus = from_str(&raw)?;
    if let Some((len, mtime)) = stamp {
        let _ = write_dir_cache(&cache_path(path), &corpus, len, mtime);
    }
    Ok(corpus)
}

/// Open a corpus straight from its cache file, with no `kjv.jsonl` present.
///
/// The web ships the cache in its data pack and the raw JSONL is 2.5 MB of
/// download the reader would never read (2026-07-26): the cache supersedes it.
/// `stamp` is the `(len, mtime)` to validate against when the source file DOES
/// exist; pass `None` to accept the cache on its tokenization stamp alone,
/// which is the shipped-together case.
pub fn load_cache(path: impl AsRef<Path>, stamp: Option<(u64, i64)>) -> Option<Corpus> {
    let bytes = std::fs::read(path.as_ref()).ok()?;
    // ANY tokenization this build ships a corpus for, not just the KJV's: the
    // German corpus has its own stamp and its cache is as valid as the KJV's.
    // The check is "was this written by a tokenizer we agree with", which is
    // what it always meant — it only looked like an equality test while there
    // was one corpus.
    let fresh = |src_len: u64, src_mtime: i64, tok: &str| {
        crate::canon::tokenization_is_ours(tok)
            && stamp.is_none_or(|(len, mtime)| src_len == len && src_mtime == mtime)
    };

    // The chapter-directory format: decode the header, leave the payload
    // alone. This is the whole point — opening costs one directory, not one
    // canon.
    if bytes.starts_with(DIR_CACHE_MAGIC) {
        let head = DIR_CACHE_MAGIC.len();
        let dir_len = u32::from_le_bytes(bytes.get(head..head + 4)?.try_into().ok()?) as usize;
        let dir: DirCache = bincode::deserialize(bytes.get(head + 4..head + 4 + dir_len)?).ok()?;
        if !fresh(dir.src_len, dir.src_mtime, &dir.tok) {
            return None;
        }
        let payload = head + 4 + dir_len;
        let mut b = CorpusBuilder::new(dir.tok);
        for (book, chapter, count, off, len) in &dir.chapters {
            let (off, len) = (payload + *off as usize, *len as usize);
            if off + len > bytes.len() {
                return None; // truncated cache: fall back to the JSONL
            }
            b.push_lazy(book, *chapter, *count as usize, off, len);
        }
        return Some(b.finish(bytes));
    }

    // The original whole-corpus cache, still on devices from earlier versions.
    let c = read_cache_v1(&bytes)?;
    fresh(c.src_len, c.src_mtime, &c.tok).then(|| mk_corpus(c.tok, c.verses))
}

/// Decode the original whole-corpus cache — raw bincode or gzipped bincode,
/// sniffed by the gzip magic so both flavours ever written still load.
fn read_cache_v1(bytes: &[u8]) -> Option<CorpusCache> {
    use std::io::Read;
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut gz = flate2::read::GzDecoder::new(bytes);
        let mut raw = Vec::new();
        gz.read_to_end(&mut raw).ok()?;
        return bincode::deserialize::<CorpusCache>(&raw).ok();
    }
    bincode::deserialize::<CorpusCache>(bytes).ok()
}

/// Serialize a corpus as the chapter-directory cache: `magic | dir_len | dir |
/// payload`, each chapter a separately-decodable bincode `Vec<Verse>`.
///
/// Never gzipped, on any target. The bytes stay resident so chapters can be
/// decoded out of them on demand, and inflating ~18 MB inside wasm on every
/// launch cost real seconds on a phone.
fn encode_dir_cache(corpus: &Corpus, src_len: u64, src_mtime: i64) -> Result<Vec<u8>, Error> {
    // Canonical order FIRST — the payload is laid out in it too, so a rebuilt
    // cache is byte-identical and the slots line up with the verse ordinals the
    // JSONL path produces.
    //
    // Sorting the directory AFTER writing the payload (as this did until
    // 2026-07-28) canonicalizes the entries but NOT the offsets baked into
    // them: those were captured from `chapter_ix`, a HashMap whose iteration
    // order std randomizes per map instance. So every build emitted a
    // semantically identical but byte-different cache — three runs, three
    // pack versions — and because the pack manifest hashes this file, every
    // release re-minted every `?v=` URL and re-downloaded the whole pack.
    // The sort masked the bug from inspection: the directory looked perfectly
    // canonical while the payload behind it did not.
    let mut order: Vec<(&String, u16, usize)> = corpus
        .chapter_ix
        .iter()
        .flat_map(|(book, by_chapter)| by_chapter.iter().map(move |(&chapter, &ix)| (book, chapter, ix)))
        .collect();
    order.sort_by(|&(ba, ca, _), &(bb, cb, _)| {
        let pos = |id: &String| crate::canon::BOOKS.iter().position(|b| b.id == id.as_str()).unwrap_or(usize::MAX);
        // The book id breaks ties: every non-canonical book shares
        // `usize::MAX`, and a stable sort would otherwise fall back to the
        // HashMap order that this function exists to eliminate.
        (pos(ba), ba.as_str(), ca).cmp(&(pos(bb), bb.as_str(), cb))
    });

    let mut payload: Vec<u8> = Vec::new();
    let mut chapters = Vec::with_capacity(order.len());
    for (book, chapter, ix) in order {
        let verses = corpus.slots[ix].verses(&corpus.raw);
        let blob = bincode::serialize(verses).map_err(|e| Error::Parse(e.to_string()))?;
        chapters.push((book.clone(), chapter, verses.len() as u32, payload.len() as u32, blob.len() as u32));
        payload.extend_from_slice(&blob);
    }
    let dir = bincode::serialize(&DirCache { src_len, src_mtime, tok: corpus.tok_version.clone(), chapters })
        .map_err(|e| Error::Parse(e.to_string()))?;

    let mut out = Vec::with_capacity(DIR_CACHE_MAGIC.len() + 4 + dir.len() + payload.len());
    out.extend_from_slice(DIR_CACHE_MAGIC);
    out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    out.extend_from_slice(&dir);
    out.extend_from_slice(&payload);
    Ok(out)
}

fn write_dir_cache(path: &Path, corpus: &Corpus, src_len: u64, src_mtime: i64) -> Result<(), Error> {
    crate::store::write_atomic_bytes(path, &encode_dir_cache(corpus, src_len, src_mtime)?)
}

/// Parse `src` and write `out` as its idxcache, stamped `(len(src), mtime)`.
/// Offline data-prep for shells whose filesystem reports a FIXED mtime — the
/// web's WASI shim reports 0 for every file — so their very first boot takes
/// the cache fast path instead of re-parsing ~19 MB of JSONL (8.4 s on a 2026
/// flagship phone).
pub fn build_cache_stamped(src: impl AsRef<Path>, out: impl AsRef<Path>, src_mtime: i64) -> Result<(), Error> {
    let src = src.as_ref();
    let raw = std::fs::read_to_string(src).map_err(|e| Error::Io { path: src.display().to_string(), source: e })?;
    let corpus = from_str(&raw)?;
    write_dir_cache(out.as_ref(), &corpus, raw.len() as u64, src_mtime)
}

/// The parsed-corpus cache, keyed to its source file's size + mtime + the
/// tokenization stamp. Any mismatch (regenerated data, changed tokenization)
/// invalidates it and the JSONL is re-parsed.
///
/// This is the ORIGINAL whole-corpus layout: one bincode blob of every verse.
/// Still read (devices carry these from earlier versions), never written —
/// decoding it materializes the entire canon, which is exactly what the
/// chapter-directory format below exists to avoid.
#[derive(Serialize, Deserialize)]
struct CorpusCache {
    src_len: u64,
    src_mtime: i64,
    tok: String,
    verses: Vec<Verse>,
}

/// Magic for the chapter-directory cache: "PLBC" + format version.
const DIR_CACHE_MAGIC: &[u8; 8] = b"PLBC0001";

/// The chapter-directory cache's header: the same stamp as [`CorpusCache`],
/// plus one entry per chapter. Opening the corpus decodes ONLY this; each
/// entry's `(off, len)` addresses a bincode `Vec<Verse>` in the payload that
/// follows, decoded when that chapter is first read.
#[derive(Serialize, Deserialize)]
struct DirCache {
    src_len: u64,
    src_mtime: i64,
    tok: String,
    /// (book, chapter, verse count, payload offset, payload length)
    chapters: Vec<(String, u16, u32, u32, u32)>,
}

/// `(len, mtime-seconds)` of the source file, or `None` if it can't be stat'd
/// (then the cache is skipped and the JSONL is parsed directly).
fn source_stamp(path: &Path) -> Option<(u64, i64)> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((md.len(), mtime))
}

/// `<source>.idxcache`, next to the data file.
pub fn cache_path(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".idxcache");
    std::path::PathBuf::from(s)
}

/// Parse a corpus from an in-memory JSONL string (header line + verse lines).
pub fn from_str(raw: &str) -> Result<Corpus, Error> {
    let mut lines = raw.lines();
    let header = lines.next().ok_or_else(|| Error::Corpus("corpus file is empty".into()))?;

    let hdr: serde_json::Value =
        serde_json::from_str(header).map_err(|e| Error::Corpus(format!("bad corpus header: {e}")))?;
    let obj = hdr.as_object().ok_or_else(|| Error::Corpus("corpus header is not an object".into()))?;
    let declared =
        obj.get("verses").and_then(|v| v.as_u64()).ok_or_else(|| Error::Corpus("header missing verse count".into()))?
            as usize;
    let tok_version = obj
        .get("tokenization")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Corpus("header missing tokenization version".into()))?
        .to_string();

    let mut verses = Vec::with_capacity(declared);
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let v: Verse = serde_json::from_str(line).map_err(|e| {
            let snippet: String = line.chars().take(60).collect();
            Error::Corpus(format!("bad verse line: {snippet:?}: {e}"))
        })?;
        verses.push(v);
    }

    check_ascending(&verses)?;
    if verses.len() != declared {
        return Err(Error::Corpus(format!("verse count mismatch: header says {declared}, file has {}", verses.len())));
    }

    Ok(mk_corpus(tok_version, verses))
}

/// Validate the verse stream is grouped by book and strictly ascending by
/// (chapter, verse) within each book, with no book recurring once passed.
/// Ported from `checkAscending`.
fn check_ascending(verses: &[Verse]) -> Result<(), Error> {
    use std::collections::HashSet;
    let mut seen: HashSet<&str> = HashSet::new();
    let mut prev: Option<(&str, u16, u16)> = None;
    for v in verses {
        let cur = (v.book.as_str(), v.chapter, v.verse);
        if let Some((pb, pc, pn)) = prev {
            let out_of_order = if cur.0 == pb { (cur.1, cur.2) <= (pc, pn) } else { seen.contains(cur.0) };
            if out_of_order {
                return Err(Error::Corpus(format!("corpus not in canonical order at {}", v.vref().ref_key())));
            }
        }
        seen.insert(v.book.as_str());
        prev = Some(cur);
    }
    Ok(())
}

/// Build the corpus indices in one pass. Assumes the stream passed
/// [`check_ascending`], so each chapter's verses form a contiguous run.
/// Build a corpus from verses already in memory (the JSONL path): every
/// chapter slot starts filled, so nothing decodes later.
fn mk_corpus(tok_version: String, verses: Vec<Verse>) -> Corpus {
    let mut b = CorpusBuilder::new(tok_version);
    // Verses arrive grouped by book and ascending (checked by the caller), so
    // a chapter's run is contiguous — cut a slot each time the chapter turns.
    let mut run: Vec<Verse> = Vec::new();
    for v in verses {
        if let Some(prev) = run.first() {
            if prev.book != v.book || prev.chapter != v.chapter {
                b.push_decoded(std::mem::take(&mut run));
            }
        }
        run.push(v);
    }
    if !run.is_empty() {
        b.push_decoded(run);
    }
    b.finish(Vec::new())
}

/// Assembles chapter slots plus the book/chapter lookup tables. Shared by the
/// JSONL path (slots pre-filled) and the cache path (slots pointing at byte
/// ranges), so both produce an identical index.
struct CorpusBuilder {
    slots: Vec<ChapterSlot>,
    chapters: HashMap<String, u16>,
    chapter_ix: HashMap<String, HashMap<u16, usize>>,
    total: usize,
    tok_version: String,
}

impl CorpusBuilder {
    fn new(tok_version: String) -> Self {
        Self { slots: Vec::new(), chapters: HashMap::new(), chapter_ix: HashMap::new(), total: 0, tok_version }
    }

    /// Register a chapter, allocating its book key only on first sight.
    fn register(&mut self, book: &str, chapter: u16, count: usize) -> usize {
        match self.chapters.get_mut(book) {
            Some(hi) => *hi = (*hi).max(chapter),
            None => {
                self.chapters.insert(book.to_string(), chapter);
            }
        }
        let ix = self.slots.len();
        let book_ix = match self.chapter_ix.get_mut(book) {
            Some(m) => m,
            None => self.chapter_ix.entry(book.to_string()).or_default(),
        };
        book_ix.insert(chapter, ix);
        let start_ord = self.total;
        self.total += count;
        start_ord
    }

    fn push_decoded(&mut self, verses: Vec<Verse>) {
        let Some(first) = verses.first() else { return };
        let (book, chapter) = (first.book.clone(), first.chapter);
        let start_ord = self.register(&book, chapter, verses.len());
        let cell = OnceLock::new();
        let _ = cell.set(verses);
        self.slots.push(ChapterSlot { start_ord, span: None, cell });
    }

    fn push_lazy(&mut self, book: &str, chapter: u16, count: usize, off: usize, len: usize) {
        let start_ord = self.register(book, chapter, count);
        self.slots.push(ChapterSlot { start_ord, span: Some((off, len)), cell: OnceLock::new() });
    }

    fn finish(self, raw: Vec<u8>) -> Corpus {
        Corpus {
            raw,
            slots: self.slots,
            chapters: self.chapters,
            chapter_ix: self.chapter_ix,
            total: self.total,
            tok_version: self.tok_version,
        }
    }
}

/// The header value written at the top of a `kjv.jsonl`. Ported from
/// `corpusHeader` — kept so the offline importer/`data-prep` can reproduce it.
pub fn corpus_header(tok_version: &str, n_verses: usize) -> serde_json::Value {
    serde_json::json!({
        "format": "overlay-kjv-canonical",
        "tokenization": tok_version,
        "source": "engKJV2006eb 14.3 (CrossWire/eBible.org, public domain)",
        "verses": n_verses,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_cache_roundtrips_and_invalidates() {
        let dir = std::env::temp_dir().join(format!("plumbline-corpus-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("kjv.jsonl");
        std::fs::write(&src, SAMPLE).unwrap();

        // First load parses and writes the cache.
        let a = load_corpus(&src).unwrap();
        assert!(cache_path(&src).exists(), "cache should be written");
        assert_eq!(a.len(), 3);

        // The cache is *used* when its stamp matches the current source: corrupt
        // the source to un-parseable garbage but write a cache whose stamp
        // matches that garbage and carries the good verses — a successful load
        // then proves the cache path (not the parser) ran.
        std::fs::write(&src, b"garbage, not jsonl").unwrap();
        let (lg, mg) = source_stamp(&src).unwrap();
        write_dir_cache(&cache_path(&src), &a, lg, mg).unwrap();
        let cached = load_corpus(&src).unwrap();
        assert_eq!(cached.len(), 3, "matching cache is used despite garbage source");
        // …and it really is the same text, decoded lazily out of the cache.
        assert_eq!(cached.verse(&VRef::new("Gen", 1, 1)).unwrap().body(), "In the beginning God created");
        assert_eq!(cached.chapter_verses("Gen", 1).len(), 2);
        assert_eq!(cached.chapter_count("Gen"), 2);
        assert_eq!(cached.verse_at(2).unwrap().vref(), VRef::new("Gen", 2, 1));
        assert_eq!(cached.index_of(&VRef::new("Gen", 2, 1)), Some(2));
        assert_eq!(cached.verses_iter().count(), 3);

        // A stale stamp (wrong length) is rejected → the garbage source is then
        // parsed and errors.
        write_dir_cache(&cache_path(&src), &a, lg + 999, mg).unwrap();
        assert!(load_corpus(&src).is_err(), "stale-stamp cache rejected → garbage source errors");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A corpus wide enough for ordering to be observable: eight books fed in
    /// neither canonical nor alphabetical order, three chapters each. Twenty-four
    /// directory entries means a HashMap traversal that happens to match
    /// canonical order is a 1-in-24! accident, so the two tests below fail on
    /// the unsorted-payload bug every run rather than most runs.
    fn many_books_jsonl() -> String {
        // Deliberately not canonical order, and not sorted: this is the order
        // the builder registers them in, which is what the old code leaked.
        const FEED: [&str; 8] = ["Rev", "Ps", "Gen", "Matt", "Exod", "John", "Isa", "Lev"];
        let mut verses = 0;
        let mut body = String::new();
        for book in FEED {
            for chapter in 1..=3u16 {
                for v in 1..=2u16 {
                    body.push_str(&format!(
                        r#"{{"b":"{book}","c":{chapter},"t":[["","{book}","",[],0],["","word","",["H430"],0],["","{v}","",[],0]],"v":{v}}}"#,
                    ));
                    body.push('\n');
                    verses += 1;
                }
            }
        }
        format!(
            "{}\n{body}",
            format_args!(r#"{{"format":"overlay-kjv-canonical","tokenization":"kjv1769-tok2","verses":{verses}}}"#)
        )
    }

    /// Split an encoded cache into its directory and payload halves.
    fn split_cache(bytes: &[u8]) -> (DirCache, &[u8]) {
        let head = DIR_CACHE_MAGIC.len();
        let dir_len = u32::from_le_bytes(bytes[head..head + 4].try_into().unwrap()) as usize;
        let dir: DirCache = bincode::deserialize(&bytes[head + 4..head + 4 + dir_len]).unwrap();
        (dir, &bytes[head + 4 + dir_len..])
    }

    #[test]
    fn cache_payload_follows_directory_order() {
        // THE structural invariant. The directory is emitted in canonical order;
        // the payload must be laid out in that SAME order, because each entry's
        // offset is captured while the payload is written. Sorting the directory
        // afterwards (what this code did until 2026-07-28) reorders the entries
        // without relocating the blobs they point at, so the offsets come out in
        // HashMap-traversal order — randomly seeded per map instance, hence a
        // different cache on every build and a fresh pack version every release.
        //
        // Reading offsets in directory order and requiring them to ascend
        // contiguously catches that in a single run: it is exactly the property
        // "the payload is in the order the directory advertises".
        let corpus = from_str(&many_books_jsonl()).unwrap();
        let bytes = encode_dir_cache(&corpus, 0, 0).unwrap();
        let (dir, payload) = split_cache(&bytes);
        assert_eq!(dir.chapters.len(), 24, "eight books, three chapters each");

        let mut at = 0u32;
        for (book, chapter, _verses, offset, len) in &dir.chapters {
            assert_eq!(
                *offset, at,
                "{book} {chapter}: payload offset {offset} is not where directory order puts it ({at}) \
                 — the payload was written in a different order than the directory lists",
            );
            at += len;
        }
        assert_eq!(at as usize, payload.len(), "payload has gaps or trailing slack");

        // And the directory really is canonical, so the check above is anchored
        // to canon order rather than to whatever order happened to be emitted.
        let canonical: Vec<(String, u16)> = {
            let mut v: Vec<(String, u16)> = dir.chapters.iter().map(|(b, c, ..)| (b.clone(), *c)).collect();
            v.sort_by_key(|(b, c)| (crate::canon::BOOKS.iter().position(|k| k.id == b.as_str()).unwrap(), *c));
            v
        };
        let emitted: Vec<(String, u16)> = dir.chapters.iter().map(|(b, c, ..)| (b.clone(), *c)).collect();
        assert_eq!(emitted, canonical, "directory is not in canonical order");
    }

    #[test]
    fn cache_bytes_are_deterministic() {
        // The property the pack manifest depends on: same text in, same bytes
        // out. Two SEPARATE parses, because std seeds each HashMap instance
        // independently — one corpus encoded twice would pass even with the bug.
        let src = many_books_jsonl();
        let a = encode_dir_cache(&from_str(&src).unwrap(), 0, 0).unwrap();
        let b = encode_dir_cache(&from_str(&src).unwrap(), 0, 0).unwrap();
        assert_eq!(a.len(), b.len(), "two builds of one corpus differ in length");
        assert!(
            a == b,
            "two builds of one corpus produced different bytes — the cache is not reproducible, \
             so every release re-mints every pack URL and re-downloads the whole pack",
        );

        // Determinism must not have been bought by making the cache wrong: it
        // still round-trips to the same text.
        let dir = std::env::temp_dir().join(format!("plumbline-corpus-det-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cache = dir.join("kjv.jsonl.idxcache");
        std::fs::write(&cache, &a).unwrap();
        let loaded = load_cache(&cache, None).expect("the deterministic cache still opens");
        assert_eq!(loaded.len(), 48);
        assert_eq!(loaded.verse(&VRef::new("Gen", 2, 1)).unwrap().body(), "Gen word 1");
        assert_eq!(loaded.verse(&VRef::new("Rev", 3, 2)).unwrap().body(), "Rev word 2");
        assert_eq!(loaded.verse_at(0).unwrap().vref(), VRef::new("Gen", 1, 1));
        assert_eq!(loaded.verses_iter().count(), 48);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_a_cache_decodes_no_verses_until_asked() {
        // The point of the chapter-directory format: `load_cache` reads the
        // directory only. Nothing is decoded until a chapter is touched, and
        // then only that chapter — this is what keeps boot off the whole canon.
        let dir = std::env::temp_dir().join(format!("plumbline-corpus-lazy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("kjv.jsonl");
        std::fs::write(&src, SAMPLE).unwrap();
        let parsed = load_corpus(&src).unwrap();
        let cache = dir.join("standalone.idxcache");
        write_dir_cache(&cache, &parsed, 0, 0).unwrap();

        // Opened with no source file in play at all (the web's shipped-pack
        // case): accepted on the tokenization stamp alone.
        let c = load_cache(&cache, None).expect("cache opens without its source");
        assert_eq!(c.len(), 3, "verse count comes from the directory");
        assert_eq!(c.chapter_count("Gen"), 2);
        assert!(c.slots.iter().all(|s| s.cell.get().is_none()), "opening decoded nothing");

        c.chapter_verses("Gen", 1);
        let decoded = c.slots.iter().filter(|s| s.cell.get().is_some()).count();
        assert_eq!(decoded, 1, "reading one chapter decoded exactly one chapter");

        // A cache whose tokenization doesn't match this build is refused.
        let mut bad = std::fs::read(&cache).unwrap();
        let at = bad
            .windows(crate::canon::TOKENIZATION_VERSION.len())
            .position(|w| w == crate::canon::TOKENIZATION_VERSION.as_bytes())
            .expect("stamp is in the directory");
        bad[at] = b'x';
        let other = dir.join("other.idxcache");
        std::fs::write(&other, &bad).unwrap();
        assert!(load_cache(&other, None).is_none(), "a foreign tokenization is refused");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn raw_and_stamped_caches_load() {
        let dir = std::env::temp_dir().join(format!("plumbline-corpus-webcache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("kjv.jsonl");
        std::fs::write(&src, SAMPLE).unwrap();

        // A device carrying the ORIGINAL whole-corpus cache (raw bincode, and
        // the gzipped flavour before it) must still boot — those files are on
        // disk from earlier versions and are never rewritten in that shape.
        let parsed = from_str(SAMPLE).unwrap();
        let (len, mtime) = source_stamp(&src).unwrap();
        let legacy = CorpusCache {
            src_len: len,
            src_mtime: mtime,
            tok: parsed.tok_version.clone(),
            verses: parsed.verses_iter().cloned().collect(),
        };
        let encoded = bincode::serialize(&legacy).unwrap();
        std::fs::write(cache_path(&src), &encoded).unwrap();
        assert_eq!(load_corpus(&src).unwrap().len(), 3, "legacy raw bincode cache is used");
        {
            use std::io::Write;
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            gz.write_all(&encoded).unwrap();
            std::fs::write(cache_path(&src), gz.finish().unwrap()).unwrap();
        }
        assert_eq!(load_corpus(&src).unwrap().len(), 3, "legacy gzipped cache is used");

        // build_cache_stamped writes a loadable cache honouring the given
        // mtime: stamped with the REAL mtime it validates; stamped with the
        // web's fixed 0 it is (correctly) ignored on this filesystem.
        build_cache_stamped(&src, cache_path(&src), mtime).unwrap();
        assert_eq!(load_corpus(&src).unwrap().len(), 3, "stamped cache validates");
        build_cache_stamped(&src, cache_path(&src), 0).unwrap();
        let c = load_corpus(&src).unwrap(); // mtime 0 ≠ real → parses the source
        assert_eq!(c.len(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    const SAMPLE: &str = concat!(
        r#"{"format":"overlay-kjv-canonical","tokenization":"kjv1769-tok2","verses":3}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","In","",[],0],["","the","",[],0],["","beginning","",["H7225"],0],["","God","",["H430"],0],["","created","",["H1254"],0]],"v":1}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","And","",[],0],["","God","",["H430"],0],["","said",",",["H559"],0]],"v":2}"#,
        "\n",
        r#"{"b":"Gen","c":2,"t":[["","Thus","",[],8],["","the","",[],0],["","heavens","",["H8064"],0]],"v":1}"#,
    );

    #[test]
    fn token_json_is_positional_array() {
        let tok = Token { pre: "".into(), word: "God".into(), post: "".into(), strongs: vec!["H430".into()], flags: 0 };
        let json = serde_json::to_string(&tok).unwrap();
        assert_eq!(json, r#"["","God","",["H430"],0]"#);
        let back: Token = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tok);
    }

    #[test]
    fn loads_and_indexes() {
        let c = from_str(SAMPLE).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c.tokenization_version(), "kjv1769-tok2");
        assert_eq!(c.chapter_count("Gen"), 2);
        assert_eq!(c.chapter_verses("Gen", 1).len(), 2);
        assert_eq!(c.chapter_verses("Gen", 2).len(), 1);
        assert_eq!(c.chapter_verses("Gen", 9).len(), 0);

        let v = c.verse(&VRef::new("Gen", 1, 1)).unwrap();
        assert_eq!(v.body(), "In the beginning God created");
        assert_eq!(v.tokens[2].strongs, vec!["H7225".to_string()]);
    }

    #[test]
    fn paragraph_flag_reads() {
        let c = from_str(SAMPLE).unwrap();
        let v = c.verse(&VRef::new("Gen", 2, 1)).unwrap();
        assert!(v.tokens[0].has_flag(FLAG_PARA));
        assert!(!v.tokens[1].has_flag(FLAG_PARA));
    }

    #[test]
    fn rejects_out_of_order() {
        let bad = concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[],"v":2}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[],"v":1}"#,
        );
        assert!(from_str(bad).is_err());
    }

    #[test]
    fn rejects_count_mismatch() {
        let bad = concat!(
            r#"{"format":"x","tokenization":"kjv1769-tok2","verses":5}"#,
            "\n",
            r#"{"b":"Gen","c":1,"t":[],"v":1}"#,
        );
        assert!(from_str(bad).is_err());
    }
}
