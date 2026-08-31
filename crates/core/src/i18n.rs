//! Every word the reader sees, in one place, in every language.
//!
//! User-visible text is core data, and a shell that spells a sentence is a bug
//! — `crates/core/tests/no_stray_strings.rs` fails the build over it.
//!
//! One JSON catalogue per language, keyed by stable dotted ids and compiled in
//! with `include_str!`: a language is not an optional pack, and the app must
//! open in the reader's language offline on a first launch.
//!
//! `en` is the source. A key missing elsewhere falls back to English rather than
//! showing its id, and `missing()` lists the gaps so the fallback stays a safety
//! net rather than a hiding place.
//!
//! Not the scripture (that is a corpus, with its own tokenization per language),
//! and not `refKey` — `VRef::ref_key` is frozen storage and stays `"Gen 1:7"` in
//! every language, while the display form localizes ("Joh 3,16").

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// The languages the app ships.
///
/// Adding one is a variant here plus a row in [`SPECS`] beside it, and the data
/// files that row names — nothing else. The corpus the engine opens, the
/// Strong's dictionary it prefers, the printed-numbering annotation, the pack
/// files the shell downloads and the tokenization allow-list all read the row;
/// `a_row_is_complete` keeps a new one from being half-filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lang {
    En,
    De,
    Es,
    Ar,
    Pa,
    Hi,
    Fr,
    Zht,
    Zhs,
}

/// The writing system a language is set in — a column, and the source of both
/// direction ([`Script::is_rtl`]) and which faces can set the text
/// ([`crate::font::Font::offered_for`]). Those are separate questions: Gurmukhi
/// and Devanagari read left to right and still have no Latin face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Latin,
    Arabic,
    Gurmukhi,
    Devanagari,
    /// One script serving both Chinese rows: traditional and simplified are two
    /// repertoires of it, so one face covers both corpora and only the corpus
    /// and catalogue differ between the rows.
    Han,
}

impl Script {
    /// Whether this script is written right to left. Read by the layout engine
    /// (which mirrors its display list) and by the shell for its own chrome and
    /// swipe direction; each would otherwise be its own `if code == "ar"`.
    pub fn is_rtl(self) -> bool {
        matches!(self, Script::Arabic)
    }

    /// The token that crosses the wire, for a shell that names a script (the
    /// web's font CSS does).
    pub fn token(self) -> &'static str {
        match self {
            Script::Latin => "latin",
            Script::Arabic => "arabic",
            Script::Gurmukhi => "gurmukhi",
            Script::Devanagari => "devanagari",
            Script::Han => "han",
        }
    }
}

/// The scripture a language reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorpusSpec {
    /// The file under the home's `data/`, e.g. `"luther1912.jsonl"`.
    pub file: &'static str,
    /// The tokenization stamp its header must carry. Token indices are
    /// per-corpus — the same verse tokenizes differently in every translation —
    /// so each text has its own; [`crate::canon::tokenization_is_ours`] is this
    /// column.
    pub tokenization: &'static str,
    /// What a reader would call this Bible: the label on a rendering list, and
    /// the name beside a verse number that differs from the KJV's.
    pub label: &'static str,
}

impl CorpusSpec {
    /// The start-up cache beside it. Derived, never spelled twice: the two names
    /// drifting apart is a boot that silently re-parses ~19 MB of JSONL and
    /// looks only like "the app got slower".
    pub fn cache_file(&self) -> String {
        format!("{}.idxcache", self.file)
    }
}

/// A language's Strong's dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexiconSpec {
    /// The file under `data/`, e.g. `"strongs-de.json"`.
    pub file: &'static str,
    /// Whether its *definitions* are machine-translated, which the study card
    /// discloses on screen. A fact about the shipped file, because the two
    /// halves of a localized dictionary arrive separately: the renderings are
    /// derived from that language's tagged corpus, while the definitions need a
    /// translation run and are Strong's own English until it happens.
    /// `build-strongs.py` prints what to set this to.
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
    /// "Spanish" rather than "Español". A shell matches either, plus the code.
    pub exonym: &'static str,
    /// The writing system this language is set in — see [`Script`], which is
    /// also where the direction comes from.
    pub script: Script,
    /// The compiled-in catalogue (`i18n/<code>.json`). English is the source;
    /// every other language overrides it key by key.
    catalog: &'static str,
    /// The scripture this language reads, when it has one of its own. `None` is
    /// a real state: a translated interface is useful before a corpus is
    /// licensed, and such a reader reads English's text.
    pub corpus: Option<CorpusSpec>,
    /// The Strong's dictionary under `data/`. English's is the source the others
    /// are translated from, and a language without its own reads it.
    pub lexicon: Option<LexiconSpec>,
    /// A modernization of this language's standard translation — a file under
    /// `data/` holding a delta of re-worded token runs. English's is the AKJV,
    /// which updates the 1769's archaic wording without being a different or a
    /// simplified translation. A column, not an English special case: a
    /// modernized Reina-Valera would fill it in and nothing else would change.
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
        script: Script::Latin,
        catalog: include_str!("i18n/en.json"),
        corpus: Some(CorpusSpec { file: "kjv.jsonl", tokenization: crate::canon::TOKENIZATION_VERSION, label: "KJV" }),
        lexicon: Some(LexiconSpec { file: "strongs.json", machine_translated: false }),
        modernization: Some("akjv.jsonl"),
        // The KJV's numbering is the addressing scheme; nothing to disagree with.
        numbering: None,
    },
    LangSpec {
        code: "de",
        endonym: "Deutsch",
        exonym: "German",
        script: Script::Latin,
        catalog: include_str!("i18n/de.json"),
        corpus: Some(CorpusSpec { file: "luther1912.jsonl", tokenization: "luther1912-tok1", label: "Luther" }),
        lexicon: Some(LexiconSpec { file: "strongs-de.json", machine_translated: true }),
        // No modernized Luther ships; the toggle hides itself off this column.
        modernization: None,
        numbering: Some(NumberingSpec { table: include_str!("versification/luther-numbering.tsv"), label: "Luther" }),
    },
    LangSpec {
        code: "es",
        endonym: "Español",
        exonym: "Spanish",
        script: Script::Latin,
        catalog: include_str!("i18n/es.json"),
        corpus: Some(CorpusSpec { file: "rv1909.jsonl", tokenization: "rv1909-tok1", label: "Reina-Valera" }),
        // Renderings from the tagged corpus; definitions machine-translated,
        // which is what the flag discloses. See `LexiconSpec`.
        lexicon: Some(LexiconSpec { file: "strongs-es.json", machine_translated: true }),
        modernization: None,
        // Reina-Valera follows the KJV's chapter and verse breaks throughout
        // (`check-rv1909.py` proves it), so there is nothing to annotate.
        numbering: None,
    },
    LangSpec {
        code: "ar",
        endonym: "العربية",
        exonym: "Arabic",
        script: Script::Arabic,
        catalog: include_str!("i18n/ar.json"),
        corpus: Some(CorpusSpec { file: "svd1865.jsonl", tokenization: "svd1865-tok1", label: "Van Dyck" }),
        // No Strong's dictionary: `build-strongs.py` derives its renderings from
        // a TAGGED corpus and `svd1865.jsonl` carries no codes. The LLM-generated
        // Van Dyck alignments that exist were deliberately not shipped — every
        // Arabic token's code list is empty, so the word study is honestly absent
        // rather than machine-guessed.
        lexicon: None,
        modernization: None,
        // No numbering table, and not for Spanish's reason. The Van Dyck prints
        // 31,104 verses (1 Tim 6:21 and 3 John 14 each split in two) and
        // `build-svd.py` merges both back — but this column annotates a DIFFERENT
        // NUMBER, and both of those open at the same number the KJV does. What is
        // lost is the other direction (someone handed "3 John 15" finds 14
        // verses), which is a reference-parsing question, not a display one.
        numbering: None,
    },
    LangSpec {
        code: "pa",
        endonym: "ਪੰਜਾਬੀ",
        exonym: "Punjabi",
        script: Script::Gurmukhi,
        catalog: include_str!("i18n/pa.json"),
        corpus: Some(CorpusSpec {
            file: "pan-fbi.jsonl", tokenization: "pan-fbi-tok1", label: "ਪਵਿੱਤਰ ਬਾਈਬਲ"
        }),
        // No Strong's dictionary, for Arabic's reason: `pan-fbi.jsonl` carries
        // no codes.
        lexicon: None,
        modernization: None,
        // No numbering table, and — like Arabic — not for Spanish's reason. The
        // two splits `build-indic.py` merges back (3 John 14, Rev 13:1) print at
        // the same number the KJV uses. What IS different is 1 John 5:6-8 — the
        // KJV's 5:6b sits at 5:7, the Comma Johanneum is absent, 5:8 realigns —
        // and `NumberingSpec` is the wrong shape for it: nothing is renumbered,
        // the text under one number differs.
        numbering: None,
    },
    LangSpec {
        code: "hi",
        endonym: "हिन्दी",
        exonym: "Hindi",
        script: Script::Devanagari,
        catalog: include_str!("i18n/hi.json"),
        corpus: Some(CorpusSpec {
            file: "hin-fbi.jsonl", tokenization: "hin-fbi-tok1", label: "पवित्र बाइबल"
        }),
        // See Punjabi: no tagged corpus, so no dictionary and none invented.
        lexicon: None,
        modernization: None,
        // Same two splits, same 1 John 5 divergence — see Punjabi.
        numbering: None,
    },
    LangSpec {
        code: "fr",
        endonym: "Français",
        exonym: "French",
        script: Script::Latin,
        catalog: include_str!("i18n/fr.json"),
        corpus: Some(CorpusSpec { file: "ost1996.jsonl", tokenization: "ost1996-tok1", label: "Ostervald" }),
        // No Strong's dictionary, for Arabic's reason: the source carries no codes.
        lexicon: None,
        modernization: None,
        // The source prints French/Hebrew-style numbering — 91 chapters break
        // differently (psalm titles numbered as verse 1, Job 38-41 recut, a dozen
        // chapter boundaries elsewhere) — and `build-ostervald.py` moves the text
        // onto KJV addresses. This table annotates the 1,263 addresses whose
        // printed French number differs.
        numbering: Some(NumberingSpec {
            table: include_str!("versification/ostervald-numbering.tsv"),
            label: "Ostervald",
        }),
    },
    // Chinese is two rows, one script: traditional and simplified are two
    // repertoires of one language reading one translation (the 1919 和合本,
    // built twice by `build-cuv.py`, proven parallel by `check-cuv.py`). Two
    // rows because catalogue and corpus both differ; one `Script::Han` because
    // one face sets both. The codes are `zht`/`zhs`, not BCP-47's
    // `zh-Hant`/`zh-Hans`: `shipped()` strips subtags after the first `-`, and
    // the manifest role grammar (`corpus:<code>`) takes two or three lowercase
    // letters. `shipped()` routes the real browser tags onto the right row.
    //
    // Both corpora tokenize PER CHARACTER (see `build-cuv.py`'s header): the
    // phrase tier becomes exact substring search, and break opportunities become
    // token boundaries, so kinsoku rides in pre/post and `crates/layout` needs no
    // intra-token breaking. The printed CUV's 71 ranged-verse addresses ship as
    // 併於上節/并于上节, with the numbering table saying what the page calls them.
    LangSpec {
        code: "zht",
        endonym: "中文（繁體）",
        exonym: "Traditional Chinese",
        script: Script::Han,
        catalog: include_str!("i18n/zht.json"),
        corpus: Some(CorpusSpec { file: "cuv1919t.jsonl", tokenization: "cuv1919t-tok1", label: "和合本" }),
        // No tagged corpus, so no dictionary — see Arabic.
        lexicon: None,
        modernization: None,
        numbering: Some(NumberingSpec { table: include_str!("versification/cuv-numbering.tsv"), label: "和合本" }),
    },
    LangSpec {
        code: "zhs",
        endonym: "中文（简体）",
        exonym: "Simplified Chinese",
        script: Script::Han,
        catalog: include_str!("i18n/zhs.json"),
        corpus: Some(CorpusSpec { file: "cuv1919s.jsonl", tokenization: "cuv1919s-tok1", label: "和合本" }),
        lexicon: None,
        modernization: None,
        // The same table as traditional: one set of printed numbers, asserted
        // identical across the editions by `check-cuv.py`.
        numbering: Some(NumberingSpec { table: include_str!("versification/cuv-numbering.tsv"), label: "和合本" }),
    },
];

impl Lang {
    pub const COUNT: usize = 9;
    pub const ALL: [Lang; Lang::COUNT] =
        [Lang::En, Lang::De, Lang::Es, Lang::Ar, Lang::Pa, Lang::Hi, Lang::Fr, Lang::Zht, Lang::Zhs];

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

    /// The writing system this language is set in. See [`Script`].
    pub fn script(self) -> Script {
        self.spec().script
    }

    /// Whether this language is written right to left. Derived from the script.
    pub fn is_rtl(self) -> bool {
        self.spec().script.is_rtl()
    }

    /// The text this language reads — English's, for a language with none of its
    /// own. Every caller that opens or names a corpus goes through here.
    pub fn corpus(self) -> &'static CorpusSpec {
        self.spec().corpus.as_ref().or(Lang::En.spec().corpus.as_ref()).expect("English has a corpus")
    }

    /// Whether this language reads a text of its own rather than English's —
    /// the question behind every "is this the KJV" gate in the panel.
    pub fn has_own_corpus(self) -> bool {
        self.spec().corpus.is_some() && self != Lang::En
    }

    /// Whether this language's own catalogue carries the first-run prose rather
    /// than falling back to English. Derived, never declared: writing the prose
    /// is what turns the feature on, so the flag and the words cannot disagree.
    /// Measured against English's key set, since English defines "all of it".
    pub fn has_native_intros(self) -> bool {
        if self == Lang::En {
            return true;
        }
        let mine = table(self);
        let mut wanted = table(Lang::En).keys().filter(|k| is_intro_prose(k)).peekable();
        wanted.peek().is_some() && wanted.all(|k| mine.contains_key(k))
    }

    /// The web manifest role this language's corpus cache is filed under.
    /// English's is the distinguished `corpusCache`: the one file the stage-1
    /// boot opens, never optional, found before the loader knows anything about
    /// the reader. Every other language's is keyed by its code, so adding a
    /// language adds a role instead of editing a list.
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

    /// The language whose text carries this tokenization stamp. Deliberately not
    /// "the reader's language": a German reader whose Luther download has not
    /// landed is reading the KJV, and that text's features (its modernization,
    /// the KJV-token-anchored analytics) are the correct ones while they are.
    pub fn for_tokenization(tok: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.corpus().tokenization == tok)
    }

    /// The language this code names, if the app ships it — tolerating a region
    /// tag, since a browser reporting `de-CH` wants German. `None` otherwise, so
    /// [`resolve`] can tell "asked for a language we do not have" apart from
    /// "asked for English".
    pub fn shipped(code: &str) -> Option<Lang> {
        let lower = code.to_ascii_lowercase();
        let mut tags = lower.split(['-', '_']);
        let base = tags.next().unwrap_or("");
        // Chinese is the one language whose subtags choose BETWEEN shipped rows
        // rather than narrowing one: a browser says `zh-TW` or `zh-Hans-CN`,
        // never `zht`. Script tag first, else the traditional-script regions,
        // else simplified — which is also what a bare `zh` usually means.
        if base == "zh" {
            let rest: Vec<&str> = tags.collect();
            let traditional = if rest.contains(&"hant") {
                true
            } else if rest.contains(&"hans") {
                false
            } else {
                rest.iter().any(|t| matches!(*t, "tw" | "hk" | "mo"))
            };
            return Some(if traditional { Lang::Zht } else { Lang::Zhs });
        }
        Lang::ALL.into_iter().find(|l| l.code() == base)
    }

    /// Parse a code, tolerating a region tag. Unknown languages are English,
    /// never an error — a reader with an unsupported locale gets a working app.
    pub fn parse(code: &str) -> Lang {
        Lang::shipped(code).unwrap_or(Lang::En)
    }
}

/// The language to paint in, given the reader's setting and the device's locale.
/// An empty setting means "follow the device" (see [`crate::config::Config`]),
/// so a German phone opens in German without anybody visiting Settings, and a
/// reader who chose English is not overruled by their hardware. The shell calls
/// this rather than deciding for itself.
pub fn resolve(chosen: &str, device: &str) -> Lang {
    Lang::shipped(chosen).or_else(|| Lang::shipped(device)).unwrap_or(Lang::En)
}

fn raw(lang: Lang) -> &'static str {
    lang.spec().catalog
}

// ── the active language ──────────────────────────────────────────────────────
//
// A deliberate process global: it is a property of the one reader this process
// serves, and the alternative is a `lang` parameter on the thirty wire-layer
// sites that turn a `VRef` into something a reader reads — each one a place to
// forget, and forgetting reads as an app that says "Genesis" in German.
//
// An atomic rather than a lock: the ABI may be called from more than one thread
// and this is read on nearly every call. A torn read is impossible for a u8, and
// a stale one is a single frame during a switch that reloads anyway.

static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// The language `VRef::display` and book names come out in. English until a
/// shell says otherwise, so tests and tools that never set it are unaffected.
pub fn active() -> Lang {
    Lang::ALL.get(ACTIVE.load(Ordering::Relaxed) as usize).copied().unwrap_or(Lang::En)
}

/// The language stamp written into a file the reader authors — provenance, not
/// display.
///
/// `refKey` is frozen storage meaning a verse in KJV/OSIS numbering, and other
/// corpora number a hundred-odd verses differently. A future versification
/// migration would have to move exactly the keys written under another
/// numbering, and that is unrunnable unless the data says which numbering each
/// key meant. An absent stamp is NOT `"en"` — it means "a build that did not
/// record this", which a migration is entitled to treat differently.
///
/// Stamped at CREATE, never on re-save: writing the current language onto an
/// unstamped old note would turn an honest absence into a confident wrong
/// answer. [`stamped_extra`] is therefore called from the constructors, and
/// preserve-on-re-save falls out of the round-trip extras map each format
/// already carries. It lives in that map rather than as a modelled field because
/// nothing branches on it yet; the migration that reads it can promote it.
pub fn stamp() -> String {
    active().code().to_string()
}

/// The extra-keys map a newly created file starts with: just its language stamp.
/// Called from the constructors, never the writers — a writer also runs on
/// re-save. See [`stamp`].
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
/// role each carries. Node cannot read a Rust static, so it asks instead of
/// duplicating the table: `plumbline-hydrate languages` prints this, and the
/// pack script already shells out to that binary for the idxcache — so there is
/// no generated file to drift.
pub fn registry_json() -> String {
    let langs: Vec<serde_json::Value> = Lang::ALL
        .iter()
        .map(|l| {
            let s = l.spec();
            serde_json::json!({
                "code": s.code,
                "endonym": s.endonym,
                "name": s.exonym,
                // For the shell's own chrome — the document's `dir`, and which
                // way a swipe turns the page. A separate question from direction
                // inside the text, which the engine handles by mirroring the
                // display list. Sent rather than derived from `script` so the
                // Arabic-is-RTL rule lives in one place.
                "rtl": s.script.is_rtl(),
                // Which faces can render this language; the web's font picker
                // reads it. See `Font::offered_for`.
                "script": s.script.token(),
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

/// One language's catalogue, parsed once for the life of the process.
///
/// The parse is neither cheap nor rare: every string comes through [`t`], and
/// the wire layer calls [`crate::reference::VRef::display`] — one `t` — for
/// every word in a laid-out chapter. Parsing per call cost 238 ms to serialize
/// one German display list against 59 ms for the same chapter in English, and
/// the tap RPC queues behind that on the one thread answering both.
///
/// One lazily filled cell per language, indexed by the variant like [`SPECS`].
/// Panics on malformed JSON deliberately: the files are compiled in, so a bad
/// one is a build-time mistake, not something a device can meet.
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

/// The first-run prose: the two welcomes that address the reader's own life
/// rather than the app.
///
/// The only strings exempt from translation, because they are somebody speaking
/// out of a shared world rather than labels a translator can render. They wait
/// for someone inside the culture to write them, and until then the paths that
/// lead to them are not offered at all.
///
/// One list, read by [`Lang::has_native_intros`], the completeness test's
/// exemption, and the all-or-nothing test beside it.
pub const INTRO_PROSE: [&str; 2] = ["intro.welcome.", "intro.curious."];

/// Whether `id` is one of the first-run prose strings. See [`INTRO_PROSE`].
pub fn is_intro_prose(id: &str) -> bool {
    INTRO_PROSE.iter().any(|p| id.starts_with(p))
}

/// Keys English has that `lang` does not. Empty for English by definition.
pub fn missing(lang: Lang) -> Vec<String> {
    let en = catalog(Lang::En);
    let mine = catalog(lang);
    en.keys().filter(|k| !mine.contains_key(*k)).cloned().collect()
}

/// Fill `{placeholders}` from `args`. A placeholder with no argument is left as
/// it is rather than blanked: a silently missing word reads like finished copy,
/// while the braces on screen are unmistakably a bug and name the argument.
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
/// English is deliberately not in the catalogue — [`crate::canon::BOOKS`] holds
/// it, frozen, and parsing reads it too. So a catalogue entry overrides and its
/// absence means English, which is why `missing()` cannot see an untranslated
/// book; `every_book_is_named_in_every_language` checks them directly.
pub fn book_name(lang: Lang, osis: &str) -> String {
    if lang != Lang::En {
        if let Some(name) = table(lang).get(&format!("book.{osis}")) {
            return name.clone();
        }
    }
    crate::canon::display_name(osis).to_string()
}

/// Pick between a one-form and a many-form key. Deliberately not a plural
/// engine: one/other covers every language this app ships, and a language with
/// more forms (Polish, Russian, Arabic) needs CLDR rules and this function
/// replaced, not extended.
pub fn plural(lang: Lang, id_one: &str, id_other: &str, n: u64, args: &[(&str, &str)]) -> String {
    let count = n.to_string();
    let mut all: Vec<(&str, &str)> = vec![("n", &count)];
    all.extend_from_slice(args);
    t(lang, if n == 1 { id_one } else { id_other }, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows are indexed by the variant, so a row inserted in the wrong place
    /// hands German the Spanish corpus and every accessor follows it silently.
    /// Spelled out rather than derived: repeating [`SPECS`] is the point here.
    #[test]
    fn each_variant_reaches_its_own_row() {
        assert_eq!(Lang::En.code(), "en");
        assert_eq!(Lang::De.code(), "de");
        assert_eq!(Lang::Es.code(), "es");
        assert_eq!(Lang::Fr.code(), "fr");
        assert_eq!(Lang::Zht.code(), "zht");
        assert_eq!(Lang::Zhs.code(), "zhs");
        assert_eq!(Lang::De.corpus().file, "luther1912.jsonl");
        assert_eq!(Lang::Es.corpus().file, "rv1909.jsonl");
        assert_eq!(Lang::Fr.corpus().file, "ost1996.jsonl");
        assert_eq!(Lang::Zht.corpus().file, "cuv1919t.jsonl");
        assert_eq!(Lang::Zhs.corpus().file, "cuv1919s.jsonl");
        assert_eq!(Lang::ALL.len(), SPECS.len());
    }

    /// Chinese locale tags choose between two shipped rows, so the routing is
    /// pinned: script subtag, then the traditional-script regions, then the
    /// mainland default — a bare `zh` is simplified.
    #[test]
    fn chinese_locales_land_on_the_right_row() {
        for tag in ["zh-TW", "zh-HK", "zh-MO", "zh-Hant", "zh-Hant-TW", "zht"] {
            assert_eq!(Lang::shipped(tag), Some(Lang::Zht), "{tag}");
        }
        for tag in ["zh", "zh-CN", "zh-SG", "zh-Hans", "zh-Hans-CN", "zhs"] {
            assert_eq!(Lang::shipped(tag), Some(Lang::Zhs), "{tag}");
        }
        // The script subtag outranks a region that disagrees with it.
        assert_eq!(Lang::shipped("zh-Hans-HK"), Some(Lang::Zhs));
        // And the special case does not leak: French still resolves by base.
        assert_eq!(Lang::shipped("fr-CA"), Some(Lang::Fr));
    }

    /// A half-filled row is the failure this registry exists to prevent: a
    /// forgotten column should be a test failure, not a language that quietly
    /// reads the English Bible.
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
                // The label reaches a reader: a rendering list's heading, and
                // what a differing verse number is credited to.
                assert!(!c.label.is_empty(), "{}'s corpus has no name a reader would know it by", s.code);
                assert_eq!(c.cache_file(), format!("{}.idxcache", c.file));
            }

            // A localized dictionary's renderings come from that language's own
            // tagged corpus; ship it without that text and the card lists words
            // that are not in the Bible on screen.
            if s.lexicon.is_some() && lang != Lang::En {
                assert!(s.corpus.is_some(), "{} has a localized lexicon but reads English's text", s.code);
            }
            if let Some(n) = &s.numbering {
                assert!(!n.label.is_empty(), "{}'s numbering is credited to nobody", s.code);
                assert!(!n.table.trim().is_empty(), "{}'s numbering table is empty", s.code);
            }
        }
    }

    /// A shared file is a corpus overwriting another's; a shared tokenization
    /// stamp lets a cache built for one text be accepted for the other, which is
    /// token indices silently pointing at the wrong words.
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
        assert_eq!(Lang::parse("fr-CA"), Lang::Fr);
        // An unsupported language is English, not an error and not empty.
        assert_eq!(Lang::parse("it"), Lang::En);
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
        assert_eq!(resolve("", "fr-FR"), Lang::Fr);
        assert_eq!(resolve("", "zh-TW"), Lang::Zht);
        assert_eq!(resolve("", "zh-CN"), Lang::Zhs);
        // A language this build does not ship, in either slot, is not an error.
        assert_eq!(resolve("it", "de-DE"), Lang::De);
        assert_eq!(resolve("", "it-IT"), Lang::En);
        assert_eq!(resolve("", ""), Lang::En);
    }

    #[test]
    fn resolved_falls_back_to_english_key_by_key() {
        // A resolved catalogue answers every English key, whatever the
        // translation lacks — the property the shell depends on.
        let en = catalog(Lang::En);
        let de = resolved(Lang::De);
        for k in en.keys() {
            assert!(de.contains_key(k), "resolved de lost the key {k}");
        }
    }

    /// Every catalogue is parsed once per process. `t()` is on the hottest path
    /// — the wire layer turns every word of a chapter into a reference through
    /// it — and parsing per call cost 686 ms for one German chapter turn against
    /// 9 ms shared. Asserted as pointer identity rather than a stopwatch, so it
    /// cannot pass by running on a fast machine; a `table` that returned a fresh
    /// `Box::leak` per call would still typecheck and would fail only here.
    #[test]
    fn a_catalogue_is_parsed_once_and_shared() {
        for lang in Lang::ALL {
            assert!(std::ptr::eq(table(lang), table(lang)), "{} is re-parsed per lookup", lang.code());
            assert!(std::ptr::eq(merged(lang), merged(lang)), "{} is re-merged per lookup", lang.code());
        }
        // And `catalog()` copies that same table, so the memoized and public
        // paths cannot drift.
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
        // canon.rs, so there is no en.json key to be missing against.
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

    /// [`INTRO_PROSE`] is the one permitted exemption, and adding to it is a
    /// decision rather than a convenience. Nothing else may fall back.

    #[test]
    fn every_shipped_string_is_translated() {
        for lang in Lang::ALL {
            if lang == Lang::En {
                continue;
            }
            let gaps: Vec<String> = missing(lang).into_iter().filter(|k| !is_intro_prose(k)).collect();
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
    fn english_carries_the_prose_the_gate_is_about() {
        // Without this the gate guards nothing: with no prose in en.json,
        // `has_native_intros` has no keys to require, so every language passes
        // and every reader is offered a path to a blank welcome.
        let en = catalog(Lang::En);
        for prefix in INTRO_PROSE {
            assert!(en.keys().any(|k| k.starts_with(prefix)), "no {prefix}* strings: the gate guards nothing");
        }
    }

    #[test]
    fn the_first_run_prose_is_all_or_nothing_in_every_language() {
        // A half-written welcome renders two paragraphs in the reader's language
        // and then three in English, mid-thought.
        let en = catalog(Lang::En);
        let all: Vec<&String> = en.keys().filter(|k| is_intro_prose(k)).collect();
        for lang in Lang::ALL {
            let mine = catalog(lang);
            let have = all.iter().filter(|k| mine.contains_key(**k)).count();
            assert!(
                have == 0 || have == all.len(),
                "{} has {have} of {} first-run prose strings — write the rest or none",
                lang.code(),
                all.len()
            );
        }
    }

    #[test]
    fn the_gate_and_the_words_can_never_disagree() {
        // The contract the shell acts on: a language is offered these paths
        // exactly when a reader taking one meets no English paragraph. An
        // equivalence rather than a snapshot, so writing a language's prose turns
        // its paths on without a test to update.
        for lang in Lang::ALL {
            let own = catalog(lang);
            let fell_back =
                catalog(Lang::En).keys().filter(|k| is_intro_prose(k)).filter(|k| !own.contains_key(*k)).count();
            assert_eq!(
                lang.has_native_intros(),
                fell_back == 0,
                "{} is offered the first-run prose with {fell_back} of its paragraphs falling back to English",
                lang.code()
            );
        }
    }

    #[test]
    fn a_translation_never_invents_a_key_english_does_not_have() {
        // The direction that rots quietly: a key renamed in en.json leaves its
        // translation behind as dead weight that still looks translated, and
        // `missing()` cannot see it.
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
        // A dropped `{n}` is a sentence that silently loses its number and still
        // reads as finished copy. An added one is worse: no caller supplies it,
        // so the braces reach the screen (see `format`).
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
        // The stamp follows the reader's active language, not a constant; the
        // per-format tests prove the key lands in the file and that a re-save
        // adds none. Restores the previous language — this is process-wide state
        // and the tests in this binary share a process.
        let before = active();
        set_active(Lang::De);
        assert_eq!(stamp(), "de");
        assert_eq!(stamped_extra()["lang"], "de");
        set_active(Lang::En);
        assert_eq!(stamped_extra()["lang"], "en");
        set_active(before);
    }

    /// Nothing outside this module may name a book from `canon`.
    ///
    /// `canon::display_name` is the frozen English table `refKey` is built from:
    /// right for storage, never right for a reader, and the two look identical
    /// in an English test run. A source scan rather than a behavioural test,
    /// because the failure is a call that should not exist rather than a wrong
    /// output. The only legitimate caller is `book_name`, its fallback.
    #[test]
    fn only_book_name_reads_the_frozen_english_book_table() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut offenders: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        let mut stack = vec![root.join("crates")];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                // canon.rs defines it; this file is its one legitimate caller.
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                if name == "canon.rs" || name == "i18n.rs" {
                    continue;
                }
                let Ok(src) = std::fs::read_to_string(&p) else { continue };
                scanned += 1;
                for (n, line) in src.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    if code.contains("display_name(") {
                        offenders.push(format!("{}:{}", p.strip_prefix(&root).unwrap_or(&p).display(), n + 1));
                    }
                }
            }
        }
        // Not vacuous: a walk that found nothing (a moved crate root, a sandbox
        // with no source) would pass silently forever.
        assert!(scanned > 20, "the scan only reached {scanned} files; it is not looking at the crates");
        assert!(
            offenders.is_empty(),
            "these name a book from the frozen English table instead of `i18n::book_name`: {offenders:?}"
        );
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
        // The point is the selection, and that `n` reaches both forms without
        // the caller passing it.
        assert_eq!(plural(Lang::En, "present.passages.one", "present.passages.other", 1, &[]), "1 passage");
        assert_eq!(plural(Lang::En, "present.passages.one", "present.passages.other", 6, &[]), "6 passages");
        assert_eq!(plural(Lang::En, "present.passages.one", "present.passages.other", 0, &[]), "0 passages");
    }
}
