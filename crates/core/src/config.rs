//! Application config: study mode, reader typography and session state,
//! persisted as JSON in the platform's per-user config directory.
//!
//! Paths are composed with [`Path::join`]; writes go through the atomic writer
//! in [`crate::store`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::font::Font;
use crate::theme::ThemeChoice;
use crate::Error;

/// How much of the app the reader sees. `Full` unlocks the study surface
/// (threads, tags, weave authoring/review); `Simple` is lookup and search only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StudyMode {
    #[default]
    Simple,
    Full,
}

impl StudyMode {
    /// Frozen on-disk token.
    pub fn token(self) -> &'static str {
        match self {
            StudyMode::Simple => "simple",
            StudyMode::Full => "full",
        }
    }

    pub fn parse(t: &str) -> Option<StudyMode> {
        match t {
            "simple" => Some(StudyMode::Simple),
            "full" => Some(StudyMode::Full),
            _ => None,
        }
    }

    /// Whether the study/authoring surface is available in this mode.
    pub fn is_full(self) -> bool {
        matches!(self, StudyMode::Full)
    }
}

/// One reopened reading pane. `verse` is the first visible verse so a session
/// reopens mid-chapter; `None` = top of the chapter, and history entries never
/// carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRef {
    pub book: String,
    pub chapter: u16,
    pub verse: Option<u16>,
    /// The text this pane reads, as a language code; empty = the reader's own.
    /// Per-pane, not per-app: German beside English is the point.
    pub lang: String,
}

/// The reader's home church, carried in a shared link. Every part is optional;
/// an empty name means "not set".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Church {
    pub name: String,
    /// When they meet, in minutes since local midnight — the grain of
    /// [`Config::sunday_service`], which the share builder fills it from. Its own
    /// field because a church from someone else's link is not this reader's config.
    pub service: Option<u16>,
    pub url: String,
}

impl Church {
    /// Nothing to show or share.
    pub fn is_empty(&self) -> bool {
        self.name.trim().is_empty()
    }
}

/// The persisted settings. New fields must be additive (default on absence) so
/// an older file keeps loading.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub mode: StudyMode,
    pub body_size: f64,
    /// The reading panes from the last session; empty → the default passage.
    pub panes: Vec<PaneRef>,
    /// Which pane was active last session.
    pub active: usize,
    /// Where the reader was, per seating ([`crate::session_slot`]), keyed by slot
    /// token. [`Config::panes`] is the fallback for a slot never used.
    pub slots: BTreeMap<String, PaneRef>,
    /// Lifetime reads of the whole Bible; `-1` = never said (0 is a legitimate
    /// answer). Seeded once by hand; after that only finishing the canon moves it.
    pub bible_reads: i64,
    /// Whether the current full-canon state has been counted, so finishing
    /// credits exactly one read. Cleared when the map drops below complete.
    pub bible_reads_credited: bool,
    /// Verse-per-line reading mode.
    pub verse_per_line: bool,
    /// Page-turn mode: a tap margin either side of the text scrolls most of a
    /// screen, so a page-turner remote can drive the page.
    pub page_turn: bool,
    /// Paint the small leading verse numbers. A layout input, not a paint flag:
    /// see `plumbline_layout::LayoutConfig`.
    pub verse_numbers: bool,
    /// Italicize the words the KJV translators supplied (`FLAG_ADDED`). When off
    /// they stay distinguishable by the palette's `added` tone, the same fallback
    /// a face with no italic uses ([`crate::font::Font::has_italic`]).
    pub added_italics: bool,
    /// The reader's colour theme. `System` follows the OS.
    pub theme: ThemeChoice,
    /// The face scripture is painted in; independent of the other two axes.
    pub text_font: Font,
    /// The face the app's own chrome is painted in. See [`crate::font`].
    pub chrome_font: Font,
    /// Default one-tap copy shape, a verse `CopyKind` token: see [`COPY_STYLES`].
    pub copy_style: String,
    /// Reader horizontal margin in px (default 28).
    pub side_margin: f64,
    /// Reader line-height as a multiple of the text height (default 1.35).
    pub line_spacing: f64,
    /// Reading history, most-recent-first, deduped by (book, chapter) and capped
    /// at [`HISTORY_CAP`].
    pub history: Vec<PaneRef>,
    /// Show the curated-scholarship analysis tiers (renderings, morphology,
    /// same-root, TSK). The text and the reader's own data are never gated.
    pub human_analysis: bool,
    /// Show the learned/statistical tiers (concept engine, SIF, leitwort).
    pub machine_analysis: bool,
    /// The reader's home church, attached to the links this reader shares.
    pub church: Church,
    /// Sunday service start, minutes since local midnight. Set, the
    /// `sunday-morning` seating is this time until 1.5 hours after
    /// ([`crate::session_slot::slot_for_at`]); unset keeps the before-noon rule.
    pub sunday_service: Option<u32>,
    /// Whether a link shared from Present opens for a new believer. On by default.
    pub present_shares_as_new: bool,
    /// The plain-English overlay (the AKJV delta) on the reader. Reader-only:
    /// memorize, Present, copy and share stay KJV (see [`crate::akjv`]).
    pub akjv_overlay: bool,
    /// Which welcome this reader was given ("new" | "curious"), empty when none.
    pub intro: String,
    /// Whether the bundled devotional has been offered, so the new-believer
    /// welcome starts it exactly once.
    ///
    /// The start can legitimately fail the first time (the welcome can finish
    /// before the pack's text stage lands, and `devotional_start` refuses a
    /// booklet the catalogue lacks), so the shell retries on a later boot. This
    /// separates "never managed to start it" from "the reader stopped it";
    /// without it a booklet the reader threw away comes back every launch.
    pub devotional_seeded: bool,
    /// The reader's language ([`crate::i18n::Lang::code`]). Empty = follow the
    /// device, not English: storing "en" on the first resolve would freeze a
    /// German reader into English. Only an explicit choice is written.
    pub language: String,
    /// The id of the running [`crate::plan::Kind::ConceptStudy`] plan, empty in
    /// normal reading mode. Here so it survives a relaunch and every pane agrees
    /// what a tap means; the shell suspends its reading tracker while it is set.
    pub concept_study: String,
    /// Which thread "share the gospel" opens. Empty = the default (the stock
    /// Romans Road), not "none": storing the choice would freeze the name of a
    /// thread the reader can rename or delete. An unmatched name falls back too.
    pub gospel_thread: String,
    /// Opt out of this language's own Strong's dictionary (`strongs-de.json`,
    /// `strongs-es.json`) in favour of the English definitions. Applied when the
    /// engine opens, like the language.
    pub localized_lexicon_off: bool,
}

/// A verse copy-shape token accepted for [`Config::copy_style`].
pub const COPY_STYLES: [&str; 3] = ["verse", "verseRef", "verseMarkdown"];

/// The most reading-history entries kept (persisted + returned).
pub const HISTORY_CAP: usize = 50;

impl Default for Config {
    fn default() -> Config {
        Config {
            mode: StudyMode::Simple,
            body_size: 20.0,
            panes: Vec::new(),
            active: 0,
            slots: BTreeMap::new(),
            bible_reads: -1,
            bible_reads_credited: false,
            verse_per_line: false,
            page_turn: false,
            verse_numbers: true,
            added_italics: true,
            theme: ThemeChoice::default(),
            text_font: Font::default(),
            chrome_font: Font::default(),
            copy_style: "verseRef".to_string(),
            side_margin: 28.0,
            line_spacing: 1.35,
            history: Vec::new(),
            // Opt-in: see the note on `from_wire` below.
            human_analysis: false,
            machine_analysis: false,
            church: Church::default(),
            sunday_service: None,
            present_shares_as_new: true,
            akjv_overlay: false,
            localized_lexicon_off: false,
            intro: String::new(),
            devotional_seeded: false,
            language: String::new(),
            concept_study: String::new(),
            gospel_thread: String::new(),
        }
    }
}

// On-disk form (camelCase, mode as a token). Missing fields fall back to the
// default so the file evolves additively (CLAUDE.md §Data formats).
//
// Additive cuts both ways: a key this build has never heard of must survive its
// saves too, or an older build strips it for good. Every struct here therefore
// ends in a flattened catch-all that `save_to` fills from the file it replaces.
// It cannot live on `Config`: `crates/ffi` rebuilds that value field by field
// from the shell's wire payload on every save (`wire::config_from_wire`), so an
// in-memory field would be dropped there instead. Serde matches declared fields
// first, so a key later promoted to a real field stops arriving in the catch-all
// and can never be written twice.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigWire {
    #[serde(default = "default_mode_token")]
    study_mode: String,
    #[serde(default = "default_body_size")]
    body_size: f64,
    #[serde(default)]
    open_panes: Vec<PaneWire>,
    #[serde(default)]
    active_pane: usize,
    /// A BTreeMap so the file is written in a stable order — a format that
    /// reshuffles itself makes every save look like a change to anything
    /// diffing it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    slots: BTreeMap<String, PaneWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sunday_service: Option<u32>,
    /// Absent → -1, "never said", not "none".
    #[serde(default = "default_bible_reads", skip_serializing_if = "is_unset_reads")]
    bible_reads: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    bible_reads_credited: bool,
    #[serde(default)]
    verse_per_line: bool,
    /// Absent → off, and off is not written, so a reader who never used it keeps
    /// their file unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    page_turn: bool,
    /// `default_true`, not serde's `bool` default: absent in an older file means
    /// the numbers and italics that reader has always had, not their removal.
    #[serde(default = "default_true")]
    verse_numbers: bool,
    #[serde(default = "default_true")]
    added_italics: bool,
    #[serde(default = "default_theme_token")]
    theme: String,
    /// Absent → the shipped default face, which is what that reader has been
    /// looking at.
    #[serde(default = "default_font_token")]
    text_font: String,
    #[serde(default = "default_font_token")]
    chrome_font: String,
    #[serde(default = "default_copy_style")]
    copy_style: String,
    #[serde(default = "default_side_margin")]
    side_margin: f64,
    #[serde(default = "default_line_spacing")]
    line_spacing: f64,
    #[serde(default)]
    history: Vec<PaneWire>,
    // Every `Option` below is an additive key: absent means the default given in
    // `from_wire`, and skipped rather than written null so an existing config
    // does not grow keys just because this build knows about the feature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_analysis: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    machine_analysis: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    church: Option<ChurchWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    present_shares_as_new: Option<bool>,
    /// `alias`: this shipped as `strongsDeOff` when German was the only
    /// translation, and that key sits in config files on devices. Read the old
    /// spelling, write only the new one.
    #[serde(default, alias = "strongsDeOff", skip_serializing_if = "Option::is_none")]
    localized_lexicon_off: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    akjv_overlay: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intro: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    devotional_seeded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    concept_study: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    gospel_thread: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChurchWire {
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    service: Option<u16>,
    #[serde(default)]
    url: String,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct PaneWire {
    book: String,
    chapter: u16,
    /// Absent = top of the chapter, or an old writer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verse: Option<u16>,
    /// Absent = the reader's own language.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    lang: String,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

fn default_mode_token() -> String {
    StudyMode::default().token().to_string()
}
fn default_body_size() -> f64 {
    Config::default().body_size
}
fn default_theme_token() -> String {
    ThemeChoice::default().token().to_string()
}
/// For an on-by-default switch: an absent key must not read as "turned off".
fn default_true() -> bool {
    true
}
/// Never said, which is not the same as "none".
fn default_bible_reads() -> i64 {
    -1
}
fn is_unset_reads(n: &i64) -> bool {
    *n < 0
}
fn default_font_token() -> String {
    Font::default().token().to_string()
}
fn default_copy_style() -> String {
    Config::default().copy_style
}
fn default_side_margin() -> f64 {
    Config::default().side_margin
}
fn default_line_spacing() -> f64 {
    Config::default().line_spacing
}

/// A copy-style token, falling back to the default on an unknown value.
fn normalize_copy_style(s: &str) -> String {
    if COPY_STYLES.contains(&s) {
        s.to_string()
    } else {
        Config::default().copy_style
    }
}
/// Clamp a finite value into `[lo, hi]`, else the fallback (guards a corrupt file).
fn clamp_or(v: f64, lo: f64, hi: f64, fallback: f64) -> f64 {
    if v.is_finite() && v >= lo && v <= hi {
        v
    } else {
        fallback
    }
}

impl Config {
    fn from_wire(w: ConfigWire) -> Config {
        let n_panes = w.open_panes.len();
        let mode = StudyMode::parse(&w.study_mode).unwrap_or_default();
        Config {
            mode,
            // Guard against a corrupt / absurd size.
            body_size: if w.body_size.is_finite() && w.body_size >= 6.0 && w.body_size <= 96.0 {
                w.body_size
            } else {
                Config::default().body_size
            },
            panes: w
                .open_panes
                .into_iter()
                .map(|p| PaneRef {
                    book: p.book,
                    chapter: p.chapter.max(1),
                    verse: p.verse.filter(|v| *v >= 1),
                    lang: p.lang,
                })
                .collect(),
            // Clamp: shells index panes with this.
            active: if n_panes == 0 { 0 } else { w.active_pane.min(n_panes - 1) },
            bible_reads: w.bible_reads,
            bible_reads_credited: w.bible_reads_credited,
            slots: w
                .slots
                .into_iter()
                .map(|(k, p)| {
                    (
                        k,
                        PaneRef {
                            book: p.book,
                            chapter: p.chapter.max(1),
                            verse: p.verse.filter(|v| *v >= 1),
                            lang: p.lang,
                        },
                    )
                })
                .collect(),
            verse_per_line: w.verse_per_line,
            page_turn: w.page_turn,
            verse_numbers: w.verse_numbers,
            added_italics: w.added_italics,
            theme: ThemeChoice::parse(&w.theme).unwrap_or_default(),
            // A face this build does not ship (hand-edited, or a later build's
            // face) falls back to the default rather than to nothing.
            text_font: Font::parse(&w.text_font).unwrap_or_default(),
            chrome_font: Font::parse(&w.chrome_font).unwrap_or_default(),
            copy_style: normalize_copy_style(&w.copy_style),
            side_margin: clamp_or(w.side_margin, 0.0, 160.0, Config::default().side_margin),
            line_spacing: clamp_or(w.line_spacing, 1.0, 3.0, Config::default().line_spacing),
            history: w
                .history
                .into_iter()
                .map(|p| PaneRef { book: p.book, chapter: p.chapter.max(1), verse: None, lang: String::new() })
                .take(HISTORY_CAP)
                .collect(),
            // The tiers are opt-in; a reader who switched one on has an explicit
            // `true` on the wire.
            human_analysis: w.human_analysis.unwrap_or(false),
            machine_analysis: w.machine_analysis.unwrap_or(false),
            present_shares_as_new: w.present_shares_as_new.unwrap_or(true),
            localized_lexicon_off: w.localized_lexicon_off.unwrap_or(false),
            akjv_overlay: w.akjv_overlay.unwrap_or(false),
            // Absent = never offered, so a config written before devotionals
            // existed gets the offer once.
            devotional_seeded: w.devotional_seeded.unwrap_or(false),
            intro: match w.intro.as_deref() {
                Some("new") => "new".to_string(),
                Some("curious") => "curious".to_string(),
                _ => String::new(), // unknown token → no welcome to re-open
            },
            // A language this build does not ship reads as "follow the device",
            // not English.
            language: match w.language.as_deref() {
                Some(code) if crate::i18n::Lang::ALL.iter().any(|l| l.code() == code) => code.to_string(),
                _ => String::new(),
            },
            // An id, not a token: whether the plan still exists is answered at
            // use (a stale id reads as normal mode), so nothing validates here.
            concept_study: w.concept_study.map(|s| s.trim().to_string()).unwrap_or_default(),
            gospel_thread: w.gospel_thread.map(|s| s.trim().to_string()).unwrap_or_default(),
            // A minute outside the day is corrupt; read it as "never set".
            sunday_service: w.sunday_service.filter(|m| *m < 24 * 60),
            church: w
                .church
                .map(|c| Church {
                    name: c.name.trim().to_string(),
                    // Same clamp as `sunday_service`.
                    service: c.service.filter(|m| *m < 24 * 60),
                    url: c.url.trim().to_string(),
                })
                .unwrap_or_default(),
        }
    }

    fn to_wire(&self) -> ConfigWire {
        ConfigWire {
            study_mode: self.mode.token().to_string(),
            body_size: self.body_size,
            open_panes: self
                .panes
                .iter()
                .map(|p| PaneWire {
                    book: p.book.clone(),
                    chapter: p.chapter,
                    verse: p.verse,
                    lang: p.lang.clone(),
                    extra: Map::new(),
                })
                .collect(),
            active_pane: self.active,
            bible_reads: self.bible_reads,
            bible_reads_credited: self.bible_reads_credited,
            slots: self
                .slots
                .iter()
                .map(|(k, p)| {
                    (
                        k.clone(),
                        PaneWire {
                            book: p.book.clone(),
                            chapter: p.chapter,
                            verse: p.verse,
                            lang: p.lang.clone(),
                            extra: Map::new(),
                        },
                    )
                })
                .collect(),
            verse_per_line: self.verse_per_line,
            page_turn: self.page_turn,
            verse_numbers: self.verse_numbers,
            added_italics: self.added_italics,
            theme: self.theme.token().to_string(),
            text_font: self.text_font.token().to_string(),
            chrome_font: self.chrome_font.token().to_string(),
            copy_style: self.copy_style.clone(),
            side_margin: self.side_margin,
            line_spacing: self.line_spacing,
            history: self
                .history
                .iter()
                .take(HISTORY_CAP)
                // History is a list of places, not of panes: no language.
                .map(|p| PaneWire {
                    book: p.book.clone(),
                    chapter: p.chapter,
                    verse: None,
                    lang: String::new(),
                    extra: Map::new(),
                })
                .collect(),
            human_analysis: Some(self.human_analysis),
            machine_analysis: Some(self.machine_analysis),
            present_shares_as_new: Some(self.present_shares_as_new),
            localized_lexicon_off: Some(self.localized_lexicon_off),
            akjv_overlay: Some(self.akjv_overlay),
            intro: (!self.intro.is_empty()).then(|| self.intro.clone()),
            devotional_seeded: self.devotional_seeded.then_some(true),
            language: (!self.language.is_empty()).then(|| self.language.clone()),
            concept_study: (!self.concept_study.is_empty()).then(|| self.concept_study.clone()),
            gospel_thread: (!self.gospel_thread.is_empty()).then(|| self.gospel_thread.clone()),
            church: (!self.church.is_empty()).then(|| ChurchWire {
                name: self.church.name.clone(),
                service: self.church.service,
                url: self.church.url.clone(),
                extra: Map::new(),
            }),
            sunday_service: self.sunday_service,
            extra: Map::new(),
        }
    }
}

/// Copy the unknown keys of the file being replaced onto the settings about to
/// be written over it — object, church and each pane (see [`ConfigWire`]).
///
/// A pane keeps its unknown keys only when the pane in the same slot is still
/// the same chapter: those lists are regenerated from the shell's state, so
/// there is no identity to match on, and anything else is dropped rather than
/// attached to a passage it was never about.
fn carry_unknown(old: ConfigWire, new: &mut ConfigWire) {
    new.extra = old.extra;
    if let (Some(from), Some(to)) = (old.church, new.church.as_mut()) {
        to.extra = from.extra;
    }
    for (from, to) in [(old.open_panes, &mut new.open_panes), (old.history, &mut new.history)] {
        for (from, to) in from.into_iter().zip(to.iter_mut()) {
            if from.book == to.book && from.chapter == to.chapter {
                to.extra = from.extra;
            }
        }
    }
}

/// The per-user config directory for this app, per platform:
/// - Windows: `%APPDATA%\plumbline`
/// - macOS: `$HOME/Library/Application Support/plumbline`
/// - other Unix: `$XDG_CONFIG_HOME/plumbline` (else `$HOME/.config/plumbline`)
pub fn config_dir() -> Option<PathBuf> {
    let app = "plumbline";
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|base| Path::new(&base).join(app))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| Path::new(&home).join("Library").join("Application Support").join(app))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
            Some(Path::new(&xdg).join(app))
        } else {
            std::env::var_os("HOME").map(|home| Path::new(&home).join(".config").join(app))
        }
    }
}

/// The config file path (`<config_dir>/config.json`).
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.json"))
}

/// Load the config at `path`, returning `(config, first_run)`; `first_run` is
/// true only when no file existed. An unreadable file loads as the default with
/// `first_run = false`, and an unparseable one is moved aside first so the next
/// save cannot write defaults over it (see [`crate::store::move_damaged_aside`]).
pub fn load_from(path: impl AsRef<Path>) -> (Config, bool) {
    let path = path.as_ref();
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), true),
        Err(_) => (Config::default(), false),
        Ok(bytes) => match serde_json::from_slice::<ConfigWire>(&bytes) {
            Ok(w) => (Config::from_wire(w), false),
            Err(_) => {
                crate::store::move_damaged_aside(path, &bytes);
                (Config::default(), false)
            }
        },
    }
}

/// Atomically write the config to `path`, keeping any key the file already there
/// carries that this build does not understand (see [`ConfigWire`]). Unparseable
/// bytes yield nothing to keep, and [`load_from`] has already moved them aside.
pub fn save_to(path: impl AsRef<Path>, config: &Config) -> Result<(), Error> {
    let path = path.as_ref();
    let mut wire = config.to_wire();
    if let Some(old) = std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<ConfigWire>(&b).ok()) {
        carry_unknown(old, &mut wire);
    }
    let json = serde_json::to_string_pretty(&wire).map(|s| s + "\n").map_err(|e| Error::Parse(e.to_string()))?;
    crate::store::write_atomic(path, &json)
}

/// Load the config from the platform config path. Falls back to the default
/// (as first-run) when no config directory can be resolved.
pub fn load() -> (Config, bool) {
    match config_path() {
        Some(p) => load_from(p),
        None => (Config::default(), true),
    }
}

/// Save the config to the platform config path. A no-op when no config
/// directory resolves.
pub fn save(config: &Config) -> Result<(), Error> {
    match config_path() {
        Some(p) => save_to(p, config),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_is_first_run_default() {
        let path = std::env::temp_dir().join(format!("plumbline-cfg-missing-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (cfg, first) = load_from(&path);
        assert!(first);
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn roundtrips_and_reload_is_not_first_run() {
        let path = std::env::temp_dir().join(format!("plumbline-cfg-rt-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let cfg = Config {
            mode: StudyMode::Full,
            body_size: 21.5,
            panes: vec![
                PaneRef { book: "John".into(), chapter: 3, verse: Some(16), lang: String::new() },
                PaneRef { book: "Rom".into(), chapter: 8, verse: None, lang: String::new() },
            ],
            active: 1,
            // Every field here is set away from its default: a default value is
            // skipped on the wire, so it would round-trip even through a key
            // that is never written or never read.
            bible_reads: 7,
            bible_reads_credited: true,
            slots: BTreeMap::from([(
                "sunday-morning".to_string(),
                PaneRef { book: "Ps".into(), chapter: 23, verse: Some(4), lang: String::new() },
            )]),
            verse_per_line: true,
            page_turn: true,
            verse_numbers: false,
            added_italics: false,
            theme: ThemeChoice::Night,
            // Two *different* non-default faces: equal values would round-trip
            // even if one axis carried the other's.
            text_font: Font::FiraCode,
            chrome_font: Font::Inter,
            copy_style: "verseMarkdown".to_string(),
            side_margin: 40.0,
            line_spacing: 1.6,
            history: vec![
                PaneRef { book: "Gen".into(), chapter: 1, verse: None, lang: String::new() },
                PaneRef { book: "Rom".into(), chapter: 8, verse: None, lang: String::new() },
            ],
            human_analysis: true,
            machine_analysis: false,
            present_shares_as_new: false,
            akjv_overlay: true,
            intro: "curious".to_string(),
            devotional_seeded: true,
            language: "de".to_string(),
            concept_study: "run-grace".to_string(),
            gospel_thread: "My Gospel Walk".to_string(),
            localized_lexicon_off: true,
            church: Church {
                name: "Grace Bible Church".into(),
                service: Some(10 * 60),
                url: "https://example.org".into(),
            },
            sunday_service: Some(10 * 60),
        };
        save_to(&path, &cfg).unwrap();

        let (back, first) = load_from(&path);
        assert!(!first);
        assert_eq!(back, cfg);
        let _ = std::fs::remove_file(&path);
    }

    /// A scratch config dir for one test (unique per test + process).
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("plumbline-cfg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Fails if a damaged config is not moved aside before the next save: it
    /// loads as the default and the save overwrites it, taking the reader's
    /// history, panes and church with it.
    #[test]
    fn damaged_config_is_moved_aside_before_the_next_save() {
        let dir = scratch("rescue");
        let path = dir.join("config.json");
        let bad = dir.join("config.json.bad");
        // Truncated mid-write: valid prefix, no closing brace.
        let damaged = r#"{"studyMode":"full","history":[{"book":"Ps","chapter":11"#;
        std::fs::write(&path, damaged).unwrap();

        let (cfg, first_run) = load_from(&path);
        assert!(!first_run);
        assert_eq!(cfg, Config::default());
        assert_eq!(
            std::fs::read_to_string(&bad).ok().as_deref(),
            Some(damaged),
            "the damaged bytes must be recoverable at config.json.bad"
        );

        // The save must now write a fresh, valid file without clobbering the rescue.
        save_to(&path, &cfg).unwrap();
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), damaged, "the save clobbered the rescue");
        let (back, first_run) = load_from(&path);
        assert!(!first_run);
        assert_eq!(back, cfg);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The rescue is the original one: a second failure is usually the same
    /// damage saved back out, and must not replace the copy that still holds
    /// the reader's data.
    #[test]
    fn an_existing_rescue_is_kept() {
        let dir = scratch("rescue-twice");
        let path = dir.join("config.json");
        let bad = dir.join("config.json.bad");
        let first_damage = r#"{"studyMode":"full","church":{"name":"Grace Bible Chur"#;
        std::fs::write(&bad, first_damage).unwrap();
        std::fs::write(&path, "{{{").unwrap();

        let (cfg, first_run) = load_from(&path);
        assert!(!first_run);
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), first_damage, "the first rescue was lost");
        save_to(&path, &cfg).unwrap();
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), first_damage);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An empty file has nothing in it to recover, so it must not spend the one
    /// `.bad` slot that real damage will need.
    #[test]
    fn an_empty_config_does_not_spend_the_rescue_slot() {
        let dir = scratch("rescue-empty");
        let path = dir.join("config.json");
        let bad = dir.join("config.json.bad");

        std::fs::write(&path, "\n").unwrap();
        let (_, first_run) = load_from(&path);
        assert!(!first_run);
        assert!(!bad.exists(), "an empty config is not worth rescuing");

        let damaged = r#"{"bodySize":21.5,"#;
        std::fs::write(&path, damaged).unwrap();
        load_from(&path);
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), damaged);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `akjvOverlay` must survive a load/save/load: this is the key spelling the
    /// shell writes, and a save that dropped it would turn the reader's overlay
    /// back off next launch. Absent still means off.
    #[test]
    fn akjv_overlay_survives_a_load_save_load() {
        let dir = scratch("akjv");
        let path = dir.join("config.json");

        // As the shell writes it.
        std::fs::write(&path, r#"{"studyMode":"simple","akjvOverlay":true}"#).unwrap();
        let (cfg, first_run) = load_from(&path);
        assert!(!first_run);
        assert!(cfg.akjv_overlay, "the shells' akjvOverlay never reached Config");

        // Check the bytes, not just the struct: the next launch reads the file.
        save_to(&path, &cfg).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains(r#""akjvOverlay": true"#), "the save dropped akjvOverlay: {json}");
        let (back, _) = load_from(&path);
        assert!(back.akjv_overlay, "the overlay was off again after a restart");

        // Absent → off.
        std::fs::write(&path, r#"{"studyMode":"simple"}"#).unwrap();
        assert!(!load_from(&path).0.akjv_overlay);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The active concept study's plan id must survive a load/save/load, or a
    /// save drops the reader out of a days-long sweep on the next launch. Absent
    /// means normal reading mode; the value is trimmed but never validated away.
    #[test]
    fn concept_study_mode_survives_a_load_save_load() {
        let dir = scratch("concept-study");
        let path = dir.join("config.json");

        std::fs::write(&path, r#"{"studyMode":"simple","conceptStudy":"run-grace"}"#).unwrap();
        let (cfg, _) = load_from(&path);
        assert_eq!(cfg.concept_study, "run-grace", "the shell's conceptStudy never reached Config");

        save_to(&path, &cfg).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains(r#""conceptStudy": "run-grace""#), "the save dropped conceptStudy: {json}");
        assert_eq!(load_from(&path).0.concept_study, "run-grace");

        // Absent → normal mode, and the key is not written when empty.
        std::fs::write(&path, r#"{"studyMode":"simple"}"#).unwrap();
        let (cfg, _) = load_from(&path);
        assert!(cfg.concept_study.is_empty());
        save_to(&path, &cfg).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("conceptStudy"), "empty must not write the key");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Forward compatibility: the on-disk formats evolve additively (CLAUDE.md
    /// §Data formats), so a key an older build drops is dropped for good.
    /// Settings save on nearly every interaction, so this file strips fastest.
    #[test]
    fn the_config_keeps_the_keys_of_a_later_build() {
        let dir = scratch("forward");
        let path = dir.join("config.json");
        std::fs::write(
            &path,
            r#"{"studyMode":"full","bodySize":21.5,
                "openPanes":[{"book":"John","chapter":3,"verse":16,"scroll":0.4,"pinned":{"by":"reader"}}],
                "activePane":0,
                "history":[{"book":"Gen","chapter":1,"openedFrom":"search"}],
                "church":{"name":"Grace Bible Church","info":"Sundays 10am","url":"","mapUrl":"https://maps.example/g"},
                "lectionary":"acna","gestures":{"swipe":"chapter"},"pinnedBooks":["Ps","John"]}"#,
        )
        .unwrap();

        let (cfg, first_run) = load_from(&path);
        assert!(!first_run);
        save_to(&path, &cfg).unwrap();

        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["studyMode"], "full", "the settings themselves must land");
        assert_eq!(back["bodySize"], 21.5);
        assert_eq!(back["lectionary"], "acna", "an unknown scalar was stripped");
        assert_eq!(back["gestures"], serde_json::json!({"swipe":"chapter"}), "an unknown object was stripped");
        assert_eq!(back["pinnedBooks"], serde_json::json!(["Ps", "John"]), "an unknown array was stripped");
        assert_eq!(back["church"]["mapUrl"], "https://maps.example/g", "the church's unknown key was stripped");
        assert_eq!(back["openPanes"][0]["scroll"], 0.4, "a pane's unknown scalar was stripped");
        assert_eq!(
            back["openPanes"][0]["pinnedBy"],
            Value::Null,
            "a pane's keys must not be renamed on the way through"
        );
        assert_eq!(
            back["openPanes"][0]["pinned"],
            serde_json::json!({"by":"reader"}),
            "a pane's unknown object was stripped"
        );
        assert_eq!(back["history"][0]["openedFrom"], "search", "a history entry's unknown key was stripped");

        // A second load/save is still lossless — the keys are not one-shot.
        let (cfg, _) = load_from(&path);
        save_to(&path, &cfg).unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["lectionary"], "acna");
        assert_eq!(back["openPanes"][0]["scroll"], 0.4);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pane's text language survives the file, and its absence still means
    /// "the reader's own" — the additive rule applied to the field that makes
    /// German-beside-English reopen as German beside English.
    #[test]
    fn a_pane_carries_its_own_text_language() {
        let dir = scratch("panelang");
        let path = dir.join("config.json");
        let cfg = Config {
            panes: vec![
                PaneRef { book: "John".into(), chapter: 3, verse: None, lang: String::new() },
                PaneRef { book: "John".into(), chapter: 3, verse: None, lang: "de".into() },
            ],
            ..Config::default()
        };
        save_to(&path, &cfg).unwrap();

        let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        // The English pane writes no key, so a file from before this feature and
        // one from a reader who never used it are the same bytes.
        assert_eq!(written["openPanes"][0]["lang"], Value::Null);
        assert_eq!(written["openPanes"][1]["lang"], "de");

        let (back, _) = load_from(&path);
        assert_eq!(back.panes, cfg.panes);

        // A file written before panes had languages reads as the reader's own.
        std::fs::write(&path, r#"{"openPanes":[{"book":"Ps","chapter":23}]}"#).unwrap();
        let (old, _) = load_from(&path);
        assert_eq!(old.panes[0].lang, "");
    }

    #[test]
    fn a_config_with_no_unknown_keys_is_written_exactly_as_before() {
        let dir = scratch("golden");
        let path = dir.join("config.json");
        save_to(&path, &Config::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{
  "studyMode": "simple",
  "bodySize": 20.0,
  "openPanes": [],
  "activePane": 0,
  "versePerLine": false,
  "verseNumbers": true,
  "addedItalics": true,
  "theme": "system",
  "textFont": "eb-garamond",
  "chromeFont": "eb-garamond",
  "copyStyle": "verseRef",
  "sideMargin": 28.0,
  "lineSpacing": 1.35,
  "history": [],
  "humanAnalysis": false,
  "machineAnalysis": false,
  "presentSharesAsNew": true,
  "localizedLexiconOff": false,
  "akjvOverlay": false
}
"#
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn page_turn_and_sunday_service_round_trip_and_stay_off_the_default_file() {
        let dir = scratch("pageturn");
        let path = dir.join("config.json");
        let cfg = Config { page_turn: true, sunday_service: Some(10 * 60 + 30), ..Config::default() };
        save_to(&path, &cfg).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"pageTurn\": true"));
        assert!(text.contains("\"sundayService\": 630"));
        let (back, _) = load_from(&path);
        assert!(back.page_turn);
        assert_eq!(back.sunday_service, Some(630));

        // A minute outside the day reads as never-set.
        std::fs::write(&path, r#"{"sundayService": 4000}"#).unwrap();
        assert_eq!(load_from(&path).0.sunday_service, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn absurd_size_and_bad_mode_fall_back() {
        let path = std::env::temp_dir().join(format!("plumbline-cfg-bad-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"studyMode":"wat","bodySize":9000}"#).unwrap();
        let (cfg, first) = load_from(&path);
        assert!(!first); // a damaged file is not a fresh first run
        assert_eq!(cfg.mode, StudyMode::Simple);
        assert_eq!(cfg.body_size, Config::default().body_size);
        let _ = std::fs::remove_file(&path);
    }
    #[test]
    fn language_is_a_choice_or_the_device_s_and_never_an_invented_one() {
        let dir = scratch("language");
        let path = dir.join("config.json");

        // Absent: follow the device, not "en" — writing that on the first locale
        // resolve would freeze a German reader into English.
        assert_eq!(Config::default().language, "");

        // A language this build ships round-trips through the file.
        let picked = Config { language: "de".to_string(), ..Config::default() };
        save_to(&path, &picked).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"language\": \"de\""));
        assert_eq!(load_from(&path).0.language, "de");

        // One it does not ship reads as "follow the device".
        std::fs::write(&path, r#"{"language":"it"}"#).unwrap();
        assert_eq!(load_from(&path).0.language, "");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(load_from(&path).0.language, "");
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    /// Shells index panes with `active`, so a corrupt/stale value must come back
    /// clamped.
    #[test]
    fn active_pane_is_clamped_to_the_pane_list() {
        let dir = std::env::temp_dir().join(format!("plumbline-config-clamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");

        std::fs::write(
            &path,
            r#"{"studyMode":"full","bodySize":18.0,"openPanes":[{"book":"Gen","chapter":1},{"book":"John","chapter":3}],"activePane":9}"#,
        )
        .unwrap();
        let (cfg, first_run) = load_from(&path);
        assert!(!first_run);
        assert_eq!(cfg.active, 1);

        std::fs::write(&path, r#"{"activePane":3}"#).unwrap();
        let (cfg, _) = load_from(&path);
        assert_eq!(cfg.active, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
