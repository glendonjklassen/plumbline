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
use std::sync::OnceLock;

/// The languages the app ships.
///
/// **Adding one is a variant here and a row in [`SPECS`] beside it**, plus the
/// data files that row names. Nothing else. The corpus the engine opens, the
/// Strong's dictionary it prefers, the printed-numbering annotation, the pack
/// files each shell downloads, the assets Android bundles and the tokenization
/// allow-list all READ THE ROW.
///
/// That is worth stating because it was not true until Spanish was added.
/// German lived in a dozen hardcoded sites that each knew a little about it —
/// `corpus_for` matching `Lang::De`, a config field named `strongs_de_off`, a
/// `germanCorpus` pack role, a `GERMAN_CACHE` constant, `if (code === "de")` in
/// the web's Settings, a hand-maintained Android asset list — and none of them
/// knew about each other. `a_row_is_complete` and
/// `every_shipped_string_is_translated` are what keep a new row from being
/// half-filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lang {
    En,
    De,
    Es,
}

/// The scripture a language reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorpusSpec {
    /// The file under the home's `data/`, e.g. `"luther1912.jsonl"`.
    pub file: &'static str,
    /// The tokenization stamp its header must carry. Token indices are
    /// per-corpus by nature — the same verse tokenizes into different words at
    /// different indices in every translation — so each text has its own, and
    /// [`crate::canon::tokenization_is_ours`] is this column.
    pub tokenization: &'static str,
    /// What a reader would call this Bible: the label on a rendering list, and
    /// the name beside a verse number that differs from the KJV's.
    pub label: &'static str,
}

impl CorpusSpec {
    /// The start-up cache beside it. DERIVED, never spelled twice: these two
    /// names drifting apart is a boot that silently re-parses ~19 MB of JSONL
    /// and looks only like "the app got slower".
    pub fn cache_file(&self) -> String {
        format!("{}.idxcache", self.file)
    }
}

/// A language's Strong's dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexiconSpec {
    /// The file under `data/`, e.g. `"strongs-de.json"`.
    pub file: &'static str,
    /// Whether its DEFINITIONS are machine-translated, which the study card
    /// says on screen.
    ///
    /// A FACT ABOUT THE SHIPPED FILE, like its tokenization is a fact about a
    /// corpus, because the two halves of a localized dictionary arrive by
    /// different routes: the renderings are derived from that language's own
    /// tagged corpus the moment the corpus exists, while the definitions need a
    /// translation run (`data-prep/strongs-lang/translate.py`, an API key and
    /// an hour). Between those two moments the file is real and useful and its
    /// definitions are still Strong's own English — and a caveat claiming
    /// otherwise would be the app telling the reader something untrue about
    /// what it is showing them. `build-strongs.py` prints what to set this to.
    pub machine_translated: bool,
}

/// What a printed Bible in this language calls a verse the KJV addresses
/// differently. See [`crate::versification`] — it annotates, never renumbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumberingSpec {
    /// `osis \t chapter \t verse \t printedRef`, compiled in.
    pub table: &'static str,
    /// Whose numbering it is, so the annotation can say "Luther 3,19" rather
    /// than leaving the reader to guess which tradition disagreed.
    pub label: &'static str,
}

/// Everything that makes a language a language, in one row.
pub struct LangSpec {
    /// The BCP-47-ish code that crosses the wire and sits in the reader's config.
    pub code: &'static str,
    /// What this language calls itself — the only honest label for a picker,
    /// since a reader looking for German is looking for "Deutsch".
    pub endonym: &'static str,
    /// Its English name, for a reader who narrows the hymnal by typing
    /// "Spanish" rather than "Español". Both shells match either, plus the code.
    pub exonym: &'static str,
    /// The compiled-in catalogue (`i18n/<code>.json`). English is the source;
    /// every other language overrides it key by key.
    catalog: &'static str,
    /// The scripture this language reads, when it has one of its own.
    ///
    /// `None` is a real state rather than a placeholder: a translated interface
    /// is useful before a corpus is licensed, and such a reader reads English's
    /// text with everything else in their language.
    pub corpus: Option<CorpusSpec>,
    /// The Strong's dictionary under `data/`. English's is the source the others
    /// are translated from, and a language without its own reads it.
    pub lexicon: Option<LexiconSpec>,
    /// A MODERNIZATION of this language's standard translation — the file under
    /// `data/` holding a delta of re-worded token runs. English's is the AKJV,
    /// which updates the 1769's archaic wording ("thou shalt" → "you shall")
    /// without being a different translation or a simplified one.
    ///
    /// A COLUMN, not a special case, and that is the point of it being here. It
    /// used to be gated by comparing the open corpus's tokenization against the
    /// KJV's, which reads as "this feature is the norm and other languages are
    /// the exception". It is not: it is one English feature that happens to be
    /// the only one of its kind so far, exactly as Luther's verse numbering is
    /// one German one. A modernized Reina-Valera would fill this in and nothing
    /// else would change.
    pub modernization: Option<&'static str>,
    /// Printed numbering that disagrees with the KJV's addresses, when it does.
    pub numbering: Option<NumberingSpec>,
}

/// One row per [`Lang`], in variant order.
static SPECS: [LangSpec; Lang::COUNT] = [
    LangSpec {
        code: "en",
        endonym: "English",
        exonym: "English",
        catalog: include_str!("i18n/en.json"),
        corpus: Some(CorpusSpec { file: "kjv.jsonl", tokenization: crate::canon::TOKENIZATION_VERSION, label: "KJV" }),
        lexicon: Some(LexiconSpec { file: "strongs.json", machine_translated: false }),
        modernization: Some("akjv.jsonl"),
        // The KJV's numbering IS the addressing scheme; there is nothing for it
        // to disagree with.
        numbering: None,
    },
    LangSpec {
        code: "de",
        endonym: "Deutsch",
        exonym: "German",
        catalog: include_str!("i18n/de.json"),
        corpus: Some(CorpusSpec { file: "luther1912.jsonl", tokenization: "luther1912-tok1", label: "Luther" }),
        lexicon: Some(LexiconSpec { file: "strongs-de.json", machine_translated: true }),
        // No modernized Luther is shipped; the toggle hides itself because this
        // column is empty, not because anything anywhere checks for German.
        modernization: None,
        numbering: Some(NumberingSpec { table: include_str!("versification/luther-numbering.tsv"), label: "Luther" }),
    },
    LangSpec {
        code: "es",
        endonym: "Español",
        exonym: "Spanish",
        catalog: include_str!("i18n/es.json"),
        corpus: Some(CorpusSpec { file: "rv1909.jsonl", tokenization: "rv1909-tok1", label: "Reina-Valera" }),
        // The renderings are Reina-Valera's own words, derived from the tagged
        // corpus; the definitions are machine-translated (a Sonnet subagent
        // fleet, 2026-08-16 — data-prep/README.md), which is what this flag
        // discloses to the reader. See `LexiconSpec`.
        lexicon: Some(LexiconSpec { file: "strongs-es.json", machine_translated: true }),
        modernization: None,
        // Reina-Valera follows the KJV's chapter and verse breaks throughout —
        // `check-rv1909.py` proves all 66 books, every chapter count and every
        // last-verse number identical — so a Spanish reader's printed Bible
        // agrees with the address on screen and there is nothing to annotate.
        numbering: None,
    },
];

impl Lang {
    pub const COUNT: usize = 3;
    pub const ALL: [Lang; Lang::COUNT] = [Lang::En, Lang::De, Lang::Es];

    /// This language's row. The one accessor everything else is built on.
    pub fn spec(self) -> &'static LangSpec {
        &SPECS[self as usize]
    }

    /// The BCP-47-ish code that crosses the wire and sits in the reader's config.
    pub fn code(self) -> &'static str {
        self.spec().code
    }

    /// What this language calls itself.
    pub fn endonym(self) -> &'static str {
        self.spec().endonym
    }

    /// This language's English name.
    pub fn exonym(self) -> &'static str {
        self.spec().exonym
    }

    /// The text this language reads, which for a language with none of its own
    /// is English's. Every caller that opens or names a corpus goes through
    /// here, so "what does a Spanish reader read" has exactly one answer.
    pub fn corpus(self) -> &'static CorpusSpec {
        self.spec().corpus.as_ref().or(Lang::En.spec().corpus.as_ref()).expect("English has a corpus")
    }

    /// Whether this language reads a text of its own rather than English's —
    /// the question behind every "is this the KJV" gate in the panel.
    pub fn has_own_corpus(self) -> bool {
        self.spec().corpus.is_some() && self != Lang::En
    }

    /// The web manifest role this language's corpus cache is filed under.
    ///
    /// English's is the distinguished `corpusCache`, and stays so: it is the one
    /// file the stage-1 boot opens, it is never optional, and the loader must
    /// find it before it knows anything about the reader. Every other language's
    /// is keyed by its code, so adding a language adds a role instead of editing
    /// a list — which is what `germanCorpus` used to be.
    pub fn corpus_role(self) -> String {
        if self == Lang::En {
            "corpusCache".to_string()
        } else {
            format!("corpus:{}", self.code())
        }
    }

    /// The manifest role this language's own Strong's dictionary is filed under.
    /// Unused for English, whose dictionary rides in the base pack.
    pub fn lexicon_role(self) -> String {
        format!("lexicon:{}", self.code())
    }

    /// The language whose text carries this tokenization stamp.
    ///
    /// The question an ENGINE asks, and it is deliberately not "what is the
    /// reader's language": a German reader whose Luther download has not landed
    /// is reading the KJV, and the features that belong to that text — its
    /// modernization, the KJV-token-anchored analytics — are correct for them
    /// while they are on it. The text decides, not the interface.
    pub fn for_tokenization(tok: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.corpus().tokenization == tok)
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

fn raw(lang: Lang) -> &'static str {
    lang.spec().catalog
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
    ACTIVE.store(lang as u8, Ordering::Relaxed);
}

/// The whole registry as JSON, for the build scripts.
///
/// `scripts/build-web-pack.mjs` decides which files go in the web pack and what
/// role each carries, and it knew German by name: a `GERMAN_TEXT` constant, a
/// `germanCorpus` role, a `germanLexicon` role, three exclusions from its
/// generic walk. Node cannot read a Rust static, so either that table is
/// duplicated there — the very thing the registry is undoing — or it is asked
/// for. It is asked for: `plumbline-hydrate languages` prints this, and the pack
/// script already shells out to that binary for the idxcache, so there is no
/// generated file to drift.
pub fn registry_json() -> String {
    let langs: Vec<serde_json::Value> = Lang::ALL
        .iter()
        .map(|l| {
            let s = l.spec();
            serde_json::json!({
                "code": s.code,
                "endonym": s.endonym,
                "name": s.exonym,
                "corpus": s.corpus.as_ref().map(|c| c.file),
                "corpusCache": s.corpus.as_ref().map(|c| c.cache_file()),
                "tokenization": s.corpus.as_ref().map(|c| c.tokenization),
                "label": s.corpus.as_ref().map(|c| c.label),
                "lexicon": s.lexicon.map(|l| l.file),
                "machineTranslated": s.lexicon.map(|l| l.machine_translated),
                "modernization": s.modernization,
                "corpusRole": l.corpus_role(),
                "lexiconRole": l.lexicon_role(),
            })
        })
        .collect();
    serde_json::json!({ "languages": langs }).to_string()
}

/// One language's strings, id → text.
pub type Strings = BTreeMap<String, String>;

/// One language's catalogue, parsed ONCE for the life of the process.
///
/// THE PARSE IS NOT CHEAP AND IT IS NOT RARE. Every string in the app comes
/// through [`t`], and [`crate::reference::VRef::display`] is one `t` per
/// reference — which the wire layer calls FOR EVERY WORD in a laid-out chapter.
/// Parsing on each call meant one German chapter turn re-parsed the catalogues
/// thousands of times: Psalm 119 is 2,318 words × (en + de, merged, in `t`) plus
/// a third parse each in [`book_name`]. Measured on the web shell, serializing
/// one display list — 238 ms in German against 59 ms for the same chapter in
/// English, and effectively ALL of the difference was here. It read to a reader
/// as "German takes forever": the tap RPC queues behind the layout on the one
/// thread that answers both.
///
/// One cell per language, indexed by the variant — the same shape as [`SPECS`],
/// and filled lazily, so a reader pays for their own catalogue and not for the
/// ones they will never see.
///
/// Panics on malformed JSON, deliberately: the files are compiled into the
/// binary, so a bad one is a build-time mistake every test hits immediately, not
/// something a device can encounter.
fn table(lang: Lang) -> &'static Strings {
    static TABLES: [OnceLock<Strings>; Lang::COUNT] = [const { OnceLock::new() }; Lang::COUNT];
    TABLES[lang as usize].get_or_init(|| {
        serde_json::from_str(raw(lang)).unwrap_or_else(|e| panic!("i18n/{}.json is not valid: {e}", lang.code()))
    })
}

/// `lang`'s strings laid over English, memoized like [`table`] — the map [`t`]
/// reads on every lookup, so the merge cannot happen per call either.
fn merged(lang: Lang) -> &'static Strings {
    static MERGED: [OnceLock<Strings>; Lang::COUNT] = [const { OnceLock::new() }; Lang::COUNT];
    MERGED[lang as usize].get_or_init(|| {
        let mut out = table(Lang::En).clone();
        if lang != Lang::En {
            out.extend(table(lang).iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        out
    })
}

/// A copy of one language's catalogue. Prefer [`t`] for a single string; this is
/// for callers that hand the whole table somewhere else (the shells' catalogue
/// JSON) or walk it (the completeness tests).
pub fn catalog(lang: Lang) -> Strings {
    table(lang).clone()
}

/// The catalogue a shell should paint with: `lang`'s strings laid over English,
/// so every key is present even where the translation is not.
pub fn resolved(lang: Lang) -> Strings {
    merged(lang).clone()
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
    match merged(lang).get(id) {
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
        if let Some(name) = table(lang).get(&format!("book.{osis}")) {
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

    /// THE ROWS ARE INDEXED BY THE VARIANT, so a row inserted in the wrong place
    /// hands German the Spanish corpus and every other accessor follows it. That
    /// failure is silent — `Lang::De.code()` would simply answer `"es"` — so the
    /// anchor has to be spelled out rather than derived, which is the one place
    /// in this module where repeating yourself is the point.
    #[test]
    fn each_variant_reaches_its_own_row() {
        assert_eq!(Lang::En.code(), "en");
        assert_eq!(Lang::De.code(), "de");
        assert_eq!(Lang::Es.code(), "es");
        assert_eq!(Lang::De.corpus().file, "luther1912.jsonl");
        assert_eq!(Lang::Es.corpus().file, "rv1909.jsonl");
        assert_eq!(Lang::ALL.len(), SPECS.len());
    }

    /// A half-filled row is the failure this registry exists to prevent: adding
    /// Spanish used to mean finding twelve sites, and the point of one row is
    /// that forgetting a column is a test failure rather than a language that
    /// quietly reads the English Bible.
    #[test]
    fn a_row_is_complete() {
        for lang in Lang::ALL {
            let s = lang.spec();
            assert!(!s.code.is_empty(), "a language with no code");
            assert!(!s.endonym.is_empty(), "{} has no endonym", s.code);
            assert!(!s.exonym.is_empty(), "{} has no exonym", s.code);
            assert!(!s.catalog.trim().is_empty(), "{} has an empty catalogue", s.code);

            if let Some(c) = &s.corpus {
                assert!(!c.file.is_empty() && !c.tokenization.is_empty(), "{}'s corpus row is half-filled", s.code);
                // The label reaches a reader — it is what a rendering list is
                // headed with and what a differing verse number is credited to.
                assert!(!c.label.is_empty(), "{}'s corpus has no name a reader would know it by", s.code);
                assert_eq!(c.cache_file(), format!("{}.idxcache", c.file));
            }

            // A localized dictionary's `kjv_def` slot holds renderings derived
            // from that language's OWN tagged corpus. Ship one without the text
            // it was derived from and the card lists words that are not in the
            // Bible on screen.
            if s.lexicon.is_some() && lang != Lang::En {
                assert!(s.corpus.is_some(), "{} has a localized lexicon but reads English's text", s.code);
            }
            if let Some(n) = &s.numbering {
                assert!(!n.label.is_empty(), "{}'s numbering is credited to nobody", s.code);
                assert!(!n.table.trim().is_empty(), "{}'s numbering table is empty", s.code);
            }
        }
    }

    /// Two languages sharing a file is a corpus that overwrites another's, and a
    /// shared tokenization stamp lets a cache built for one text be accepted for
    /// the other — which is token indices silently pointing at the wrong words.
    #[test]
    fn no_two_languages_share_a_data_file() {
        let mut files: Vec<&str> = Vec::new();
        let mut toks: Vec<&str> = Vec::new();
        let mut lexicons: Vec<&str> = Vec::new();
        for lang in Lang::ALL {
            if let Some(c) = &lang.spec().corpus {
                assert!(!files.contains(&c.file), "{} reuses the corpus file {}", lang.code(), c.file);
                assert!(!toks.contains(&c.tokenization), "{} reuses the stamp {}", lang.code(), c.tokenization);
                files.push(c.file);
                toks.push(c.tokenization);
            }
            if let Some(l) = lang.spec().lexicon {
                assert!(!lexicons.contains(&l.file), "{} reuses the dictionary {}", lang.code(), l.file);
                lexicons.push(l.file);
            }
        }
    }

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

    /// EVERY CATALOGUE IS PARSED ONCE PER PROCESS, and this is a correctness
    /// test about cost rather than a stopwatch.
    ///
    /// `t()` is on the hottest path in the app — the wire layer turns every word
    /// of a laid-out chapter into a reference through it — and parsing per call
    /// made one German chapter turn cost 686 ms against 9 ms with the tables
    /// shared (Psalm 119, web shell). The reader felt it as a word tap
    /// that took half a second, because the tap queues behind the layout on the
    /// one thread that answers both.
    ///
    /// Asserted as POINTER IDENTITY, so it cannot pass by being fast on a fast
    /// machine: two lookups of the same language must be the same table.
    ///
    /// MUTATION: return `Box::leak(Box::new(…parse…))` from `table` instead of
    /// `cell.get_or_init(…)` — still `&'static Strings`, still correct copy, and
    /// this goes red where nothing else in the suite does.
    #[test]
    fn a_catalogue_is_parsed_once_and_shared() {
        for lang in Lang::ALL {
            assert!(std::ptr::eq(table(lang), table(lang)), "{} is re-parsed per lookup", lang.code());
            assert!(std::ptr::eq(merged(lang), merged(lang)), "{} is re-merged per lookup", lang.code());
        }
        // And the shared table is the same one `catalog()` hands out a copy of,
        // so the memoized path and the public one cannot drift.
        assert_eq!(catalog(Lang::De), *table(Lang::De));
        assert_eq!(resolved(Lang::De), *merged(Lang::De));
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
