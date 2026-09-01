//! Verse references (`VRef`) and the canon-overview segmentation.
//!
//! Ported from overlay `Corpus.hs` (VRef, refKey/parseRefKey) and `Refs.hs`
//! (display text, canon segments).

use crate::canon;
use std::fmt;

/// A verse address: OSIS book id, chapter, verse. Weave links, thread entries
/// and panel targets all speak `VRef`; only the frozen storage formats spell the
/// three parts out.
///
/// The derived `Ord` is alphabetical by book id (matching overlay); canon
/// reading order is [`VRef::reading_key`].
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

    /// The frozen compact ref form, `"Gen 1:7"`. It is inside stored bytes —
    /// thread entries, weave endpoints, saved notes, backup zips — and stays
    /// this shape in every language.
    pub fn ref_key(&self) -> String {
        format!("{} {}:{}", self.book, self.chapter, self.verse)
    }

    /// Human display form in the reader's language, e.g. `"Genesis 1:7"` or
    /// `"1. Mose 1,7"`.
    ///
    /// Reads [`crate::i18n::active`], the process-wide language a shell sets once
    /// at startup, so the wire sites that turn a reference into copy are correct
    /// without knowing a language exists. [`VRef::display_in`] is the explicit
    /// form, for a caller that has a language in hand.
    pub fn display(&self) -> String {
        self.display_in(crate::i18n::active())
    }

    /// Human display form in `lang` — localized book name AND separator. German
    /// writes `Joh 3,16`, with a comma. The whole shape is a catalogue template
    /// (`ref.verse`) rather than a separator constant, because the next language
    /// may not put the book first at all.
    ///
    /// Never confuse this with [`VRef::ref_key`], which is frozen storage.
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

/// A canon section's label in the reader's language.
///
/// The strings in [`CANON_SEGMENTS`] are IDS, not copy: `memory.rs` matches them
/// by value and they key the catalogue here, so they stay English and stable
/// while what the reader sees moves with their language. Every surface that
/// names a section goes through this, so the eight names cannot disagree.
///
/// An id the catalogue does not carry falls back to itself — the English label,
/// so still a name rather than a blank.
pub fn segment_label(id: &str, lang: crate::i18n::Lang) -> String {
    let key = format!("canon.{}", id.to_lowercase());
    let out = crate::i18n::t(lang, &key, &[]);
    if out == key {
        id.to_string()
    } else {
        out
    }
}

/// The index (39) at which the New Testament begins, for the OT/NT seam on the
/// canon map. (`Matt` is book 39; the OT/NT divide sits between 38 and 39.)
pub const OT_NT_DIVIDE: usize = 39;

#[cfg(test)]
mod tests {
    use super::*;

    /// The eight section names are translated while their ids stay English.
    /// Every id must resolve to something the catalogue carries: the helper falls
    /// back to the id, so a missing key shows English instead of failing, and
    /// only a test can tell the difference.
    #[test]
    fn every_canon_section_is_named_in_every_language() {
        use crate::i18n::Lang;
        for (id, _, _) in CANON_SEGMENTS {
            for lang in Lang::ALL {
                let out = segment_label(id, lang);
                assert!(!out.is_empty(), "{id} is unnamed in {}", lang.code());
                assert!(!out.starts_with("canon."), "{id} in {} fell through to the raw key `{out}`", lang.code());
                if lang != Lang::En {
                    assert_ne!(
                        out,
                        id,
                        "{id} is untranslated in {} — the fallback is English, so a \
                         missing key looks like a deliberate choice unless this fails",
                        lang.code()
                    );
                }
            }
        }
        // English reads as itself.
        assert_eq!(segment_label("Gospels", Lang::En), "Gospels");
        // An id the catalogue has never heard of is still a name, not a blank.
        assert_eq!(segment_label("Apocrypha", Lang::En), "Apocrypha");
    }

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
        // German writes a comma, and the book is Johannes: both, or neither —
        // "Johannes 3:16" is still wrong to a German reader.
        assert_eq!(v.display_in(Lang::De), "Johannes 3,16");
        assert_eq!(VRef::new("Ezek", 1, 1).display_in(Lang::De), "Hesekiel 1,1");
        assert_eq!(VRef::new("Gen", 1, 7).display_in(Lang::De), "1. Mose 1,7");

        assert_eq!(v.chapter_display_in(Lang::En), "John 3");
        assert_eq!(v.chapter_display_in(Lang::De), "Johannes 3");

        // The storage key never moves: a German build writing "Johannes 3,16"
        // into a note or a weave endpoint would author data no other build could
        // read.
        assert_eq!(v.ref_key(), "John 3:16");
        for lang in Lang::ALL {
            let _ = v.display_in(lang);
            assert_eq!(v.ref_key(), "John 3:16", "ref_key moved under {}", lang.code());
        }
        assert_eq!(VRef::parse_ref_key(&v.ref_key()), Some(v));
    }
}
