//! Cross-platform atomic file writes for personal study data (threads, tags,
//! and later weaves).
//!
//! The pattern is portable: write the full contents to a **sibling temp file**
//! in the same directory, flush + fsync it, close it, then `fs::rename` it over
//! the target. `std::fs::rename` replaces an existing destination on Unix *and*
//! on Windows (it maps to `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`), and
//! a rename within one directory is atomic on both, so a concurrent reader
//! never sees a half-written file. Closing the temp handle before the rename is
//! required on Windows (an open handle would cause a sharing violation).
//!
//! All paths are composed with [`Path::join`] — never a hardcoded `/` — so the
//! same code produces correct paths on every platform.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Error;

/// Atomically write `contents` to `path`, creating parent directories as
/// needed. Portable across Unix and Windows.
pub fn write_atomic(path: impl AsRef<Path>, contents: &str) -> Result<(), Error> {
    write_atomic_bytes(path, contents.as_bytes())
}

/// Atomically write raw `bytes` to `path` (the binary sibling of
/// [`write_atomic`], for caches and other non-text artifacts). Same portable
/// temp-sibling → fsync → rename dance.
pub fn write_atomic_bytes(path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
        }
    }

    let tmp = temp_sibling(path);
    // Scope the handle so it is closed (dropped) before the rename — Windows
    // will not replace a file that still has an open handle.
    {
        let mut f = File::create(&tmp).map_err(|e| io_err(&tmp, e))?;
        f.write_all(bytes).map_err(|e| io_err(&tmp, e))?;
        f.sync_all().map_err(|e| io_err(&tmp, e))?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp); // best-effort cleanup on failure
        io_err(path, e)
    })
}

/// A hidden temp path next to `path`, unique per process, so a rename stays
/// within the same directory (and thus the same filesystem — required for an
/// atomic rename).
fn temp_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let tmp_name = format!(".{name}.{}.tmp", std::process::id());
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Slug a display name into a filename stem: lowercase, non-alphanumerics to
/// separators, words joined by `-`. Empty input falls back to `fallback`.
/// Matches overlay's `threadFileFor` / `tagFileFor` slugging.
pub fn slug(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let s = cleaned.split_whitespace().collect::<Vec<_>>().join("-");
    if s.is_empty() {
        fallback.to_string()
    } else {
        s
    }
}

fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.display().to_string(), source }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        // A unique-per-process scratch dir under the OS temp dir (portable).
        std::env::temp_dir().join(format!("pure-store-{}-{tag}", std::process::id()))
    }

    #[test]
    fn writes_creating_dirs_then_replaces() {
        let dir = scratch("atomic");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("thing.json");

        write_atomic(&path, "first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");

        // Overwrite in place; no leftover temp files.
        write_atomic(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        let leftovers: Vec<_> = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files should be gone");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn slugging() {
        assert_eq!(slug("Romans Road", "thread"), "romans-road");
        assert_eq!(slug("  A Priest, after the Order! ", "thread"), "a-priest-after-the-order");
        assert_eq!(slug("", "tag"), "tag");
        assert_eq!(slug("!!!", "tag"), "tag");
    }
}
