//! Personal per-verse notes — the reader's own margin notes, distinct from the
//! 1769 translators' notes in [`crate::notes`] (which are read-only shipped
//! data). Tier 0 #3.
//!
//! Personal study data, so it lives under the data home like threads/tags/weaves
//! and writes through the same cross-platform atomic store: one JSON file per
//! annotated verse under `home/notes/`, named by a slug of the refKey. The
//! refKey is stored *inside* the file (the filename is only a slug), so loading
//! is authoritative and a note survives a book being renamed in display.
//!
//! Notes are the anchor of the future sync service, so the on-disk shape is a
//! plain, additive-friendly object with a `format` stamp.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::reference::VRef;
use crate::Error;

const FORMAT: &str = "pure-note-v1";

/// One personal note on a verse: the text plus create/update timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserNote {
    pub vref: VRef,
    pub text: String,
    pub created: String,
    pub updated: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NoteRepr {
    format: String,
    #[serde(rename = "ref")]
    ref_key: String,
    text: String,
    created: String,
    #[serde(default)]
    updated: String,
}

/// A note plus the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedNote {
    pub file: PathBuf,
    pub note: UserNote,
}

/// The `home/notes` directory.
fn notes_dir(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join("notes")
}

/// The file a note on `vref` lives in (`home/notes/<slug>.json`).
pub fn note_file(home: impl AsRef<Path>, vref: &VRef) -> PathBuf {
    notes_dir(home).join(format!("{}.json", crate::store::slug(&vref.ref_key(), "note")))
}

/// Load every `home/notes/*.json`, returning the notes keyed by verse (for the
/// reader's gutter marks + panel section) and any read/parse errors. A missing
/// directory yields no notes.
pub fn load_notes(home: impl AsRef<Path>) -> (HashMap<VRef, LoadedNote>, Vec<String>) {
    let dir = notes_dir(home);
    let files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect(),
        Err(_) => Vec::new(),
    };
    let mut out = HashMap::new();
    let mut errors = Vec::new();
    for path in files {
        match std::fs::read(&path) {
            Err(e) => errors.push(format!("{}: {e}", path.display())),
            Ok(bytes) => match serde_json::from_slice::<NoteRepr>(&bytes) {
                Ok(r) if r.format == FORMAT => match VRef::parse_ref_key(&r.ref_key) {
                    Some(vref) => {
                        let note = UserNote {
                            vref: vref.clone(),
                            text: r.text,
                            created: r.created,
                            updated: r.updated,
                        };
                        out.insert(vref, LoadedNote { file: path, note });
                    }
                    None => errors.push(format!("{}: bad ref {}", path.display(), r.ref_key)),
                },
                Ok(r) => errors.push(format!("{}: unknown note format {}", path.display(), r.format)),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    (out, errors)
}

/// Set the note on `vref` to `text`, atomically. An empty/whitespace `text`
/// **removes** the note (so the gutter clears when you delete all the words).
/// `stamp` is a caller-supplied UTC timestamp; `created` is preserved across
/// edits when a note already exists. Returns the file written (or `None` when
/// the note was removed).
pub fn set_note(
    home: impl AsRef<Path>,
    vref: &VRef,
    text: &str,
    stamp: &str,
) -> Result<Option<PathBuf>, Error> {
    let path = note_file(&home, vref);
    if text.trim().is_empty() {
        // Delete on empty; a missing file is fine.
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
        }
    } else {
        // Preserve `created` if a note is already there.
        let created = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<NoteRepr>(&b).ok())
            .map(|r| r.created)
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| stamp.to_string());
        let repr = NoteRepr {
            format: FORMAT.to_string(),
            ref_key: vref.ref_key(),
            text: text.to_string(),
            created,
            updated: stamp.to_string(),
        };
        let json = serde_json::to_string_pretty(&repr)
            .map(|s| s + "\n")
            .map_err(|e| Error::Parse(e.to_string()))?;
        crate::store::write_atomic(&path, &json)?;
        Ok(Some(path))
    }
}

/// Remove the note on `vref` (a no-op if none). Convenience over `set_note` with
/// an empty string.
pub fn remove_note(home: impl AsRef<Path>, vref: &VRef) -> Result<(), Error> {
    set_note(home, vref, "", "").map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("plumbline-usernote-{}-{tag}", std::process::id()))
    }

    #[test]
    fn set_load_update_and_remove() {
        let home = scratch("crud");
        let _ = std::fs::remove_dir_all(&home);
        let v = VRef::new("John", 3, 16);

        // No notes yet.
        assert!(load_notes(&home).0.is_empty());

        // Create.
        set_note(&home, &v, "the golden text", "2026-01-01T00:00:00Z").unwrap();
        let (notes, errs) = load_notes(&home);
        assert!(errs.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[&v].note.text, "the golden text");
        assert_eq!(notes[&v].note.created, "2026-01-01T00:00:00Z");

        // Update preserves `created`, advances `updated`.
        set_note(&home, &v, "God's love for the world", "2026-02-02T00:00:00Z").unwrap();
        let (notes, _) = load_notes(&home);
        assert_eq!(notes[&v].note.text, "God's love for the world");
        assert_eq!(notes[&v].note.created, "2026-01-01T00:00:00Z");
        assert_eq!(notes[&v].note.updated, "2026-02-02T00:00:00Z");

        // Empty text removes it.
        set_note(&home, &v, "   ", "2026-03-03T00:00:00Z").unwrap();
        assert!(load_notes(&home).0.is_empty());

        // remove_note on an absent note is a no-op.
        remove_note(&home, &v).unwrap();

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn distinct_verses_get_distinct_files() {
        let home = scratch("distinct");
        let _ = std::fs::remove_dir_all(&home);
        set_note(&home, &VRef::new("Gen", 1, 7), "a", "t").unwrap();
        set_note(&home, &VRef::new("Gen", 17, 1), "b", "t").unwrap();
        let (notes, _) = load_notes(&home);
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[&VRef::new("Gen", 1, 7)].note.text, "a");
        assert_eq!(notes[&VRef::new("Gen", 17, 1)].note.text, "b");
        let _ = std::fs::remove_dir_all(&home);
    }
}
