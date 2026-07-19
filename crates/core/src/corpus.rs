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
        (
            &self.pre,
            &self.word,
            &self.post,
            &self.strongs,
            &self.flags,
        )
            .serialize(s)
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
    tokens
        .into_iter()
        .map(Token::render)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The loaded corpus plus the indices every lookup rides on.
#[derive(Debug, Clone)]
pub struct Corpus {
    verses: Vec<Verse>,
    by_ref: HashMap<VRef, usize>,
    /// book id → highest chapter number.
    chapters: HashMap<String, u16>,
    /// (book id, chapter) → (start index, verse count) — a contiguous slice.
    chapter_ix: HashMap<String, HashMap<u16, (usize, usize)>>,
    tok_version: String,
}

impl Corpus {
    /// All verses in file (canonical) order.
    pub fn verses(&self) -> &[Verse] {
        &self.verses
    }

    /// The verse at an index, if in range.
    pub fn verse_at(&self, i: usize) -> Option<&Verse> {
        self.verses.get(i)
    }

    /// The index of a verse address.
    pub fn index_of(&self, r: &VRef) -> Option<usize> {
        self.by_ref.get(r).copied()
    }

    /// A verse by address.
    pub fn verse(&self, r: &VRef) -> Option<&Verse> {
        self.index_of(r).and_then(|i| self.verses.get(i))
    }

    /// The tokenization version stamped in the file header.
    pub fn tokenization_version(&self) -> &str {
        &self.tok_version
    }

    /// Number of chapters in a book (1 if unknown).
    pub fn chapter_count(&self, book: &str) -> u16 {
        self.chapters.get(book).copied().unwrap_or(1)
    }

    /// The verses of one chapter, in order (empty if the chapter doesn't exist).
    pub fn chapter_verses(&self, book: &str, chapter: u16) -> &[Verse] {
        match self.chapter_ix.get(book).and_then(|m| m.get(&chapter)) {
            Some(&(start, len)) => &self.verses[start..start + len],
            None => &[],
        }
    }

    /// Total verse count.
    pub fn len(&self) -> usize {
        self.verses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.verses.is_empty()
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
    if let Some((len, mtime)) = stamp {
        if let Some(c) = read_cache(&cache_path(path)) {
            if c.src_len == len && c.src_mtime == mtime && c.tok == crate::canon::TOKENIZATION_VERSION {
                return Ok(mk_corpus(c.tok, c.verses));
            }
        }
    }

    // Slow path: parse the JSONL, then write the cache (best-effort — a failed
    // or torn cache write just means the next launch re-parses).
    let raw = std::fs::read_to_string(path).map_err(|e| Error::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let corpus = from_str(&raw)?;
    if let Some((len, mtime)) = stamp {
        let cache = CorpusCacheRef {
            src_len: len,
            src_mtime: mtime,
            tok: &corpus.tok_version,
            verses: &corpus.verses,
        };
        let _ = write_cache(&cache_path(path), &cache);
    }
    Ok(corpus)
}

/// Read + gunzip + decode a corpus cache, or `None` if absent/corrupt/stale.
fn read_cache(path: &Path) -> Option<CorpusCache> {
    use std::io::Read;
    let bytes = std::fs::read(path).ok()?;
    let mut gz = flate2::read::GzDecoder::new(&bytes[..]);
    let mut raw = Vec::new();
    gz.read_to_end(&mut raw).ok()?;
    bincode::deserialize::<CorpusCache>(&raw).ok()
}

/// Encode + gzip + atomically write a corpus cache (best-effort).
fn write_cache<T: Serialize>(path: &Path, cache: &T) -> Result<(), Error> {
    use std::io::Write;
    let raw = bincode::serialize(cache).map_err(|e| Error::Parse(e.to_string()))?;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&raw).map_err(|e| Error::Parse(e.to_string()))?;
    let bytes = gz.finish().map_err(|e| Error::Parse(e.to_string()))?;
    crate::store::write_atomic_bytes(path, &bytes)
}

/// The parsed-corpus cache, keyed to its source file's size + mtime + the
/// tokenization stamp. Any mismatch (regenerated data, changed tokenization)
/// invalidates it and the JSONL is re-parsed.
#[derive(Serialize, Deserialize)]
struct CorpusCache {
    src_len: u64,
    src_mtime: i64,
    tok: String,
    verses: Vec<Verse>,
}

/// Borrowing twin of [`CorpusCache`] for the write path — bincode encodes the
/// same field sequence, so the cache round-trips without deep-cloning ~31k
/// verses just to serialize them.
#[derive(Serialize)]
struct CorpusCacheRef<'a> {
    src_len: u64,
    src_mtime: i64,
    tok: &'a str,
    verses: &'a [Verse],
}

/// `(len, mtime-seconds)` of the source file, or `None` if it can't be stat'd
/// (then the cache is skipped and the JSONL is parsed directly).
fn source_stamp(path: &Path) -> Option<(u64, i64)> {
    let md = std::fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as i64;
    Some((md.len(), mtime))
}

/// `<source>.idxcache`, next to the data file.
fn cache_path(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".idxcache");
    std::path::PathBuf::from(s)
}

/// Parse a corpus from an in-memory JSONL string (header line + verse lines).
pub fn from_str(raw: &str) -> Result<Corpus, Error> {
    let mut lines = raw.lines();
    let header = lines.next().ok_or_else(|| Error::Corpus("corpus file is empty".into()))?;

    let hdr: serde_json::Value = serde_json::from_str(header)
        .map_err(|e| Error::Corpus(format!("bad corpus header: {e}")))?;
    let obj = hdr
        .as_object()
        .ok_or_else(|| Error::Corpus("corpus header is not an object".into()))?;
    let declared = obj
        .get("verses")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| Error::Corpus("header missing verse count".into()))? as usize;
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
        return Err(Error::Corpus(format!(
            "verse count mismatch: header says {declared}, file has {}",
            verses.len()
        )));
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
            let out_of_order = if cur.0 == pb {
                (cur.1, cur.2) <= (pc, pn)
            } else {
                seen.contains(cur.0)
            };
            if out_of_order {
                return Err(Error::Corpus(format!(
                    "corpus not in canonical order at {}",
                    v.vref().ref_key()
                )));
            }
        }
        seen.insert(v.book.as_str());
        prev = Some(cur);
    }
    Ok(())
}

/// Build the corpus indices in one pass. Assumes the stream passed
/// [`check_ascending`], so each chapter's verses form a contiguous run.
fn mk_corpus(tok_version: String, verses: Vec<Verse>) -> Corpus {
    let mut by_ref = HashMap::with_capacity(verses.len());
    let mut chapters: HashMap<String, u16> = HashMap::new();
    let mut chapter_ix: HashMap<String, HashMap<u16, (usize, usize)>> = HashMap::new();

    for (i, v) in verses.iter().enumerate() {
        by_ref.insert(v.vref(), i);
        // Allocate a book-name key only on first sight of the book (~66 books,
        // so ~132 allocations total) rather than cloning it for every verse
        // (~62k). The chapter keys are u16 and never allocate.
        match chapters.get_mut(&v.book) {
            Some(hi) => *hi = (*hi).max(v.chapter),
            None => {
                chapters.insert(v.book.clone(), v.chapter);
            }
        }
        let book_ix = match chapter_ix.get_mut(&v.book) {
            Some(m) => m,
            None => chapter_ix.entry(v.book.clone()).or_default(),
        };
        book_ix
            .entry(v.chapter)
            .and_modify(|(start, len)| {
                *start = (*start).min(i);
                *len += 1;
            })
            .or_insert((i, 1));
    }

    Corpus { verses, by_ref, chapters, chapter_ix, tok_version }
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
        let dir = std::env::temp_dir().join(format!("pure-corpus-cache-{}", std::process::id()));
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
        let good = CorpusCache { src_len: lg, src_mtime: mg, tok: a.tok_version.clone(), verses: a.verses.clone() };
        write_cache(&cache_path(&src), &good).unwrap();
        assert_eq!(load_corpus(&src).unwrap().len(), 3, "matching cache is used despite garbage source");

        // A stale stamp (wrong length) is rejected → the garbage source is then
        // parsed and errors.
        let stale = CorpusCache { src_len: lg + 999, src_mtime: mg, tok: a.tok_version.clone(), verses: a.verses.clone() };
        write_cache(&cache_path(&src), &stale).unwrap();
        assert!(load_corpus(&src).is_err(), "stale-stamp cache rejected → garbage source errors");

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
        let tok = Token {
            pre: "".into(),
            word: "God".into(),
            post: "".into(),
            strongs: vec!["H430".into()],
            flags: 0,
        };
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
