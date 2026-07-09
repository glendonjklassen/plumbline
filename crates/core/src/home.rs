//! Resolving the data *home* — the directory holding `data/kjv.jsonl`,
//! `data/strongs.json`, and the personal study folders (`weaves/`, `threads/`,
//! `tags/`).
//!
//! Ported from overlay's `Home.hs` (minus the SWORD hydration). Historically the
//! home was the process CWD, which only worked when launched from a source
//! checkout; a stranger running an installed binary got an empty reader. The
//! home now resolves once, in order:
//!
//!   1. `$PURE_STUDY_HOME` / `$OVERLAY_HOME` — explicit override (also how the
//!      test suite and tooling point the loaders somewhere specific);
//!   2. the CWD, when it looks like a working tree (has `data/kjv.jsonl`) — the
//!      run-in-place case, preserving the existing dev workflow;
//!   3. a data directory next to the executable (`<exe_dir>[/..]/data`), so a
//!      packaged app that ships its corpus beside the binary just works;
//!   4. `$XDG_DATA_HOME/pure-study` (Windows `%APPDATA%`, macOS Application
//!      Support) — the installed case.
//!
//! All paths are composed with [`Path::join`], never a hardcoded separator, so
//! this is correct on Windows and Unix alike.

use std::path::{Path, PathBuf};

/// Where a resolved home came from — for a friendly status line / diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeKind {
    /// `$PURE_STUDY_HOME` / `$OVERLAY_HOME`.
    Env,
    /// The CWD is a working tree; run in place.
    Tree,
    /// A `data/` directory shipped next to the executable.
    ExeDir,
    /// The per-user data directory for an installed app.
    DataDir,
}

impl HomeKind {
    pub fn label(self) -> &'static str {
        match self {
            HomeKind::Env => "environment override",
            HomeKind::Tree => "working tree",
            HomeKind::ExeDir => "app data (next to the executable)",
            HomeKind::DataDir => "user data directory",
        }
    }
}

/// The corpus marker file under a home: `<home>/data/kjv.jsonl`.
pub fn corpus_marker(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("data").join("kjv.jsonl")
}

/// Does this directory look like a pure-study/overlay home — i.e. does it hold
/// a hydrated `data/kjv.jsonl`?
pub fn looks_like_home(path: impl AsRef<Path>) -> bool {
    corpus_marker(path).is_file()
}

/// The per-user data directory for this app, per platform:
/// - Windows: `%APPDATA%\pure-study`
/// - macOS: `$HOME/Library/Application Support/pure-study`
/// - other Unix: `$XDG_DATA_HOME/pure-study` (else `$HOME/.local/share/pure-study`)
pub fn data_dir() -> Option<PathBuf> {
    let app = "pure-study";
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|b| Path::new(&b).join(app))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| Path::new(&h).join("Library").join("Application Support").join(app))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
            Some(Path::new(&xdg).join(app))
        } else {
            std::env::var_os("HOME").map(|h| Path::new(&h).join(".local").join("share").join(app))
        }
    }
}

/// Homes to probe next to the executable: `<exe_dir>/data/..`, and one level up
/// (a `bin/` layout), each treated as a home root.
fn exe_dir_homes() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.to_path_buf());
            if let Some(up) = dir.parent() {
                out.push(up.to_path_buf());
            }
        }
    }
    out
}

/// Resolve the data home. An explicit env override is honored verbatim (even if
/// it isn't hydrated yet, so the caller can surface a precise "hydrate here"
/// error); otherwise only a directory that actually looks like a home is
/// returned. `None` means nothing was found and no override was given — the
/// caller should show first-run / hydration guidance.
pub fn resolve_home() -> Option<(PathBuf, HomeKind)> {
    // 1. Explicit override (either spelling).
    for var in ["PURE_STUDY_HOME", "OVERLAY_HOME"] {
        if let Some(v) = std::env::var_os(var).filter(|s| !s.is_empty()) {
            return Some((PathBuf::from(v), HomeKind::Env));
        }
    }
    // 2. CWD if it's a working tree.
    if let Ok(cwd) = std::env::current_dir() {
        if looks_like_home(&cwd) {
            return Some((cwd, HomeKind::Tree));
        }
    }
    // 3. Next to the executable.
    for cand in exe_dir_homes() {
        if looks_like_home(&cand) {
            return Some((cand, HomeKind::ExeDir));
        }
    }
    // 4. The installed per-user data directory.
    if let Some(d) = data_dir() {
        if looks_like_home(&d) {
            return Some((d, HomeKind::DataDir));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_a_home_by_its_corpus_marker() {
        let home = std::env::temp_dir().join(format!("pure-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert!(!looks_like_home(&home));
        std::fs::create_dir_all(home.join("data")).unwrap();
        std::fs::write(corpus_marker(&home), "{}").unwrap();
        assert!(looks_like_home(&home));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn data_dir_is_named_for_the_app() {
        // Whatever the platform resolves to, it should end in the app folder.
        if let Some(d) = data_dir() {
            assert!(d.ends_with("pure-study"));
        }
    }
}
