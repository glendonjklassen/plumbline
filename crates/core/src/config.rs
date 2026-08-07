//! Application config: the study mode chosen at first run plus the reader's
//! body-text size, persisted as JSON at the platform's per-user config
//! directory.
//!
//! Decision #4 (README §For developers, decisions table; revised 2026-07-25)
//! is *guided first-run*: the first launch picks the analysis tiers
//! (scholars' / machine) with examples — the text and the reader's own data
//! are always on. That choice — and the live font size — live here so every
//! shell (Compose and the PWA) reads and writes the same file through one
//! code path.
//!
//! Paths are resolved per-OS and composed with [`Path::join`] (never a hardcoded
//! separator); writes go through the cross-platform atomic writer in
//! [`crate::store`], so this is correct on Windows and Unix alike.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::theme::ThemeChoice;
use crate::Error;

/// How much of the app the reader sees. `Full` unlocks the study surface
/// (threads, tags, weave authoring/review, and — when it lands — the R&D tier);
/// `Simple` is a clean reader with lookup and search only.
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

/// One reopened reading pane: which passage it showed, and (additively since
/// 2026-07-25) the first visible verse, so a session reopens mid-chapter where
/// the reader left off. `None` = top of the chapter; history entries don't
/// carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRef {
    pub book: String,
    pub chapter: u16,
    pub verse: Option<u16>,
}

/// The reader's home church, carried in a shared link so one QR hands over
/// both the Bible and where to find the people who sent it (2026-07-27).
/// Every part is optional; an empty name means "not set".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Church {
    pub name: String,
    /// One free line — when and where they meet.
    pub info: String,
    /// Their website, if they have one.
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
    /// The reading panes from the last session (empty on a fresh install → the
    /// app opens its default passage). Selections/scroll are transient.
    pub panes: Vec<PaneRef>,
    /// Which pane was active last session.
    pub active: usize,
    /// Verse-per-line reading mode (each verse starts a fresh line).
    pub verse_per_line: bool,
    /// The reader's colour theme (Tier 0 #5). `System` follows the OS.
    pub theme: ThemeChoice,
    /// Default one-tap copy shape, for shells whose copy is a single action
    /// (e.g. the Android long-press). A verse `CopyKind` token:
    /// `"verse"` / `"verseRef"` / `"verseMarkdown"`.
    pub copy_style: String,
    /// Reader horizontal margin in px — the space on either side of the text
    /// column (feature-manifest MARGIN; default 28).
    pub side_margin: f64,
    /// Reader line-height as a multiple of the text height (feature-manifest
    /// line_height; default 1.35).
    pub line_spacing: f64,
    /// Reading history, most-recent-first, deduped by (book, chapter) and capped
    /// at [`HISTORY_CAP`] — powers a "recently read" list + jump-back.
    pub history: Vec<PaneRef>,
    /// Show the curated-scholarship analysis tiers (renderings, morphology,
    /// same-root, TSK). Replaces half of the old Simple/Full switch; the text
    /// and the reader's own data are never gated.
    pub human_analysis: bool,
    /// Show the learned/statistical tiers (the symbolic concept engine, SIF,
    /// leitwort).
    pub machine_analysis: bool,
    /// The reader's home church — shown in the welcome when a shared link
    /// carried one, and attached to the links this reader shares.
    pub church: Church,
    /// Whether a link shared from PRESENT opens for a new believer: that
    /// screen is what you show someone face to face, so the person receiving
    /// it is usually meeting the Bible, not setting up a study tool. On by
    /// default (2026-07-27); the recipient can still change everything.
    pub present_shares_as_new: bool,
    /// The plain-English overlay (the AKJV delta) on the reader. Off unless the
    /// reader asks for it, and reader-only either way — memorize, Present, copy
    /// and share stay KJV (see [`crate::akjv`]). Kept here since 2026-07-29:
    /// both shells were writing it and the core was dropping it, so the switch
    /// never survived a restart.
    pub akjv_overlay: bool,
    /// Which welcome this reader was given ("new" | "curious"), empty when
    /// none. The shells offer it again from the chrome — a reader shouldn't
    /// have to reinstall to read it twice (2026-07-27).
    pub intro: String,
    /// The reader's language, as a code ([`crate::i18n::Lang::code`]).
    ///
    /// EMPTY MEANS "follow the device", which is not the same as English: a
    /// German phone should open in German without anyone visiting Settings,
    /// and storing "en" the first time we resolve it would freeze that reader
    /// into English forever. The shell passes its locale in and the core
    /// decides; only an explicit choice is written here.
    pub language: String,
}

/// A verse copy-shape token accepted for [`Config::copy_style`].
pub const COPY_STYLES: [&str; 3] = ["verse", "verseRef", "verseMarkdown"];

/// The most reading-history entries kept (persisted + returned).
pub const HISTORY_CAP: usize = 50;

impl Default for Config {
    fn default() -> Config {
        Config {
            mode: StudyMode::Simple,
            body_size: 18.0,
            panes: Vec::new(),
            active: 0,
            verse_per_line: false,
            theme: ThemeChoice::default(),
            copy_style: "verseRef".to_string(),
            side_margin: 28.0,
            line_spacing: 1.35,
            history: Vec::new(),
            // Opt-in: see the note on `from_wire` below.
            human_analysis: false,
            machine_analysis: false,
            church: Church::default(),
            present_shares_as_new: true,
            akjv_overlay: false,
            intro: String::new(),
            language: String::new(),
        }
    }
}

// On-disk form (camelCase, mode as a token). Missing fields fall back to the
// default so the file evolves additively.
//
// Evolving additively cuts both ways: a field this build has never heard of has
// to survive its saves too. The formats are frozen contracts (CLAUDE.md §Data
// formats) and a sideloaded APK never auto-updates, so a v1.0 that drops a v1.1
// key drops it for good on that device. Every struct here therefore ends in a
// flattened catch-all, and `save_to` fills them from the file it is replacing —
// the reader's settings cannot be carried on `Config` itself, because
// `crates/ffi` rebuilds that value field by field out of the shell's wire
// payload on every save (`wire::config_from_wire`), so an in-memory field would
// be dropped there instead. Serde matches the declared fields first, so the
// catch-all holds only keys we have never heard of; a key a later version
// promotes to a real field stops arriving in it the moment that field exists,
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
    #[serde(default)]
    verse_per_line: bool,
    #[serde(default = "default_theme_token")]
    theme: String,
    #[serde(default = "default_copy_style")]
    copy_style: String,
    #[serde(default = "default_side_margin")]
    side_margin: f64,
    #[serde(default = "default_line_spacing")]
    line_spacing: f64,
    #[serde(default)]
    history: Vec<PaneWire>,
    // The per-tier analysis gates (2026-07-25). Absent in an older file →
    // derived from studyMode, preserving what the reader was seeing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_analysis: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    machine_analysis: Option<bool>,
    /// The home church (2026-07-27); absent in every older file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    church: Option<ChurchWire>,
    /// Present-screen shares open as a new believer (2026-07-27); absent in an
    /// older file → on, which is the default the feature shipped with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    present_shares_as_new: Option<bool>,
    /// The plain-English overlay (2026-07-29); absent in an older file → off,
    /// which is what every reader saw while the field was being dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    akjv_overlay: Option<bool>,
    /// The welcome this reader was given (2026-07-27); absent when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    intro: Option<String>,
    /// The reader's chosen language; absent means "follow the device", which
    /// is why this skips rather than writing null — an existing config must
    /// not grow a key just because this build knows about languages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    language: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChurchWire {
    #[serde(default)]
    name: String,
    #[serde(default)]
    info: String,
    #[serde(default)]
    url: String,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct PaneWire {
    book: String,
    chapter: u16,
    /// First visible verse (additive; absent = top / an old writer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    verse: Option<u16>,
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
                .map(|p| PaneRef { book: p.book, chapter: p.chapter.max(1), verse: p.verse.filter(|v| *v >= 1) })
                .collect(),
            // Clamp: shells index panes with this.
            active: if n_panes == 0 { 0 } else { w.active_pane.min(n_panes - 1) },
            verse_per_line: w.verse_per_line,
            theme: ThemeChoice::parse(&w.theme).unwrap_or_default(),
            copy_style: normalize_copy_style(&w.copy_style),
            side_margin: clamp_or(w.side_margin, 0.0, 160.0, Config::default().side_margin),
            line_spacing: clamp_or(w.line_spacing, 1.0, 3.0, Config::default().line_spacing),
            history: w
                .history
                .into_iter()
                .map(|p| PaneRef { book: p.book, chapter: p.chapter.max(1), verse: None })
                .take(HISTORY_CAP)
                .collect(),
            // Absent in an older file → on. (Deriving from studyMode would
            // surprise-hide the tiers on devices whose shell defaulted to Full
            // without persisting it — the gates are opt-OUT switches.)
            // Absent = off. The tiers are opt-in (2026-07-28): a first-time
            // reader should inherit the text, not a study apparatus. A reader
            // who switched one on has an explicit `true` here and keeps it.
            human_analysis: w.human_analysis.unwrap_or(false),
            machine_analysis: w.machine_analysis.unwrap_or(false),
            // Trimmed on the way in: these arrive from a shared link's query
            // string, where trailing spaces are an accident of copy-paste.
            present_shares_as_new: w.present_shares_as_new.unwrap_or(true),
            // Absent = off: the KJV is the text, and off is what the reader was
            // getting on every launch before this field was kept.
            akjv_overlay: w.akjv_overlay.unwrap_or(false),
            intro: match w.intro.as_deref() {
                Some("new") => "new".to_string(),
                Some("curious") => "curious".to_string(),
                _ => String::new(), // unknown token → no welcome to re-open
            },
            // A language this build does not ship reads as "follow the device"
            // rather than English: the reader chose a language once, and the
            // honest response to not having it is to fall back to their
            // system's, not to overrule them with ours.
            language: match w.language.as_deref() {
                Some(code) if crate::i18n::Lang::ALL.iter().any(|l| l.code() == code) => code.to_string(),
                _ => String::new(),
            },
            church: w
                .church
                .map(|c| Church {
                    name: c.name.trim().to_string(),
                    info: c.info.trim().to_string(),
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
                .map(|p| PaneWire { book: p.book.clone(), chapter: p.chapter, verse: p.verse, extra: Map::new() })
                .collect(),
            active_pane: self.active,
            verse_per_line: self.verse_per_line,
            theme: self.theme.token().to_string(),
            copy_style: self.copy_style.clone(),
            side_margin: self.side_margin,
            line_spacing: self.line_spacing,
            history: self
                .history
                .iter()
                .take(HISTORY_CAP)
                .map(|p| PaneWire { book: p.book.clone(), chapter: p.chapter, verse: None, extra: Map::new() })
                .collect(),
            human_analysis: Some(self.human_analysis),
            machine_analysis: Some(self.machine_analysis),
            present_shares_as_new: Some(self.present_shares_as_new),
            akjv_overlay: Some(self.akjv_overlay),
            intro: (!self.intro.is_empty()).then(|| self.intro.clone()),
            language: (!self.language.is_empty()).then(|| self.language.clone()),
            church: (!self.church.is_empty()).then(|| ChurchWire {
                name: self.church.name.clone(),
                info: self.church.info.clone(),
                url: self.church.url.clone(),
                extra: Map::new(),
            }),
            extra: Map::new(),
        }
    }
}

/// Copy the unknown keys of the file being replaced onto the settings about to be
/// written over it — the whole object, the church, and each pane in the two pane
/// lists (see the note on [`ConfigWire`]).
///
/// A pane keeps its unknown keys when the pane in the same slot is still the same
/// chapter. Those two lists are the live session, regenerated from the shell's
/// own state rather than edited in place, so there is no identity to match on;
/// same slot, same chapter is as far as an honest guess goes, and anything else
/// is dropped rather than attached to a passage it was never about.
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
///
/// Returns `None` only when the environment gives us nothing to build on.
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

/// Load the config at `path`, returning `(config, first_run)` where `first_run`
/// is true when no file existed yet (the caller should present the chooser). A
/// present-but-unreadable file loads as the default with `first_run = false`
/// (we do not re-prompt someone whose file merely got damaged) — and an
/// unparseable one is moved aside first, so the next save cannot quietly write
/// defaults over it (see [`move_damaged_aside`]).
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
/// carries that this build does not understand (see [`ConfigWire`]).
///
/// Unparseable bytes yield nothing to keep — and they are not there to be read
/// anyway, since [`load_from`] moves a damaged file aside before it comes to
/// this.
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

/// Save the config to the platform config path. A no-op error if no config
/// directory resolves (nothing we can do; the app still runs).
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
                PaneRef { book: "John".into(), chapter: 3, verse: Some(16) },
                PaneRef { book: "Rom".into(), chapter: 8, verse: None },
            ],
            active: 1,
            verse_per_line: true,
            theme: ThemeChoice::Night,
            copy_style: "verseMarkdown".to_string(),
            side_margin: 40.0,
            line_spacing: 1.6,
            history: vec![
                PaneRef { book: "Gen".into(), chapter: 1, verse: None },
                PaneRef { book: "Rom".into(), chapter: 8, verse: None },
            ],
            human_analysis: true,
            machine_analysis: false,
            present_shares_as_new: false,
            akjv_overlay: true,
            intro: "curious".to_string(),
            language: "de".to_string(),
            church: Church {
                name: "Grace Bible Church".into(),
                info: "Sundays 10am · 12 Long Street".into(),
                url: "https://example.org".into(),
            },
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

    /// AUDIT 2026-07-29 data loss: a damaged config used to load as the default
    /// and then be *overwritten* by the next save, taking the reader's history,
    /// panes and church with it. It has to be moved aside first.
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

        // The save that used to destroy them now writes a fresh, valid file.
        save_to(&path, &cfg).unwrap();
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), damaged, "the save clobbered the rescue");
        let (back, first_run) = load_from(&path);
        assert!(!first_run);
        assert_eq!(back, cfg);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The rescue is the ORIGINAL one: a second failure is usually the same
    /// damage saved back out, so it must not replace the copy that still has
    /// the reader's data in it.
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

    /// AUDIT 2026-07-29: both shells wrote `akjvOverlay`, the core had no field
    /// for it, and so every save wrote the object back without it — the reader
    /// turned the plain-English overlay on and found it off again next launch.
    /// The key spelling here is the one the shells use; absent still means off.
    #[test]
    fn akjv_overlay_survives_a_load_save_load() {
        let dir = scratch("akjv");
        let path = dir.join("config.json");

        // Written by a shell (Android ConfigState / the web session snapshot).
        std::fs::write(&path, r#"{"studyMode":"simple","akjvOverlay":true}"#).unwrap();
        let (cfg, first_run) = load_from(&path);
        assert!(!first_run);
        assert!(cfg.akjv_overlay, "the shells' akjvOverlay never reached Config");

        // The save that used to drop it. Check the bytes, not just the struct:
        // it is the written file the next launch reads.
        save_to(&path, &cfg).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains(r#""akjvOverlay": true"#), "the save dropped akjvOverlay: {json}");
        let (back, _) = load_from(&path);
        assert!(back.akjv_overlay, "the overlay was off again after a restart");

        // Absent → off, which is what every reader saw before this landed.
        std::fs::write(&path, r#"{"studyMode":"simple"}"#).unwrap();
        assert!(!load_from(&path).0.akjv_overlay);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// AUDIT 2026-07-29 forward compatibility: the on-disk formats evolve
    /// **additively** (CLAUDE.md §Data formats), and a sideloaded APK never
    /// auto-updates — so a key this build drops is dropped for good on that
    /// device. Settings are saved on nearly every interaction, so this file is the
    /// one a v1.0 would strip fastest.
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

    /// A config with nothing unknown in it is written byte for byte as it was
    /// before any of that landed — this file rides in the backup zip too.
    #[test]
    fn a_config_with_no_unknown_keys_is_written_exactly_as_before() {
        let dir = scratch("golden");
        let path = dir.join("config.json");
        save_to(&path, &Config::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{
  "studyMode": "simple",
  "bodySize": 18.0,
  "openPanes": [],
  "activePane": 0,
  "versePerLine": false,
  "theme": "system",
  "copyStyle": "verseRef",
  "sideMargin": 28.0,
  "lineSpacing": 1.35,
  "history": [],
  "humanAnalysis": false,
  "machineAnalysis": false,
  "presentSharesAsNew": true,
  "akjvOverlay": false
}
"#
        );
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

        // Absent: follow the device. NOT "en" — writing that the first time we
        // resolved a locale would freeze a German reader into English. The
        // golden test above proves the key is not even written.
        assert_eq!(Config::default().language, "");

        // A language this build ships round-trips through the file.
        let picked = Config { language: "de".to_string(), ..Config::default() };
        save_to(&path, &picked).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"language\": \"de\""));
        assert_eq!(load_from(&path).0.language, "de");

        // One it does NOT ship reads as "follow the device": the reader chose a
        // language once, and the honest answer to not having it is their
        // system's, not ours overruling them.
        std::fs::write(&path, r#"{"language":"fr"}"#).unwrap();
        assert_eq!(load_from(&path).0.language, "");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(load_from(&path).0.language, "");
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;

    /// REVIEW 2026-07-14 correctness #4: shells index panes with `active` —
    /// a corrupt/stale value must come back clamped.
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
