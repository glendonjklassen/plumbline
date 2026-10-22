//! Personal notes — the reader's own margin notes, distinct from the 1769
//! translators' notes in [`crate::notes`], which are read-only shipped data.
//!
//! **Any number of notes per verse.** One JSON file per note under
//! `home/notes/`, named `<slug of the refKey>-<id>.json` and written through the
//! atomic store like threads/tags/weaves. The refKey and the id are stored
//! *inside* the file, so loading is authoritative and the name is only a place.
//!
//! The id is the note's identity across devices: a restore merges by it
//! ([`crate::merge`]) — the same id is the same note, edited, and the newer edit
//! wins; different ids are different notes, and both are kept.
//!
//! **Breaking (v0.78.0):** notes were one per verse in `pure-note-v1`
//! (`notes/<slug>.json`, no id). Those files still load — each is one note, with
//! an id derived from its verse and creation time ([`legacy_id`]) so two devices
//! holding the same old note agree on its identity — and an edit rewrites it as
//! `plumbline-note-v2` at its new path. A build older than 0.78.0 refuses v2
//! files rather than misreading them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::reference::VRef;
use crate::Error;

/// The format every note is written in.
pub const FORMAT: &str = "plumbline-note-v2";

/// The one-note-per-verse format before 0.78.0. Read, never written. The tag
/// keeps its pre-rename name because it sits inside shipped backup zips.
pub const FORMAT_V1: &str = "pure-note-v1";

/// One personal note on a verse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserNote {
    /// 32 lowercase hex chars — see the module docs.
    pub id: String,
    pub vref: VRef,
    pub text: String,
    pub created: String,
    pub updated: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NoteRepr {
    pub(crate) format: String,
    /// Absent in v1 files; required in v2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<String>,
    #[serde(rename = "ref")]
    pub(crate) ref_key: String,
    pub(crate) text: String,
    pub(crate) created: String,
    #[serde(default)]
    pub(crate) updated: String,
    /// Every key in the file this build has never heard of, carried straight
    /// back out on save: the formats evolve additively, and a build that dropped
    /// a later one's fields would drop them for good. Serde fills this with the
    /// leftovers *after* the fields above are matched, so a known key can never
    /// be swallowed nor a promoted one written twice; an empty map writes nothing.
    #[serde(flatten)]
    pub(crate) extra: Map<String, Value>,
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

/// The file the note `id` on `vref` lives in: `home/notes/<slug>-<id>.json`.
pub fn note_file(home: impl AsRef<Path>, vref: &VRef, id: &str) -> PathBuf {
    notes_dir(home).join(note_file_name(vref, id))
}

/// Just the name of [`note_file`] — what the merge writes a note under.
pub(crate) fn note_file_name(vref: &VRef, id: &str) -> String {
    format!("{}-{id}.json", crate::store::slug(&vref.ref_key(), "note"))
}

/// The id of a v1 note, which has none on disk: derived from what identifies it
/// — its verse and its creation stamp — so every device holding a copy of the
/// same old note derives the same id, and a merge sees one note rather than two.
pub fn legacy_id(ref_key: &str, created: &str) -> String {
    let mut h = Sha256::new();
    h.update(FORMAT_V1.as_bytes());
    h.update(b"\n");
    h.update(ref_key.as_bytes());
    h.update(b"\n");
    h.update(created.as_bytes());
    h.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// Parse a note file's bytes into a note, accepting v1 and v2. The id of a v1
/// note is [`legacy_id`]. `Err` names what is wrong, without the path.
pub(crate) fn parse(bytes: &[u8]) -> Result<(UserNote, NoteRepr), String> {
    let r: NoteRepr = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let id = match r.format.as_str() {
        FORMAT => match &r.id {
            Some(id) if !id.is_empty() => id.clone(),
            _ => return Err("a v2 note with no id".to_string()),
        },
        FORMAT_V1 => legacy_id(&r.ref_key, &r.created),
        other => return Err(format!("unknown note format {other}")),
    };
    let vref = VRef::parse_ref_key(&r.ref_key).ok_or_else(|| format!("bad ref {}", r.ref_key))?;
    let note = UserNote { id, vref, text: r.text.clone(), created: r.created.clone(), updated: r.updated.clone() };
    Ok((note, r))
}

/// Load every `home/notes/*.json`, in reading order (then oldest first), plus
/// any read/parse errors. A missing directory yields no notes. Two files with
/// one id — an edit that moved a v1 file, and a copy of the old one restored
/// beside it — keep the newer, in memory only (`store::resolve_duplicate_ids`).
pub fn load_notes(home: impl AsRef<Path>) -> (Vec<LoadedNote>, Vec<String>) {
    let dir = notes_dir(home);
    let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => {
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect()
        }
        Err(_) => Vec::new(),
    };
    files.sort();
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for path in files {
        match std::fs::read(&path) {
            Err(e) => errors.push(format!("{}: {e}", path.display())),
            Ok(bytes) => match parse(&bytes) {
                Ok((note, _)) => out.push(LoadedNote { file: path, note }),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    let mut out =
        crate::store::resolve_duplicate_ids(out, |n| Some(n.note.id.as_str()), |n| Some(n.note.updated.as_str()));
    out.sort_by(|a, b| {
        (a.note.vref.reading_key(), &a.note.created, &a.note.id).cmp(&(
            b.note.vref.reading_key(),
            &b.note.created,
            &b.note.id,
        ))
    });
    (out, errors)
}

/// The notes on one verse, oldest first.
pub fn notes_on<'a>(loaded: &'a [LoadedNote], vref: &VRef) -> Vec<&'a LoadedNote> {
    loaded.iter().filter(|n| &n.note.vref == vref).collect()
}

fn write_repr(path: &Path, repr: &NoteRepr) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(repr).map(|s| s + "\n").map_err(|e| Error::Parse(e.to_string()))?;
    crate::store::write_atomic(path, &json)
}

/// Add a new note on `vref`. `id` is the caller's fresh identity
/// (`store::new_id`); `stamp` is a UTC timestamp, used for both `created` and
/// `updated`. Empty or whitespace text is refused — there is nothing to add.
pub fn add_note(home: impl AsRef<Path>, vref: &VRef, text: &str, id: &str, stamp: &str) -> Result<PathBuf, Error> {
    if text.trim().is_empty() {
        return Err(Error::Corpus("a note needs some text".to_string()));
    }
    let path = note_file(&home, vref, id);
    let repr = NoteRepr {
        format: FORMAT.to_string(),
        id: Some(id.to_string()),
        ref_key: vref.ref_key(),
        text: text.to_string(),
        created: stamp.to_string(),
        updated: stamp.to_string(),
        // A new note is stamped with the language its ref was written in
        // (`i18n::stamp`).
        extra: crate::i18n::stamped_extra(),
    };
    write_repr(&path, &repr)?;
    Ok(path)
}

/// Replace the text of note `id`, atomically. Empty or whitespace text
/// **deletes** the note (the editor's "leave empty to remove"), answering
/// `Ok(None)`. `created`, the language stamp — or its absence — and every
/// unknown key are kept. A v1 note is rewritten as v2 at its new path and the
/// old file removed, so the verse holds it once.
pub fn edit_note(
    home: impl AsRef<Path>,
    loaded: &[LoadedNote],
    id: &str,
    text: &str,
    stamp: &str,
) -> Result<Option<PathBuf>, Error> {
    let Some(ln) = loaded.iter().find(|n| n.note.id == id) else {
        return Err(Error::Corpus(format!("no note with id {id}")));
    };
    if text.trim().is_empty() {
        delete_file(&ln.file)?;
        return Ok(None);
    }
    // The file being replaced is the only place its unknown keys can come from.
    // It loaded a moment ago, so it parses; if it no longer does, the bytes go
    // aside as `.bad` before the edit lands rather than being lost to it.
    let existing = match std::fs::read(&ln.file) {
        Ok(bytes) => match parse(&bytes) {
            Ok((_, r)) => Some(r),
            Err(_) => {
                crate::store::move_damaged_aside(&ln.file, &bytes);
                None
            }
        },
        Err(_) => None,
    };
    let path = note_file(&home, &ln.note.vref, id);
    let created = Some(ln.note.created.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| stamp.to_string());
    let repr = NoteRepr {
        format: FORMAT.to_string(),
        id: Some(id.to_string()),
        ref_key: ln.note.vref.ref_key(),
        text: text.to_string(),
        created,
        updated: stamp.to_string(),
        // Re-saving must not invent a provenance the file never had.
        extra: existing.map(|r| r.extra).unwrap_or_default(),
    };
    write_repr(&path, &repr)?;
    if ln.file != path {
        delete_file(&ln.file)?;
    }
    Ok(Some(path))
}

/// Delete note `id`. `Ok(false)` when no such note is loaded. Deleting is not
/// writing: whatever shape the file is in, the reader asked for it to be gone.
pub fn delete_note(loaded: &[LoadedNote], id: &str) -> Result<bool, Error> {
    let Some(ln) = loaded.iter().find(|n| n.note.id == id) else { return Ok(false) };
    delete_file(&ln.file)?;
    Ok(true)
}

fn delete_file(path: &Path) -> Result<(), Error> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("plumbline-usernote-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    const ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn a_verse_holds_any_number_of_notes() {
        let home = scratch("many");
        let v = VRef::new("John", 3, 16);
        assert!(load_notes(&home).0.is_empty());

        add_note(&home, &v, "the golden text", ID_A, "2026-01-01T00:00:00Z").unwrap();
        add_note(&home, &v, "preached on, Easter", ID_B, "2026-02-01T00:00:00Z").unwrap();
        add_note(&home, &VRef::new("Gen", 1, 1), "in the beginning", "c".repeat(32).as_str(), "2026-03-01T00:00:00Z")
            .unwrap();

        let (notes, errs) = load_notes(&home);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(notes.len(), 3);
        // Reading order first, so Genesis leads; then oldest first on a verse.
        assert_eq!(notes[0].note.vref, VRef::new("Gen", 1, 1));
        let on = notes_on(&notes, &v);
        assert_eq!(
            on.iter().map(|n| n.note.text.as_str()).collect::<Vec<_>>(),
            ["the golden text", "preached on, Easter"]
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_edit_changes_one_note_and_keeps_created() {
        let home = scratch("edit");
        let v = VRef::new("John", 3, 16);
        add_note(&home, &v, "first", ID_A, "2026-01-01T00:00:00Z").unwrap();
        add_note(&home, &v, "second", ID_B, "2026-01-02T00:00:00Z").unwrap();
        let (notes, _) = load_notes(&home);
        edit_note(&home, &notes, ID_A, "first, revised", "2026-02-02T00:00:00Z").unwrap();
        let (notes, _) = load_notes(&home);
        let a = notes.iter().find(|n| n.note.id == ID_A).unwrap();
        assert_eq!(a.note.text, "first, revised");
        assert_eq!(a.note.created, "2026-01-01T00:00:00Z");
        assert_eq!(a.note.updated, "2026-02-02T00:00:00Z");
        assert_eq!(notes.iter().find(|n| n.note.id == ID_B).unwrap().note.text, "second", "the other note moved");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn emptying_or_deleting_removes_only_that_note() {
        let home = scratch("delete");
        let v = VRef::new("John", 3, 16);
        add_note(&home, &v, "first", ID_A, "t1").unwrap();
        add_note(&home, &v, "second", ID_B, "t2").unwrap();
        let (notes, _) = load_notes(&home);
        assert_eq!(edit_note(&home, &notes, ID_A, "   ", "t3").unwrap(), None);
        let (notes, _) = load_notes(&home);
        assert_eq!(notes.len(), 1);
        assert!(delete_note(&notes, ID_B).unwrap());
        assert!(load_notes(&home).0.is_empty());
        assert!(!delete_note(&[], ID_B).unwrap(), "deleting nothing is not an error");
        assert!(add_note(&home, &v, "  ", ID_A, "t").is_err(), "an empty note is not added");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The format every note is written in, byte for byte: these files ship
    /// inside backup zips.
    #[test]
    fn a_new_note_is_written_exactly_so() {
        let home = scratch("golden");
        let v = VRef::new("John", 3, 16);
        let path = add_note(&home, &v, "the golden text", ID_A, "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(path.file_name().unwrap(), format!("john-3-16-{ID_A}.json").as_str());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!(
                r#"{{
  "format": "plumbline-note-v2",
  "id": "{ID_A}",
  "ref": "John 3:16",
  "text": "the golden text",
  "created": "2026-01-01T00:00:00Z",
  "updated": "2026-01-01T00:00:00Z",
  "lang": "en"
}}
"#
            )
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A v1 note — the only kind before 0.78.0, one per verse, no id — loads as
    /// one note whose id every device derives alike, and an edit moves it to v2.
    #[test]
    fn a_v1_note_loads_with_a_shared_id_and_an_edit_moves_it_to_v2() {
        let home = scratch("legacy");
        let dir = home.join("notes");
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("john-3-16.json");
        std::fs::write(
            &old,
            r#"{"format":"pure-note-v1","ref":"John 3:16","text":"older","created":"2025-01-01T00:00:00Z","updated":"2025-01-01T00:00:00Z","mood":"grateful"}"#,
        )
        .unwrap();
        let (notes, errs) = load_notes(&home);
        assert!(errs.is_empty(), "{errs:?}");
        let id = legacy_id("John 3:16", "2025-01-01T00:00:00Z");
        assert_eq!(notes[0].note.id, id, "the id is derived, so two devices agree on it");
        assert_eq!(id.len(), 32);
        assert_ne!(id, legacy_id("John 3:16", "2025-01-02T00:00:00Z"), "a different note is a different id");

        let new_path = edit_note(&home, &notes, &id, "edited", "2026-08-03T00:00:00Z").unwrap().unwrap();
        assert!(!old.exists(), "the v1 file must go, or the verse would show the note twice");
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&new_path).unwrap()).unwrap();
        assert_eq!(back["format"], FORMAT);
        assert_eq!(back["id"], id.as_str());
        assert_eq!(back["created"], "2025-01-01T00:00:00Z");
        assert_eq!(back["mood"], "grateful", "an unknown key was stripped");
        assert!(back.get("lang").is_none(), "a re-save invented a provenance the file never had");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Unreadable bytes are reported, not silently treated as "no note".
    #[test]
    fn unreadable_and_foreign_notes_are_reported_rather_than_dropped() {
        let home = scratch("malformed");
        let dir = home.join("notes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("truncated.json"), r#"{"format":"plumbline-note-v2","ref":"John 3:16","text":"half"#)
            .unwrap();
        std::fs::write(
            dir.join("later.json"),
            r#"{"format":"plumbline-note-v3","id":"x","ref":"John 3:17","text":"x","created":"t","updated":"t"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("badref.json"),
            r#"{"format":"pure-note-v1","ref":"John","text":"x","created":"t","updated":"t"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("noid.json"),
            r#"{"format":"plumbline-note-v2","ref":"John 3:18","text":"x","created":"t"}"#,
        )
        .unwrap();
        add_note(&home, &VRef::new("Gen", 1, 1), "in the beginning", ID_A, "2026-01-01T00:00:00Z").unwrap();

        let (notes, errs) = load_notes(&home);
        assert_eq!(notes.len(), 1, "the readable note should still load");
        assert_eq!(errs.len(), 4, "every unreadable file should be named: {errs:?}");
        let all = errs.join(" | ");
        assert!(all.contains("truncated.json"), "{all}");
        assert!(all.contains("unknown note format plumbline-note-v3"), "{all}");
        assert!(all.contains("bad ref John"), "{all}");
        assert!(all.contains("no id"), "{all}");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Two files, one id: the copy of a v1 note restored beside the v2 file its
    /// edit became. One note shows, the newer.
    #[test]
    fn one_id_in_two_files_shows_once_and_the_newer_wins() {
        let home = scratch("dupe");
        let dir = home.join("notes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("john-3-16.json"),
            r#"{"format":"pure-note-v1","ref":"John 3:16","text":"old","created":"2025-01-01T00:00:00Z","updated":"2025-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        let (notes, _) = load_notes(&home);
        let id = notes[0].note.id.clone();
        // Write the v2 copy by hand, leaving the v1 file in place.
        let v2 = note_file(&home, &VRef::new("John", 3, 16), &id);
        std::fs::write(
            &v2,
            format!(r#"{{"format":"plumbline-note-v2","id":"{id}","ref":"John 3:16","text":"new","created":"2025-01-01T00:00:00Z","updated":"2026-01-01T00:00:00Z"}}"#),
        )
        .unwrap();
        let (notes, errs) = load_notes(&home);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note.text, "new");
        let _ = std::fs::remove_dir_all(&home);
    }
}
