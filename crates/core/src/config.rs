//! Application config: the study mode chosen at first run plus the reader's
//! body-text size, persisted as JSON at the platform's per-user config
//! directory.
//!
//! Decision #4 (see PLAN.md) is *off by default + guided first-run*: the first
//! launch asks **Simple reader** vs **Full study**, and casual readers never see
//! the study/authoring complexity. That choice — and the live font size — live
//! here so every shell (GTK today, WinUI/Compose later) reads and writes the
//! same file through one code path.
//!
//! Paths are resolved per-OS and composed with [`Path::join`] (never a hardcoded
//! separator); writes go through the cross-platform atomic writer in
//! [`crate::store`], so this is correct on Windows and Unix alike.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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

/// One reopened reading pane: which passage it showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneRef {
    pub book: String,
    pub chapter: u16,
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
}

impl Default for Config {
    fn default() -> Config {
        Config { mode: StudyMode::Simple, body_size: 18.0, panes: Vec::new(), active: 0 }
    }
}

// On-disk form (camelCase, mode as a token). Missing fields fall back to the
// default so the file evolves additively.
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
}

#[derive(Serialize, Deserialize)]
struct PaneWire {
    book: String,
    chapter: u16,
}

fn default_mode_token() -> String {
    StudyMode::default().token().to_string()
}
fn default_body_size() -> f64 {
    Config::default().body_size
}

impl Config {
    fn from_wire(w: ConfigWire) -> Config {
        Config {
            mode: StudyMode::parse(&w.study_mode).unwrap_or_default(),
            // Guard against a corrupt / absurd size.
            body_size: if w.body_size.is_finite() && w.body_size >= 6.0 && w.body_size <= 96.0 {
                w.body_size
            } else {
                Config::default().body_size
            },
            panes: w.open_panes.into_iter().map(|p| PaneRef { book: p.book, chapter: p.chapter.max(1) }).collect(),
            active: w.active_pane,
        }
    }

    fn to_wire(&self) -> ConfigWire {
        ConfigWire {
            study_mode: self.mode.token().to_string(),
            body_size: self.body_size,
            open_panes: self.panes.iter().map(|p| PaneWire { book: p.book.clone(), chapter: p.chapter }).collect(),
            active_pane: self.active,
        }
    }
}

/// The per-user config directory for this app, per platform:
/// - Windows: `%APPDATA%\pure-study`
/// - macOS: `$HOME/Library/Application Support/pure-study`
/// - other Unix: `$XDG_CONFIG_HOME/pure-study` (else `$HOME/.config/pure-study`)
///
/// Returns `None` only when the environment gives us nothing to build on.
pub fn config_dir() -> Option<PathBuf> {
    let app = "pure-study";
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|base| Path::new(&base).join(app))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|home| Path::new(&home).join("Library").join("Application Support").join(app))
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
/// (we do not re-prompt someone whose file merely got damaged).
pub fn load_from(path: impl AsRef<Path>) -> (Config, bool) {
    let path = path.as_ref();
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), true),
        Err(_) => (Config::default(), false),
        Ok(bytes) => match serde_json::from_slice::<ConfigWire>(&bytes) {
            Ok(w) => (Config::from_wire(w), false),
            Err(_) => (Config::default(), false),
        },
    }
}

/// Atomically write the config to `path`.
pub fn save_to(path: impl AsRef<Path>, config: &Config) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(&config.to_wire())
        .map(|s| s + "\n")
        .map_err(|e| Error::Parse(e.to_string()))?;
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
        let path = std::env::temp_dir().join(format!("pure-cfg-missing-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (cfg, first) = load_from(&path);
        assert!(first);
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn roundtrips_and_reload_is_not_first_run() {
        let path = std::env::temp_dir().join(format!("pure-cfg-rt-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let cfg = Config {
            mode: StudyMode::Full,
            body_size: 21.5,
            panes: vec![PaneRef { book: "John".into(), chapter: 3 }, PaneRef { book: "Rom".into(), chapter: 8 }],
            active: 1,
        };
        save_to(&path, &cfg).unwrap();

        let (back, first) = load_from(&path);
        assert!(!first);
        assert_eq!(back, cfg);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn absurd_size_and_bad_mode_fall_back() {
        let path = std::env::temp_dir().join(format!("pure-cfg-bad-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"studyMode":"wat","bodySize":9000}"#).unwrap();
        let (cfg, first) = load_from(&path);
        assert!(!first); // a damaged file is not a fresh first run
        assert_eq!(cfg.mode, StudyMode::Simple);
        assert_eq!(cfg.body_size, Config::default().body_size);
        let _ = std::fs::remove_file(&path);
    }
}
