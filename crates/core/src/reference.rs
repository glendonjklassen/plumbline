//! Verse references (`VRef`) and the canon-overview segmentation.
//!
//! Ported from overlay `Corpus.hs` (VRef, refKey/parseRefKey) and `Refs.hs`
//! (display text, canon segments).

use crate::canon;
use std::fmt;

/// A verse address: OSIS book id, chapter, verse — the one noun everything in
/// the program points at. Weave links, thread entries, and panel targets all
/// speak `VRef`; only the frozen storage formats spell the three parts out.
///
/// The derived `Ord` is **alphabetical by book id** (matching overlay's derived
/// `Ord`); canon reading order is a separate concern — see [`VRef::reading_key`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VRef {
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
}

impl VRef {
    pub fn new(book: impl Into<String>, chapter: u16, verse: u16) -> Self {
        VRef { book: book.into(), chapter, verse }
    }

    /// Reading-order key: (canon book index, chapter, verse). Used to
    /// canonicalize weave links and sort passages the way a reader expects.
    /// An unknown book sorts last.
    pub fn reading_key(&self) -> (usize, u16, u16) {
        (canon::book_order(&self.book).unwrap_or(usize::MAX), self.chapter, self.verse)
    }

    /// Compact canonical ref form, e.g. `"Gen 1:7"` — used by thread entries
    /// and weave link endpoints, **inside stored/signed bytes**, so the format
    /// is frozen. Ported from `refKey`.
    pub fn ref_key(&self) -> String {
        format!("{} {}:{}", self.book, self.chapter, self.verse)
    }

    /// Human display form in English, e.g. `"Genesis 1:7"`.
    ///
    /// Kept as the no-argument call because most of the codebase predates
    /// languages and English is the source; [`VRef::display_in`] is the one to
    /// reach for anywhere a reader's language is known.
    pub fn display(&self) -> String {
        self.display_in(crate::i18n::Lang::En)
    }

    /// Human display form in `lang` — localized book name AND separator.
    ///
    /// German writes `Joh 3,16`: a comma, not a colon. That is not decoration,
    /// it is what a reference looks like in German, and a colon reads as wrong
    /// there the way `3.16` would in English. The whole shape is a catalogue
    /// template (`ref.verse`) rather than a separator constant, because the next
    /// language may not put the book first at all.
    ///
    /// NEVER confuse this with [`VRef::ref_key`], which is frozen storage and
    /// stays `"Gen 1:7"` in every language — it is inside saved notes, threads,
    /// weave endpoints and backup zips.
    pub fn display_in(&self, lang: crate::i18n::Lang) -> String {
        crate::i18n::t(
            lang,
            "ref.verse",
            &[
                ("book", &crate::i18n::book_name(lang, &self.book)),
                ("chapter", &self.chapter.to_string()),
                ("verse", &self.verse.to_string()),
            ],
        )
    }

    /// A chapter without a verse, e.g. `"John 3"` / `"Johannes 3"`.
    pub fn chapter_display_in(&self, lang: crate::i18n::Lang) -> String {
        crate::i18n::t(
            lang,
            "ref.chapter",
            &[("book", &crate::i18n::book_name(lang, &self.book)), ("chapter", &self.chapter.to_string())],
        )
    }

    /// Parse a compact ref key like `"Gen 1:7"`. Ported from `parseRefKey`.
    pub fn parse_ref_key(s: &str) -> Option<VRef> {
        let s = s.trim();
        let (book, cv) = s.rsplit_once(' ')?;
        let (c, v) = cv.split_once(':')?;
        Some(VRef { book: book.trim().to_string(), chapter: c.parse().ok()?, verse: v.parse().ok()? })
    }
}

impl fmt::Display for VRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ref_key())
    }
}

/// The canon's sections as `(label, first book index, last book index)` over
/// the 66 books in OSIS order — for the shared canon-overview map. Ported from
/// `Refs.canonSegments`.
pub const CANON_SEGMENTS: [(&str, usize, usize); 8] = [
    ("Law", 0, 4),
    ("History", 5, 16),
    ("Wisdom", 17, 21),
    ("Prophets", 22, 38),
    ("Gospels", 39, 42),
    ("Acts", 43, 43),
    ("Letters", 44, 64),
    ("Revelation", 65, 65),
];

/// The index (39) at which the New Testament begins, for the OT/NT seam on the
/// canon map. (`Matt` is book 39; the OT/NT divide sits between 38 and 39.)
pub const OT_NT_DIVIDE: usize = 39;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_key_roundtrip() {
        let r = VRef::new("Gen", 1, 7);
        assert_eq!(r.ref_key(), "Gen 1:7");
        assert_eq!(VRef::parse_ref_key("Gen 1:7"), Some(r));
        assert_eq!(VRef::parse_ref_key("  1Cor 13:4  "), Some(VRef::new("1Cor", 13, 4)));
        assert_eq!(VRef::parse_ref_key("garbage"), None);
        assert_eq!(VRef::parse_ref_key("Gen 1"), None);
    }

    #[test]
    fn display_uses_names() {
        assert_eq!(VRef::new("1Cor", 13, 4).display(), "1 Corinthians 13:4");
    }

    #[test]
    fn reading_order_is_canonical_not_alphabetical() {
        // Alphabetically "Gen" < "John", but in reading order John comes after.
        let gen = VRef::new("Gen", 1, 1);
        let john = VRef::new("John", 3, 16);
        assert!(gen.reading_key() < john.reading_key());
        // Derived Ord is alphabetical (matches overlay): "1Cor" < "Gen".
        assert!(VRef::new("1Cor", 1, 1) < VRef::new("Gen", 1, 1));
    }

    #[test]
    fn a_reference_localizes_its_book_and_its_separator() {
        use crate::i18n::Lang;
        let v = VRef::new("John", 3, 16);

        assert_eq!(v.display(), "John 3:16");
        assert_eq!(v.display_in(Lang::En), "John 3:16");
        // German writes a comma, and the book is Johannes. Both, or neither is
        // worth doing — "Johannes 3:16" is still wrong to a German reader.
        assert_eq!(v.display_in(Lang::De), "Johannes 3,16");
        assert_eq!(VRef::new("Ezek", 1, 1).display_in(Lang::De), "Hesekiel 1,1");
        assert_eq!(VRef::new("Gen", 1, 7).display_in(Lang::De), "1. Mose 1,7");

        assert_eq!(v.chapter_display_in(Lang::En), "John 3");
        assert_eq!(v.chapter_display_in(Lang::De), "Johannes 3");

        // THE STORAGE KEY NEVER MOVES. It is inside saved notes, threads, weave
        // endpoints and backup zips; a German build writing "Johannes 3,16"
        // there would author data no other build could read.
        assert_eq!(v.ref_key(), "John 3:16");
        for lang in Lang::ALL {
            let _ = v.display_in(lang);
            assert_eq!(v.ref_key(), "John 3:16", "ref_key moved under {}", lang.code());
        }
        assert_eq!(VRef::parse_ref_key(&v.ref_key()), Some(v));
    }
}
