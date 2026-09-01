//! What a printed Bible in another tradition calls a verse we address the KJV's
//! way — a display concern and nothing more.
//!
//! Every shipped corpus sits at the KJV's own verse addresses: all 66 books,
//! every chapter count, every last verse, 31,102 identical refKeys. So `refKey`
//! means one verse in all of them, and a reader's notes, tags, threads, weaves,
//! memory cards and shared links need no mapping and no migration.
//!
//! What differs is the number printed beside the verse. German tradition breaks
//! 26 books' chapters in slightly different places (357 verses of 31,102), and
//! the Unbound editors, moving the text onto KJV addresses, left the German
//! number in the verse as a `3:19 ` prefix; `build-luther.py` strips those and
//! writes them here.
//!
//! This module only ANNOTATES. Chapter counts, navigation, search results and
//! the reading map stay on KJV numbering, and `VRef::display_in` still says
//! "Maleachi 4,1"; where the traditions disagree, the reader is told what their
//! printed Bible calls the same verse so a reference someone handed them can be
//! found. Renumbering wholesale would touch every site that computes a chapter
//! number, with 26 book boundaries to get off-by-one wrong silently, to fix 1.1%
//! of references.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::i18n::Lang;
use crate::reference::VRef;

/// refKey → the `chapter:verse` a printed Bible in this language shows, parsed
/// from the `osis \t chapter \t verse \t printedRef` table its
/// [`crate::i18n::NumberingSpec`] names. Empty for a language whose tradition
/// agrees with the KJV's breaks, which is most of them.
///
/// One cell per language, so a reader parses only their own table and only once.
fn printed_map(lang: Lang) -> &'static HashMap<String, (u16, u16)> {
    static MAPS: [OnceLock<HashMap<String, (u16, u16)>>; Lang::COUNT] = [const { OnceLock::new() }; Lang::COUNT];
    MAPS[lang as usize].get_or_init(|| {
        let mut m = HashMap::new();
        let Some(spec) = lang.spec().numbering else { return m };
        for line in spec.table.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut f = line.split('\t');
            let (Some(osis), Some(ch), Some(vs), Some(ger)) = (f.next(), f.next(), f.next(), f.next()) else {
                continue;
            };
            let Some((gc, gv)) = ger.trim().split_once(':') else { continue };
            let (Ok(ch), Ok(vs), Ok(gc), Ok(gv)) =
                (ch.parse::<u16>(), vs.parse::<u16>(), gc.parse::<u16>(), gv.parse::<u16>())
            else {
                continue;
            };
            m.insert(format!("{osis} {ch}:{vs}"), (gc, gv));
        }
        m
    })
}

/// What a printed Bible in `lang`'s tradition calls this verse — `None` when it
/// agrees with ours, which is 98.9% of the canon.
///
/// Already formatted for the language, so German gets its comma: `"3,19"`.
pub fn printed_as(lang: Lang, vref: &VRef) -> Option<String> {
    let (c, v) = printed_map(lang).get(&vref.ref_key()).copied()?;
    // Same separator rule as a full reference: `ref.chapterVerse` is a comma in
    // German and a colon in English.
    Some(crate::i18n::t(lang, "ref.chapterVerse", &[("chapter", &c.to_string()), ("verse", &v.to_string())]))
}

/// The annotation to show beside a reference, e.g. `"Luther 3,19"`. `None` when
/// the two traditions agree.
pub fn printed_note(lang: Lang, vref: &VRef) -> Option<String> {
    let printed = printed_as(lang, vref)?;
    // Whose numbering, from the language's own row rather than baked into the
    // `ref.printedAs` sentence — otherwise the next language's annotation
    // credits its verse numbers to Luther.
    let tradition = lang.spec().numbering.map(|n| n.label).unwrap_or_default();
    Some(crate::i18n::t(lang, "ref.printedAs", &[("tradition", tradition), ("ref", &printed)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_loads_and_is_the_size_the_corpus_produced() {
        // 357 is what `build-luther.py` extracted. A table that silently parsed
        // to a handful of entries would annotate almost nothing and look like
        // "the traditions agree" — the failure worth catching.
        let m = printed_map(Lang::De);
        assert!(m.len() > 300, "the Luther numbering table has only {} entries", m.len());
    }

    #[test]
    fn the_french_and_chinese_tables_load_at_their_produced_sizes() {
        // 1,263 is what `build-ostervald.py` derived (psalm titles shift 983
        // addresses on their own); 22 is `build-cuv.py`'s. Same silent-shrink
        // failure mode as the Luther test above.
        assert!(
            printed_map(Lang::Fr).len() > 1200,
            "the Ostervald table has only {} entries",
            printed_map(Lang::Fr).len()
        );
        assert_eq!(printed_map(Lang::Zht).len(), 22, "the CUV table changed size");
        // One tradition, one table: both Chinese rows must annotate alike.
        assert_eq!(printed_map(Lang::Zht), printed_map(Lang::Zhs));
    }

    #[test]
    fn the_new_classic_disagreements_are_named() {
        // French: the fish is 2:1 ("Jonas 2:1" begins the psalm chapter a
        // French reader knows), and a title psalm's body shifts by one.
        assert_eq!(printed_map(Lang::Fr).get("Jonah 1:17").copied(), Some((2, 1)));
        assert_eq!(printed_map(Lang::Fr).get("Ps 3:1").copied(), Some((3, 2)));
        // Chinese: the Pericope's opener is printed inside 8:1, and KJV
        // 1 Chr 22:1 sits at the end of the printed 21.
        assert_eq!(printed_map(Lang::Zht).get("John 7:53").copied(), Some((8, 1)));
        assert_eq!(printed_map(Lang::Zhs).get("1Chr 22:1").copied(), Some((21, 31)));
        // And where the traditions agree, nothing is said — in any of them.
        assert_eq!(printed_as(Lang::Fr, &VRef::new("John", 3, 16)), None);
        assert_eq!(printed_as(Lang::Zht, &VRef::new("John", 3, 16)), None);
    }

    #[test]
    fn a_language_whose_tradition_agrees_has_no_table_and_that_is_not_a_gap() {
        // Reina-Valera follows the KJV's breaks, so Spanish carries no numbering
        // row, and the absence must behave like agreement rather than a missing
        // file — an empty map answering `Some` would put a bogus "printed as"
        // line on a Spanish study card.
        assert!(printed_map(Lang::Es).is_empty(), "Spanish grew a numbering table without one being written");
        assert_eq!(printed_as(Lang::Es, &VRef::new("Mal", 4, 1)), None);
        assert_eq!(printed_note(Lang::Es, &VRef::new("Joel", 3, 1)), None);
    }

    #[test]
    fn the_classic_disagreements_are_named() {
        // Malachi: the KJV's chapter 4 is the tail of German chapter 3.
        assert_eq!(printed_as(Lang::De, &VRef::new("Mal", 4, 1)), Some("3,19".to_string()));
        // Joel: the one book where German counts a different number of chapters.
        assert_eq!(printed_as(Lang::De, &VRef::new("Joel", 2, 28)), Some("3,1".to_string()));
        assert_eq!(printed_as(Lang::De, &VRef::new("Joel", 3, 1)), Some("4,1".to_string()));
        // A verse on the other side of a chapter break.
        assert_eq!(printed_as(Lang::De, &VRef::new("Gen", 31, 55)), Some("32,1".to_string()));
    }

    #[test]
    fn where_the_traditions_agree_nothing_is_said() {
        // The overwhelming majority: John 3:16 is John 3:16 in both.
        assert_eq!(printed_as(Lang::De, &VRef::new("John", 3, 16)), None);
        assert_eq!(printed_as(Lang::De, &VRef::new("Rom", 5, 8)), None);
        // And an English reader is never told about German numbering at all.
        assert_eq!(printed_as(Lang::En, &VRef::new("Mal", 4, 1)), None);
        assert_eq!(printed_note(Lang::En, &VRef::new("Mal", 4, 1)), None);
    }

    #[test]
    fn the_note_reads_as_a_reference_a_reader_can_look_up() {
        let note = printed_note(Lang::De, &VRef::new("Mal", 4, 1)).expect("Malachi 4:1 disagrees");
        assert!(note.contains("3,19"), "the note does not carry the German number: {note}");
        assert!(note.contains("Luther"), "the note does not say whose numbering it is: {note}");
    }
}
