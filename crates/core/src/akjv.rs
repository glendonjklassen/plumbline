//! The plain-English overlay (`akjv.jsonl` / `akjv.akjvb`).
//!
//! Where the American King James Version (Michael Peter Engelbrite, 1999,
//! public domain) words a verse differently from the KJV, this says so — as a
//! DELTA over the KJV's frozen tokens, never as a second text. Each entry is a
//! run of KJV tokens and the phrase the AKJV puts in their place:
//! `[startTok, endTok, "you shall"]`.
//!
//! That shape is the whole point. `kjv.jsonl` and the frozen `kjv1769-tok2`
//! stamp stay untouched; the reader swaps words at layout time; every Strong's
//! code stays attached to the KJV token that owns it (the overlay never moves a
//! code); and "show me the word this replaced" costs nothing, because the
//! original is still sitting in the corpus at that index.
//!
//! **The render rule, which `data-prep/README.md` also states because the
//! producer has to agree with it:** a span `[a,b]` renders as
//! `pre(a) + replacement + post(b)`. The interior punctuation of the tokens
//! `a..b` is dropped — the replacement carries whatever punctuation the AKJV
//! put between its own words (KJV "Verily, verily" → AKJV "Truly, truly").
//!
//! The overlay is a READING aid and nothing else. It must never reach a
//! memory card, a Present hand-off, or copied text: those are the KJV.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::reference::VRef;

/// A run of KJV tokens the AKJV words differently. `text` empty = the AKJV
/// drops the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AkjvSpan {
    pub start: u16,
    pub end: u16,
    pub text: String,
}

/// The whole overlay: per verse, the runs that differ, in token order.
#[derive(Debug, Clone, Default)]
pub struct Akjv {
    ix: HashMap<VRef, Vec<AkjvSpan>>,
    source: String,
}

#[derive(Deserialize)]
struct HeaderWire {
    tokenization: String,
    #[serde(default)]
    source: String,
}

#[derive(Deserialize)]
struct VerseWire {
    b: String,
    c: u16,
    v: u16,
    d: Vec<(u16, u16, String)>,
}

impl Akjv {
    /// Provenance line from the header.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// How many verses carry at least one re-rendering.
    pub fn verse_count(&self) -> usize {
        self.ix.len()
    }

    /// The runs this verse re-renders, in token order.
    pub fn spans(&self, vref: &VRef) -> &[AkjvSpan] {
        self.ix.get(vref).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The run covering `tok`, if the AKJV re-renders it. The reader uses this
    /// twice: to know a word is marked, and to answer "what did this replace?".
    pub fn span_at(&self, vref: &VRef, tok: u16) -> Option<&AkjvSpan> {
        self.spans(vref).iter().find(|s| s.start <= tok && tok <= s.end)
    }

    /// Parse the JSONL form (header line, then one object per verse). `None` on
    /// a tokenization mismatch or an unreadable header — the overlay is
    /// optional, and a stale one must not be shown over the wrong text.
    pub fn parse(tok_version: &str, text: &str) -> Option<Akjv> {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header: HeaderWire = serde_json::from_str(lines.next()?).ok()?;
        if header.tokenization != tok_version {
            return None;
        }
        let mut ix: HashMap<VRef, Vec<AkjvSpan>> = HashMap::new();
        for line in lines {
            let Ok(w) = serde_json::from_str::<VerseWire>(line) else { continue };
            let mut spans: Vec<AkjvSpan> = w
                .d
                .into_iter()
                .filter(|(s, e, _)| s <= e)
                .map(|(start, end, text)| AkjvSpan { start, end, text })
                .collect();
            if spans.is_empty() {
                continue;
            }
            spans.sort_by_key(|s| s.start);
            ix.insert(VRef::new(&w.b, w.c, w.v), spans);
        }
        Some(Akjv { ix, source: header.source })
    }
}

/// Load the overlay, preferring the packed sibling. A home with only the JSONL
/// (an older pack, a hand-built home) still works, and a packed file that will
/// not read falls through to the text.
pub fn load_akjv(tok_version: &str, path: impl AsRef<Path>) -> Option<Akjv> {
    let path = path.as_ref();
    if let Ok(bytes) = std::fs::read(akjvb_path(path)) {
        if let Some(a) = parse_akjv_bin(tok_version, &bytes) {
            return Some(a);
        }
    }
    Akjv::parse(tok_version, &std::fs::read_to_string(path).ok()?)
}

/// `data/akjv.jsonl` → `data/akjv.akjvb`.
pub fn akjvb_path(path: &Path) -> std::path::PathBuf {
    path.with_extension("akjvb")
}

// ── the packed form (`.akjvb`) ────────────────────────────────────────────────
//
// Packed from the start, because the morphology sidecar taught the lesson the
// expensive way: nothing an engine parses survives a browser tab, so a JSONL
// parse is paid on EVERY launch. The shape here makes it nearly free — 46k
// spans draw on only ~3k distinct replacement phrases, so interned they are
// three small integers each.
//
//   0..8    magic "PLAKJV01"
//   8..12   verse_count u32
//   12..16  span_count  u32
//   16..20  text_count  u32
//   20..24  reserved    u32
//   then    tokenization, then source: u32 length + bytes, each padded to 4
//   then    the replacement table: u32 length + bytes each, padded to 4
//   then    verses: book u16, chapter u16, verse u16, n_spans u16
//   then    spans:  start u16, end u16, text u16, pad u16
//
// Book ids share the replacement table; there are 66 of them and interning is
// interning.

const AKJVB_MAGIC: &[u8; 8] = b"PLAKJV01";
const AKJVB_HEADER: usize = 24;

fn push_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

/// Pack the overlay. `None` if it is stale, or larger than the packed index
/// widths allow — in which case the caller keeps shipping the JSONL rather
/// than a silently truncated file.
pub fn encode_akjv_bin(tok_version: &str, text: &str) -> Option<Vec<u8>> {
    let a = Akjv::parse(tok_version, text)?;

    // Deterministic order: the pack manifest hashes this file, so an unstable
    // traversal would churn the pack version on every build.
    let mut refs: Vec<&VRef> = a.ix.keys().collect();
    refs.sort_by(|x, y| (&x.book, x.chapter, x.verse).cmp(&(&y.book, y.chapter, y.verse)));

    let mut texts: Vec<&str> = Vec::new();
    let mut tx: HashMap<&str, u16> = HashMap::new();
    macro_rules! intern {
        ($s:expr) => {{
            let s: &str = $s;
            match tx.get(s) {
                Some(&i) => i,
                None => {
                    let i = u16::try_from(texts.len()).ok()?;
                    texts.push(s);
                    tx.insert(s, i);
                    i
                }
            }
        }};
    }

    let mut verses: Vec<[u16; 4]> = Vec::with_capacity(refs.len());
    let mut spans: Vec<[u16; 4]> = Vec::new();
    for r in &refs {
        let ss = &a.ix[*r];
        let book = intern!(r.book.as_str());
        verses.push([book, r.chapter, r.verse, u16::try_from(ss.len()).ok()?]);
        for s in ss {
            let t = intern!(s.text.as_str());
            spans.push([s.start, s.end, t, 0]);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(AKJVB_MAGIC);
    out.extend_from_slice(&(verses.len() as u32).to_le_bytes());
    out.extend_from_slice(&(spans.len() as u32).to_le_bytes());
    out.extend_from_slice(&(texts.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    push_str(&mut out, tok_version);
    push_str(&mut out, &a.source);
    for t in &texts {
        push_str(&mut out, t);
    }
    for v in &verses {
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    for s in &spans {
        for x in s {
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    Some(out)
}

/// Read the packed form. `None` on a foreign/short/stale file.
pub fn parse_akjv_bin(tok_version: &str, bytes: &[u8]) -> Option<Akjv> {
    if bytes.len() < AKJVB_HEADER || &bytes[..8] != AKJVB_MAGIC {
        return None;
    }
    let u32_at = |o: usize| -> Option<usize> {
        Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?) as usize)
    };
    let verse_count = u32_at(8)?;
    let span_count = u32_at(12)?;
    let text_count = u32_at(16)?;

    let mut at = AKJVB_HEADER;
    let take_str = |at: &mut usize| -> Option<String> {
        let len = u32::from_le_bytes(bytes.get(*at..*at + 4)?.try_into().ok()?) as usize;
        *at += 4;
        let s = std::str::from_utf8(bytes.get(*at..*at + len)?).ok()?.to_string();
        *at += len;
        while *at % 4 != 0 {
            *at += 1;
        }
        Some(s)
    };

    if take_str(&mut at)? != tok_version {
        return None; // stale: addresses a different tokenization
    }
    let source = take_str(&mut at)?;
    let mut texts: Vec<String> = Vec::with_capacity(text_count);
    for _ in 0..text_count {
        texts.push(take_str(&mut at)?);
    }

    let verses_at = at;
    let spans_at = verses_at.checked_add(verse_count.checked_mul(8)?)?;
    if bytes.len() < spans_at.checked_add(span_count.checked_mul(8)?)? {
        return None;
    }
    let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);

    let mut ix: HashMap<VRef, Vec<AkjvSpan>> = HashMap::with_capacity(verse_count);
    let mut s_at = spans_at;
    for i in 0..verse_count {
        let v = verses_at + i * 8;
        let book = texts.get(u16_at(v) as usize)?;
        let vref = VRef::new(book.as_str(), u16_at(v + 2), u16_at(v + 4));
        let n = u16_at(v + 6) as usize;
        let mut ss = Vec::with_capacity(n);
        for _ in 0..n {
            ss.push(AkjvSpan {
                start: u16_at(s_at),
                end: u16_at(s_at + 2),
                text: texts.get(u16_at(s_at + 4) as usize)?.clone(),
            });
            s_at += 8;
        }
        ix.insert(vref, ss);
    }
    Some(Akjv { ix, source })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OVERLAY: &str = concat!(
        r#"{"format":"overlay-akjv-v1","tokenization":"kjv1769-tok2","source":"AKJV 1999 (public domain)"}"#,
        "\n",
        r#"{"b":"John","c":3,"v":3,"d":[[3,3,"to"],[5,6,"Truly, truly"],[9,10,"to you"]]}"#,
        "\n",
        r#"{"b":"Gen","c":1,"v":2,"d":[[7,7,"on"]]}"#,
        "\n",
        r#"{"b":"Ps","c":23,"v":1,"d":[[2,2,""]]}"#,
    );

    fn overlay() -> Akjv {
        Akjv::parse("kjv1769-tok2", OVERLAY).unwrap()
    }

    #[test]
    fn parses_spans_and_finds_them_by_token() {
        let a = overlay();
        assert_eq!(a.verse_count(), 3);
        assert_eq!(a.source(), "AKJV 1999 (public domain)");
        let john = VRef::new("John", 3, 3);
        assert_eq!(a.spans(&john).len(), 3);
        // A single-token span.
        assert_eq!(a.span_at(&john, 3).map(|s| s.text.as_str()), Some("to"));
        // Every token INSIDE a multi-token run resolves to the same span, which
        // is what lets the reader mark the whole run and answer a tap on any
        // word of it.
        assert_eq!(a.span_at(&john, 5).map(|s| s.text.as_str()), Some("Truly, truly"));
        assert_eq!(a.span_at(&john, 6).map(|s| s.text.as_str()), Some("Truly, truly"));
        // An untouched token is not marked.
        assert_eq!(a.span_at(&john, 4), None);
        assert_eq!(a.span_at(&john, 99), None);
        // A verse with no entry at all.
        assert!(a.spans(&VRef::new("Rev", 1, 1)).is_empty());
        // An empty replacement is a DROP, not an absent span.
        assert_eq!(a.span_at(&VRef::new("Ps", 23, 1), 2).map(|s| s.text.as_str()), Some(""));
    }

    #[test]
    fn a_stale_overlay_is_refused() {
        // The whole risk of an overlay: shown over a text it was not aligned
        // to, every span points at the wrong word.
        assert!(Akjv::parse("kjv1611-tok1", OVERLAY).is_none());
        assert!(Akjv::parse("kjv1769-tok2", "").is_none());
        assert!(Akjv::parse("kjv1769-tok2", "not json at all").is_none());
    }

    #[test]
    fn packed_loads_identically_to_the_text() {
        let text = overlay();
        let bytes = encode_akjv_bin("kjv1769-tok2", OVERLAY).expect("encodes");
        let packed = parse_akjv_bin("kjv1769-tok2", &bytes).unwrap();
        assert_eq!(packed.verse_count(), text.verse_count());
        assert_eq!(packed.source(), text.source());
        for r in [VRef::new("John", 3, 3), VRef::new("Gen", 1, 2), VRef::new("Ps", 23, 1)] {
            assert_eq!(packed.spans(&r), text.spans(&r), "{r:?}");
            for tok in 0..12u16 {
                assert_eq!(packed.span_at(&r, tok), text.span_at(&r, tok), "{r:?} tok {tok}");
            }
        }
    }

    #[test]
    fn packing_is_deterministic() {
        let a = encode_akjv_bin("kjv1769-tok2", OVERLAY).unwrap();
        for _ in 0..8 {
            assert_eq!(encode_akjv_bin("kjv1769-tok2", OVERLAY).unwrap(), a);
        }
    }

    #[test]
    fn a_bad_packed_file_is_none_not_garbage() {
        let good = encode_akjv_bin("kjv1769-tok2", OVERLAY).unwrap();
        assert!(parse_akjv_bin("kjv1769-tok2", b"not an akjvb").is_none());
        assert!(parse_akjv_bin("kjv1769-tok2", &good[..good.len() - 8]).is_none());
        assert!(parse_akjv_bin("kjv1769-tok2", &[]).is_none());
        // The stamp rides INSIDE the packed file, so staleness is caught with
        // no JSONL header present.
        assert!(parse_akjv_bin("kjv1611-tok1", &good).is_none());
        assert!(encode_akjv_bin("kjv1611-tok1", OVERLAY).is_none());
    }
}
