//! Every word the reader sees, in one place, in every language.
//!
//! The shells held about five hundred string literals between them — 147 in the
//! web's Svelte files, 333 in Kotlin — while `panel.rs` was already generating
//! the guide, the about page and every study block from the core. So half the
//! product already worked the way it should and the half that did not was
//! simply never named. This module names it: **user-visible text is core data,
//! and a shell that spells a sentence is a bug** (`crates/core/tests/no_stray_strings.rs`
//! fails the build over it).
//!
//! ## The catalogue
//!
//! One JSON file per language, keyed by stable dotted ids, compiled in with
//! `include_str!`. Compiled rather than downloaded on purpose: a language is
//! not an optional pack, the app must open in the reader's language offline on
//! a first launch, and the whole English catalogue is a few tens of KB.
//!
//! `en` is the SOURCE. A key missing from another language falls back to
//! English rather than showing its own id — a reader who meets one untranslated
//! sentence is inconvenienced; a reader who meets `hymnal.empty` is looking at
//! a crash. `missing()` lists the gaps so the fallback is a safety net and not a
//! hiding place.
//!
//! ## What this is NOT
//!
//! Not the scripture. The Bible text is a corpus, not a string table, and a
//! German Bible is a second corpus with its own tokenization — see
//! `docs/I18N.md`. Not `refKey` either: `VRef::ref_key` is a frozen storage
//! contract and stays `"Gen 1:7"` in every language, while the DISPLAY form
//! localizes ("Joh 3,16" — German writes a comma).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};

/// The languages the app ships. Adding one is a variant, a JSON file and a line
/// in [`Lang::ALL`] — the completeness test then insists it carries every key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];

    /// The BCP-47-ish code that crosses the wire and sits in the reader's config.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }

    /// What this language calls itself — the only honest label for a language
    /// picker, since a reader looking for German is looking for "Deutsch".
    pub fn endonym(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
        }
    }

    /// The language this code names, if the app ships it — tolerating a region
    /// tag, since a browser reporting `de-CH` wants German.
    ///
    /// `None` for anything else, and that distinction matters: [`resolve`] has
    /// to tell "this reader asked for a language we do not have" apart from
    /// "this reader asked for English".
    pub fn shipped(code: &str) -> Option<Lang> {
        let base = code.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == base)
    }

    /// Parse a code, tolerating a region tag. Unknown languages are English,
    /// never an error — a reader with an unsupported locale gets a working app.
    pub fn parse(code: &str) -> Lang {
        Lang::shipped(code).unwrap_or(Lang::En)
    }
}

/// The language to paint in, given the reader's setting and the device's locale.
///
/// An EMPTY setting means "follow the device" — see [`crate::config::Config`] —
/// so a German phone opens in German without anybody visiting Settings, and a
/// reader who later chooses English is not overruled by their own hardware.
/// Both shells call this rather than deciding for themselves, because a rule
/// implemented twice is a rule that disagrees with itself once.
pub fn resolve(chosen: &str, device: &str) -> Lang {
    Lang::shipped(chosen).or_else(|| Lang::shipped(device)).unwrap_or(Lang::En)
}

const EN: &str = include_str!("i18n/en.json");
const DE: &str = include_str!("i18n/de.json");

fn raw(lang: Lang) -> &'static str {
    match lang {
        Lang::En => EN,
        Lang::De => DE,
    }
}

// ── the active language ──────────────────────────────────────────────────────
//
// A PROCESS GLOBAL, and deliberately.
//
// Thirty places in the wire layer turn a `VRef` into something a reader reads —
// search hits, weave endpoints, note headers, thread entries, tag targets,
// occurrence lists — and every one of them would have had to grow a `lang`
// parameter, threaded from an engine handle that several of them do not have.
// Every one of those is a place to forget, and forgetting looks like a German
// app that says "Genesis" in the passage navigator: not a crash, not a test
// failure, just an app that reads as broken.
//
// The honest justification is that this is not really global state — it is a
// property of the ONE READER this process serves. There is no second reader in
// another language, on either shell. An atomic rather than a lock because
// Android may call the ABI from more than one thread and this is read on nearly
// every call; a torn read is impossible for a u8 and a stale one would be a
// single frame in the wrong language during a switch that reloads anyway.

static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// The language `VRef::display` and book names come out in. English until a
/// shell says otherwise, which is what keeps every test and every tool that
/// never sets it reading the way it always did.
pub fn active() -> Lang {
    Lang::ALL.get(ACTIVE.load(Ordering::Relaxed) as usize).copied().unwrap_or(Lang::En)
}

/// The language stamp written into a file the reader authors.
///
/// PROVENANCE, not display. Every user-authored format — notes, threads, tags,
/// weaves, memory cards, the reading map — carries the language its refKeys were
/// written against, and the reason is a door that only stays open if we write it
/// now.
///
/// `refKey` is frozen storage and means a verse in KJV/OSIS numbering. A German
/// Bible (Luther 1912) numbers a hundred-odd verses differently — 3 John, the
/// Joel and Malachi chapter splits, a few others. A reader who picks one language
/// and stays is unaffected: their own notes are self-consistent, and nothing is
/// corrupted. What is affected is the bundled stock study set, which is keyed to
/// KJV numbering, and passages shared between readers of different languages.
///
/// When the versification map lands it will want to migrate exactly the refKeys
/// written under German numbering and leave the rest alone — and that migration
/// is UNRUNNABLE unless the data says which numbering each key meant. Hence the
/// stamp, written before there is a second corpus to need it.
///
/// ABSENT is not the same as `"en"`. An absent stamp means "written by a build
/// that did not record this", which is strictly more information than assuming
/// English, and a migration is entitled to treat the two differently.
///
/// ## Stamped at CREATE, never on re-save
///
/// This is the part that is easy to get wrong, and getting it wrong is worse
/// than not stamping at all. A note written last year carries no stamp. If its
/// German-reading owner edits it and the save writes the CURRENT language, that
/// note now claims German numbering it was never written in — a confident wrong
/// answer where there was an honest absence, and a migration would act on it.
///
/// So [`stamp_new`] only fills an empty slot. Every format already carries a
/// round-trip map for keys it does not model, and a save lifts that map off the
/// file it is replacing, so preserve-on-re-save falls out for free.
///
/// ## Why it lives in that map rather than as a modelled field
///
/// Because nothing in this build reads it. It is written once and carried
/// forward, which is precisely what those maps are for; a real field would mean
/// a `lang` on five domain structs, five construction sites, two hand-written
/// serializers, and five dead-code allows, all to hold a value no code branches
/// on. When something finally does read it — the versification migration — it
/// can be promoted to a field, and serde stops routing it here the moment that
/// field exists.
pub fn stamp() -> String {
    active().code().to_string()
}

/// The extra-keys map a NEWLY CREATED file starts with: just its language stamp.
///
/// Called from the constructors rather than the writers, because a writer also
/// runs on re-save — see [`stamp`].
pub fn stamped_extra() -> serde_json::Map<String, serde_json::Value> {
    let mut m = serde_json::Map::new();
    m.insert("lang".to_string(), serde_json::Value::String(stamp()));
    m
}

/// Set the language for the rest of this process. A shell calls this once, at
/// startup, with what [`resolve`] gave it.
pub fn set_active(lang: Lang) {
    let idx = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    ACTIVE.store(idx as u8, Ordering::Relaxed);
}

/// One language's strings, id → text.
pub type Strings = BTreeMap<String, String>;

/// Parse a catalogue. Panics on malformed JSON, deliberately: the files are
/// compiled into the binary, so a bad one is a build-time mistake that every
/// test will hit immediately, not something a device can encounter.
pub fn catalog(lang: Lang) -> Strings {
    serde_json::from_str(raw(lang)).unwrap_or_else(|e| panic!("i18n/{}.json is not valid: {e}", lang.code()))
}

/// The catalogue a shell should paint with: `lang`'s strings laid over English,
/// so every key is present even where the translation is not.
pub fn resolved(lang: Lang) -> Strings {
    let mut out = catalog(Lang::En);
    if lang != Lang::En {
        out.extend(catalog(lang));
    }
    out
}

/// Keys English has that `lang` does not. Empty for English by definition.
pub fn missing(lang: Lang) -> Vec<String> {
    let en = catalog(Lang::En);
    let mine = catalog(lang);
    en.keys().filter(|k| !mine.contains_key(*k)).cloned().collect()
}

/// Fill `{placeholders}` from `args`.
///
/// A placeholder with no argument is LEFT AS IT IS rather than blanked: "Read
/// through — {book} {chapter}" losing its book silently reads like finished
/// copy, while the braces still on screen are unmistakably a bug and name the
/// argument that went missing.
pub fn format(template: &str, args: &[(&str, &str)]) -> String {
    if !template.contains('{') {
        return template.to_string();
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match args.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                out.push_str(after);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// One string, resolved and formatted. `id` missing from every language comes
/// back as the id itself — visible, greppable, and impossible to mistake for
/// copy.
pub fn t(lang: Lang, id: &str, args: &[(&str, &str)]) -> String {
    let strings = resolved(lang);
    match strings.get(id) {
        Some(s) => format(s, args),
        None => id.to_string(),
    }
}

/// A book's name in `lang`, by OSIS id.
///
/// English is NOT in the catalogue: [`crate::canon::BOOKS`] already holds it,
/// that table is frozen, and parsing reads it too — a second copy in en.json
/// would be two sources for one fact and they would drift. So the catalogue
/// OVERRIDES, and the absence of an override means English.
///
/// Because of that, `missing()` cannot see an untranslated book (there is no
/// English key to be missing against); `every_book_is_named_in_every_language`
/// checks them directly instead.
pub fn book_name(lang: Lang, osis: &str) -> String {
    if lang != Lang::En {
        if let Some(name) = catalog(lang).get(&format!("book.{osis}")) {
            return name.clone();
        }
    }
    crate::canon::display_name(osis).to_string()
}

/// Pick between a one-form and a many-form key.
///
/// DELIBERATELY NOT A PLURAL ENGINE. English and German both split exactly
/// one/other, which covers every language this app ships; a language with more
/// forms (Polish, Russian, Arabic) needs CLDR rules and this function replaced,
/// not extended. Naming that here is cheaper than discovering it later.
pub fn plural(lang: Lang, id_one: &str, id_other: &str, n: u64, args: &[(&str, &str)]) -> String {
    let count = n.to_string();
    let mut all: Vec<(&str, &str)> = vec![("n", &count)];
    all.extend_from_slice(args);
    t(lang, if n == 1 { id_one } else { id_other }, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalogue_parses() {
        for lang in Lang::ALL {
            let c = catalog(lang);
            assert!(!c.is_empty() || lang != Lang::En, "{} is empty", lang.code());
        }
    }

    #[test]
    fn codes_round_trip_and_tolerate_regions() {
        assert_eq!(Lang::parse("de"), Lang::De);
        assert_eq!(Lang::parse("de-CH"), Lang::De);
        assert_eq!(Lang::parse("de_AT"), Lang::De);
        assert_eq!(Lang::parse("DE"), Lang::De);
        assert_eq!(Lang::parse("en-GB"), Lang::En);
        // An unsupported language is English, not an error and not empty.
        assert_eq!(Lang::parse("fr"), Lang::En);
        assert_eq!(Lang::parse(""), Lang::En);
        for lang in Lang::ALL {
            assert_eq!(Lang::parse(lang.code()), lang);
        }
    }

    #[test]
    fn a_setting_beats_the_device_and_an_empty_setting_follows_it() {
        // Nobody has visited Settings: the phone decides.
        assert_eq!(resolve("", "de-DE"), Lang::De);
        assert_eq!(resolve("", "en-US"), Lang::En);
        // A choice was made, and the hardware does not get to overrule it.
        assert_eq!(resolve("en", "de-DE"), Lang::En);
        assert_eq!(resolve("de", "en-US"), Lang::De);
        // A language this build does not ship, in either slot, is not an error.
        assert_eq!(resolve("fr", "de-DE"), Lang::De);
        assert_eq!(resolve("", "fr-FR"), Lang::En);
        assert_eq!(resolve("", ""), Lang::En);
    }

    #[test]
    fn resolved_falls_back_to_english_key_by_key() {
        // Whatever German has or lacks, a resolved catalogue answers every
        // English key — that is the property the shells depend on.
        let en = catalog(Lang::En);
        let de = resolved(Lang::De);
        for k in en.keys() {
            assert!(de.contains_key(k), "resolved de lost the key {k}");
        }
    }

    #[test]
    fn placeholders_fill_and_survive_a_missing_argument() {
        assert_eq!(format("Read {book} {chapter}", &[("book", "John"), ("chapter", "3")]), "Read John 3");
        // Unknown argument: the brace stays, so the bug is on screen.
        assert_eq!(format("Read {book}", &[]), "Read {book}");
        // An unclosed brace is not a panic.
        assert_eq!(format("Read {book", &[("book", "John")]), "Read {book");
        // No placeholders is the common case and copies straight through.
        assert_eq!(format("Hymnal", &[]), "Hymnal");
    }

    #[test]
    fn an_unknown_id_shows_itself() {
        assert_eq!(t(Lang::En, "no.such.key", &[]), "no.such.key");
    }

    #[test]
    fn every_book_is_named_in_every_language() {
        // `missing()` cannot catch these: English keeps its book names in
        // canon.rs, so there is no en.json key for a translation to be missing
        // against. A reader must never meet "Hesekiel" as "Ezek".
        for lang in Lang::ALL {
            if lang == Lang::En {
                continue;
            }
            let c = catalog(lang);
            for b in crate::canon::BOOKS {
                let key = format!("book.{}", b.id);
                assert!(c.contains_key(&key), "{} has no name for {} ({key})", lang.code(), b.name);
            }
        }
    }

    /// Keys deliberately left in English, and the only reason that is allowed.
    ///
    /// The welcome pages are the maintainer's own writing, in the first person
    /// — "I've known many people for whom that prayer has been answered". A
    /// machine draft of that is not a translation, it is words put in
    /// somebody's mouth in a language they cannot check. So these fall back to
    /// English until a person writes them, and the fallback means a German
    /// reader meets English prose rather than a blank page.
    ///
    /// Nothing else may be on this list. Adding a key here is a decision, not a
    /// convenience, and `every_shipped_string_is_translated` is what makes it
    /// one.
    const ENGLISH_ONLY: [&str; 2] = ["intro.welcome.", "intro.curious."];

    #[test]
    fn every_shipped_string_is_translated() {
        for lang in Lang::ALL {
            if lang == Lang::En {
                continue;
            }
            let gaps: Vec<String> =
                missing(lang).into_iter().filter(|k| !ENGLISH_ONLY.iter().any(|p| k.starts_with(p))).collect();
            assert!(
                gaps.is_empty(),
                "{} is missing {} key(s), and only the welcome prose may be missing: {:?}",
                lang.code(),
                gaps.len(),
                gaps
            );
        }
    }

    #[test]
    fn a_translation_never_invents_a_key_english_does_not_have() {
        // The other direction, and the one that rots quietly: a key renamed in
        // en.json leaves its translation behind, where it is dead weight that
        // still LOOKS translated. Nothing reads it, `missing()` cannot see it,
        // and the next person to grep for the id finds the stale German.
        let en = catalog(Lang::En);
        for lang in Lang::ALL {
            if lang == Lang::En {
                continue;
            }
            let orphans: Vec<String> =
                catalog(lang).into_keys().filter(|k| !en.contains_key(k) && !k.starts_with("book.")).collect();
            assert!(orphans.is_empty(), "{} has keys English does not: {:?}", lang.code(), orphans);
        }
    }

    #[test]
    fn a_translated_string_keeps_every_placeholder_it_was_given() {
        // A dropped `{n}` is a sentence that silently loses its number, and it
        // reads as finished copy — "Backed up  files as" is not obviously
        // broken to anybody who does not know what it should say. An ADDED
        // placeholder is worse: no caller supplies it, so the braces reach the
        // screen (see `format`).
        let en = catalog(Lang::En);
        let names = |s: &str| -> Vec<String> {
            let mut out: Vec<String> =
                s.split('{').skip(1).filter_map(|part| part.split_once('}').map(|(n, _)| n.to_string())).collect();
            out.sort();
            out
        };
        for lang in Lang::ALL {
            if lang == Lang::En {
                continue;
            }
            for (key, translated) in catalog(lang) {
                let Some(source) = en.get(&key) else { continue };
                assert_eq!(
                    names(source),
                    names(&translated),
                    "{} {key}: placeholders differ\n  en: {source}\n  {}: {translated}",
                    lang.code(),
                    lang.code()
                );
            }
        }
    }

    #[test]
    fn a_new_file_is_stamped_with_the_language_the_reader_is_reading() {
        // The stamp is the reader's ACTIVE language, not a constant. Every
        // format's create path goes through `stamped_extra`, so this is the one
        // place that has to prove it follows the reader; the per-format tests
        // prove the key lands in the file and that a re-save does not add one.
        //
        // Restores the previous language: this is process-wide state and the
        // tests in this binary share a process.
        let before = active();
        set_active(Lang::De);
        assert_eq!(stamp(), "de");
        assert_eq!(stamped_extra()["lang"], "de");
        set_active(Lang::En);
        assert_eq!(stamped_extra()["lang"], "en");
        set_active(before);
    }

    #[test]
    fn book_names_localize_and_fall_back_to_the_canon_table() {
        assert_eq!(book_name(Lang::En, "Ezek"), "Ezekiel");
        assert_eq!(book_name(Lang::De, "Ezek"), "Hesekiel");
        assert_eq!(book_name(Lang::De, "Gen"), "1. Mose");
        assert_eq!(book_name(Lang::De, "1Cor"), "1. Korinther");
        // An unknown id is not a panic; canon decides what it answers.
        let _ = book_name(Lang::De, "Nope");
    }

    #[test]
    fn plural_picks_a_form_and_lends_it_n() {
        // These ids exist in en.json; the point is the selection and that `n`
        // is available to both forms without the caller passing it.
        assert_eq!(plural(Lang::En, "present.passages.one", "present.passages.other", 1, &[]), "1 passage");
        assert_eq!(plural(Lang::En, "present.passages.one", "present.passages.other", 6, &[]), "6 passages");
        assert_eq!(plural(Lang::En, "present.passages.one", "present.passages.other", 0, &[]), "0 passages");
    }
}
