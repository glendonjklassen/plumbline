//! The fixed canon: book identities and ordering, plus the tokenization
//! version stamp.
//!
//! Ported from overlay `Canon.hs`. Everything here is part of a **frozen
//! contract** — weave links, thread entries, and (in overlay) signed patches
//! address into tokenized text stamped with [`TOKENIZATION_VERSION`]. Never
//! change existing entries.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Bump only when the tokenizer algorithm changes; data stamped with a
/// different version is refused on load.
///
/// `tok2`: pilcrows (¶) are no longer tokens; they became the paragraph flag
/// on the following word.
pub const TOKENIZATION_VERSION: &str = "kjv1769-tok2";

/// A canonical book: its OSIS id (used in data files and refs), the name as
/// printed by SWORD's `mod2imp`, and the display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Book {
    /// OSIS id, e.g. `"Gen"` — the key used in every data file and ref.
    pub id: &'static str,
    /// Name as printed by `mod2imp` (used only by the offline importer).
    pub imp_name: &'static str,
    /// Display name, e.g. `"1 Corinthians"`.
    pub name: &'static str,
}

/// The 66 books in canonical (OSIS) order. Frozen.
pub const BOOKS: [Book; 66] = [
    Book { id: "Gen", imp_name: "Genesis", name: "Genesis" },
    Book { id: "Exod", imp_name: "Exodus", name: "Exodus" },
    Book { id: "Lev", imp_name: "Leviticus", name: "Leviticus" },
    Book { id: "Num", imp_name: "Numbers", name: "Numbers" },
    Book { id: "Deut", imp_name: "Deuteronomy", name: "Deuteronomy" },
    Book { id: "Josh", imp_name: "Joshua", name: "Joshua" },
    Book { id: "Judg", imp_name: "Judges", name: "Judges" },
    Book { id: "Ruth", imp_name: "Ruth", name: "Ruth" },
    Book { id: "1Sam", imp_name: "I Samuel", name: "1 Samuel" },
    Book { id: "2Sam", imp_name: "II Samuel", name: "2 Samuel" },
    Book { id: "1Kgs", imp_name: "I Kings", name: "1 Kings" },
    Book { id: "2Kgs", imp_name: "II Kings", name: "2 Kings" },
    Book { id: "1Chr", imp_name: "I Chronicles", name: "1 Chronicles" },
    Book { id: "2Chr", imp_name: "II Chronicles", name: "2 Chronicles" },
    Book { id: "Ezra", imp_name: "Ezra", name: "Ezra" },
    Book { id: "Neh", imp_name: "Nehemiah", name: "Nehemiah" },
    Book { id: "Esth", imp_name: "Esther", name: "Esther" },
    Book { id: "Job", imp_name: "Job", name: "Job" },
    Book { id: "Ps", imp_name: "Psalms", name: "Psalms" },
    Book { id: "Prov", imp_name: "Proverbs", name: "Proverbs" },
    Book { id: "Eccl", imp_name: "Ecclesiastes", name: "Ecclesiastes" },
    Book { id: "Song", imp_name: "Song of Solomon", name: "Song of Solomon" },
    Book { id: "Isa", imp_name: "Isaiah", name: "Isaiah" },
    Book { id: "Jer", imp_name: "Jeremiah", name: "Jeremiah" },
    Book { id: "Lam", imp_name: "Lamentations", name: "Lamentations" },
    Book { id: "Ezek", imp_name: "Ezekiel", name: "Ezekiel" },
    Book { id: "Dan", imp_name: "Daniel", name: "Daniel" },
    Book { id: "Hos", imp_name: "Hosea", name: "Hosea" },
    Book { id: "Joel", imp_name: "Joel", name: "Joel" },
    Book { id: "Amos", imp_name: "Amos", name: "Amos" },
    Book { id: "Obad", imp_name: "Obadiah", name: "Obadiah" },
    Book { id: "Jonah", imp_name: "Jonah", name: "Jonah" },
    Book { id: "Mic", imp_name: "Micah", name: "Micah" },
    Book { id: "Nah", imp_name: "Nahum", name: "Nahum" },
    Book { id: "Hab", imp_name: "Habakkuk", name: "Habakkuk" },
    Book { id: "Zeph", imp_name: "Zephaniah", name: "Zephaniah" },
    Book { id: "Hag", imp_name: "Haggai", name: "Haggai" },
    Book { id: "Zech", imp_name: "Zechariah", name: "Zechariah" },
    Book { id: "Mal", imp_name: "Malachi", name: "Malachi" },
    Book { id: "Matt", imp_name: "Matthew", name: "Matthew" },
    Book { id: "Mark", imp_name: "Mark", name: "Mark" },
    Book { id: "Luke", imp_name: "Luke", name: "Luke" },
    Book { id: "John", imp_name: "John", name: "John" },
    Book { id: "Acts", imp_name: "Acts", name: "Acts" },
    Book { id: "Rom", imp_name: "Romans", name: "Romans" },
    Book { id: "1Cor", imp_name: "I Corinthians", name: "1 Corinthians" },
    Book { id: "2Cor", imp_name: "II Corinthians", name: "2 Corinthians" },
    Book { id: "Gal", imp_name: "Galatians", name: "Galatians" },
    Book { id: "Eph", imp_name: "Ephesians", name: "Ephesians" },
    Book { id: "Phil", imp_name: "Philippians", name: "Philippians" },
    Book { id: "Col", imp_name: "Colossians", name: "Colossians" },
    Book { id: "1Thess", imp_name: "I Thessalonians", name: "1 Thessalonians" },
    Book { id: "2Thess", imp_name: "II Thessalonians", name: "2 Thessalonians" },
    Book { id: "1Tim", imp_name: "I Timothy", name: "1 Timothy" },
    Book { id: "2Tim", imp_name: "II Timothy", name: "2 Timothy" },
    Book { id: "Titus", imp_name: "Titus", name: "Titus" },
    Book { id: "Phlm", imp_name: "Philemon", name: "Philemon" },
    Book { id: "Heb", imp_name: "Hebrews", name: "Hebrews" },
    Book { id: "Jas", imp_name: "James", name: "James" },
    Book { id: "1Pet", imp_name: "I Peter", name: "1 Peter" },
    Book { id: "2Pet", imp_name: "II Peter", name: "2 Peter" },
    Book { id: "1John", imp_name: "I John", name: "1 John" },
    Book { id: "2John", imp_name: "II John", name: "2 John" },
    Book { id: "3John", imp_name: "III John", name: "3 John" },
    Book { id: "Jude", imp_name: "Jude", name: "Jude" },
    Book { id: "Rev", imp_name: "Revelation of John", name: "Revelation" },
];

fn by_id() -> &'static HashMap<&'static str, Book> {
    static MAP: OnceLock<HashMap<&'static str, Book>> = OnceLock::new();
    MAP.get_or_init(|| BOOKS.iter().map(|b| (b.id, *b)).collect())
}

fn by_imp_name() -> &'static HashMap<&'static str, Book> {
    static MAP: OnceLock<HashMap<&'static str, Book>> = OnceLock::new();
    MAP.get_or_init(|| BOOKS.iter().map(|b| (b.imp_name, *b)).collect())
}

fn order() -> &'static HashMap<&'static str, usize> {
    static MAP: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    MAP.get_or_init(|| BOOKS.iter().enumerate().map(|(i, b)| (b.id, i)).collect())
}

/// Look up a book by OSIS id.
pub fn book_by_id(id: &str) -> Option<Book> {
    by_id().get(id).copied()
}

/// Look up a book by its `mod2imp` name (offline importer only).
pub fn book_by_imp_name(name: &str) -> Option<Book> {
    by_imp_name().get(name).copied()
}

/// The 0-based canon position of a book id (Gen = 0 … Rev = 65), or `None`.
pub fn book_order(id: &str) -> Option<usize> {
    order().get(id).copied()
}

/// The display name for an OSIS id, falling back to the id itself.
pub fn display_name(id: &str) -> &str {
    book_by_id(id).map(|b| b.name).unwrap_or(id)
}

/// Canon book ids in order.
pub fn book_ids() -> impl Iterator<Item = &'static str> {
    BOOKS.iter().map(|b| b.id)
}

/// The OSIS id `delta` books away from `id` in canon order (`delta` may be
/// negative), or `None` past either end / for an unknown id. Used to roll
/// chapter-stepping across book boundaries (Tier 0 #8).
pub fn adjacent_book(id: &str, delta: i32) -> Option<&'static str> {
    let i = book_order(id)? as i32 + delta;
    if i < 0 || i as usize >= BOOKS.len() {
        None
    } else {
        Some(BOOKS[i as usize].id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_six_books_in_order() {
        assert_eq!(BOOKS.len(), 66);
        assert_eq!(BOOKS[0].id, "Gen");
        assert_eq!(BOOKS[65].id, "Rev");
        assert_eq!(book_order("Gen"), Some(0));
        assert_eq!(book_order("Mal"), Some(38)); // last OT book
        assert_eq!(book_order("Matt"), Some(39)); // first NT book
        assert_eq!(book_order("Rev"), Some(65));
    }

    #[test]
    fn lookups() {
        assert_eq!(book_by_id("1Cor").unwrap().name, "1 Corinthians");
        assert_eq!(book_by_imp_name("Revelation of John").unwrap().id, "Rev");
        assert_eq!(display_name("Ps"), "Psalms");
        assert_eq!(display_name("Nonexistent"), "Nonexistent");
        assert!(book_by_id("nope").is_none());
    }
}
