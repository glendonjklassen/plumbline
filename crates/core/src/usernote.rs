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
use serde_json::{Map, Value};

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
    /// Every key in the file this build has never heard of, carried straight
    /// back out again on save.
    ///
    /// The on-disk formats evolve **additively** (CLAUDE.md §Data formats), and
    /// a sideloaded APK never auto-updates: a build that drops the fields of a
    /// later one drops them for good on that device. So the reader's file, not
    /// this struct, decides what a note contains. Nothing has to be skipped
    /// when it is empty — a flattened map with no entries writes no key at all,
    /// so a note written here reads byte for byte as it always did.
    ///
    /// Serde fills this with the leftovers *after* the fields above are
    /// matched, so a known key can never be swallowed, and a key a later
    /// version promotes to a real field stops arriving here the moment that
    /// field exists — it can never be written twice.
    #[serde(flatten)]
    extra: Map<String, Value>,
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
        // Preserve `created` — and anything else the file carries that we do not
        // understand (see [`NoteRepr::extra`]) — if a note is already there. A
        // note is edited through its text, not by handing this function a
        // `UserNote`, so the file we are replacing is the only place those keys
        // can come from.
        //
        // Two ways that file can be something this build must not simply
        // overwrite, and they need opposite answers:
        let on_disk = std::fs::read(&path).ok();
        let existing = match &on_disk {
            None => None,
            Some(bytes) => match serde_json::from_slice::<NoteRepr>(bytes) {
                // A FOREIGN format stamp — a `pure-note-v2` from a later build,
                // say. It parsed, so it is not damaged; it is a file whose meaning
                // we do not know. Refuse, the way `thread.rs` refuses to clobber a
                // thread it could not load, rather than rewrite it as v1 and take
                // the stamp off. Both shells surface a note-save error.
                Ok(r) if r.format != FORMAT => {
                    return Err(Error::Corpus(format!(
                        "{} is a {} note, which this build does not understand — refusing to overwrite it",
                        path.display(),
                        r.format
                    )))
                }
                Ok(r) => Some(r),
                // UNPARSEABLE bytes: there is nothing in them to keep and nothing
                // the reader can read, so the note they are writing now must land.
                // Their old bytes go aside as `.bad` first — the same rescue a
                // damaged config gets — so a truncated write is recoverable by hand
                // instead of gone.
                Err(_) => {
                    crate::store::move_damaged_aside(&path, bytes);
                    None
                }
            },
        };
        let created = existing
            .as_ref()
            .map(|r| r.created.clone())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| stamp.to_string());
        let repr = NoteRepr {
            format: FORMAT.to_string(),
            ref_key: vref.ref_key(),
            text: text.to_string(),
            created,
            updated: stamp.to_string(),
            extra: existing.map(|r| r.extra).unwrap_or_default(),
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

    /// AUDIT 2026-07-29 forward compatibility: the on-disk formats evolve
    /// **additively** (CLAUDE.md §Data formats), and a sideloaded APK never
    /// auto-updates — so a key this build drops is dropped for good on that
    /// device. Editing a note written by a later build must keep every one.
    #[test]
    fn a_note_keeps_the_keys_of_a_later_build() {
        let home = scratch("forward");
        let _ = std::fs::remove_dir_all(&home);
        let v = VRef::new("John", 3, 16);
        let path = note_file(&home, &v);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"format":"pure-note-v1","ref":"John 3:16","text":"first draft",
                "created":"2026-01-01T00:00:00Z","updated":"2026-01-01T00:00:00Z",
                "mood":"grateful","voice":{"lang":"en","clip":"jn3-16.ogg"},
                "linkedTo":["Rom 5:8","1John 4:9"]}"#,
        )
        .unwrap();

        set_note(&home, &v, "God's love for the world", "2026-02-02T00:00:00Z").unwrap();
        let back: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["text"], "God's love for the world", "the edit itself must land");
        assert_eq!(back["created"], "2026-01-01T00:00:00Z");
        assert_eq!(back["updated"], "2026-02-02T00:00:00Z");
        assert_eq!(back["mood"], "grateful", "an unknown scalar was stripped");
        assert_eq!(
            back["voice"],
            serde_json::json!({"lang":"en","clip":"jn3-16.ogg"}),
            "an unknown object was stripped"
        );
        assert_eq!(
            back["linkedTo"],
            serde_json::json!(["Rom 5:8", "1John 4:9"]),
            "an unknown array was stripped"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A note with nothing unknown in it is written byte for byte as it was
    /// before any of that landed — these files already ship inside backup zips.
    #[test]
    fn a_note_with_no_unknown_keys_is_written_exactly_as_before() {
        let home = scratch("golden");
        let _ = std::fs::remove_dir_all(&home);
        let v = VRef::new("John", 3, 16);
        set_note(&home, &v, "the golden text", "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(
            std::fs::read_to_string(note_file(&home, &v)).unwrap(),
            r#"{
  "format": "pure-note-v1",
  "ref": "John 3:16",
  "text": "the golden text",
  "created": "2026-01-01T00:00:00Z",
  "updated": "2026-01-01T00:00:00Z"
}
"#
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    // ── malformed and foreign files (TODO §I) ────────────────────────────────

    /// Unreadable bytes are reported, not silently treated as "no note". A
    /// loader that swallowed them would show the reader an empty margin and then
    /// overwrite whatever was really there.
    #[test]
    fn unreadable_and_foreign_notes_are_reported_rather_than_dropped() {
        let home = scratch("malformed");
        let _ = std::fs::remove_dir_all(&home);
        let dir = home.join("notes");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("truncated.json"), r#"{"format":"pure-note-v1","ref":"John 3:16","text":"half"#).unwrap();
        std::fs::write(dir.join("later.json"), r#"{"format":"pure-note-v2","ref":"John 3:17","text":"x","created":"t","updated":"t"}"#).unwrap();
        // No chapter:verse at all. Note the frozen parser does NOT check the
        // book against the canon (`VRef::parse_ref_key` splits on the last space
        // and the colon, nothing more), so "Nowhere 3:16" would load as a note on
        // a book that does not exist — harmless, since it can never match a verse
        // being read, and not this test's business.
        std::fs::write(dir.join("badref.json"), r#"{"format":"pure-note-v1","ref":"John","text":"x","created":"t","updated":"t"}"#).unwrap();
        // And one good one beside them: three bad files must not cost the reader
        // the note that is fine.
        set_note(&home, &VRef::new("Gen", 1, 1), "in the beginning", "2026-01-01T00:00:00Z").unwrap();

        let (notes, errs) = load_notes(&home);
        assert_eq!(notes.len(), 1, "the readable note should still load");
        assert_eq!(notes[&VRef::new("Gen", 1, 1)].note.text, "in the beginning");
        assert_eq!(errs.len(), 3, "every unreadable file should be named: {errs:?}");
        let all = errs.join(" | ");
        assert!(all.contains("truncated.json"), "{all}");
        assert!(all.contains("unknown note format pure-note-v2"), "{all}");
        assert!(all.contains("bad ref John"), "{all}");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A note whose bytes are damaged is set aside as `.bad` and the note the
    /// reader is writing now lands. Nothing else can recover those bytes — the
    /// text is not on screen, because it could not be loaded — so the alternative
    /// is losing them silently.
    #[test]
    fn a_damaged_note_is_moved_aside_and_the_new_one_lands() {
        let home = scratch("damaged");
        let _ = std::fs::remove_dir_all(&home);
        let v = VRef::new("John", 3, 16);
        let path = note_file(&home, &v);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let damaged = r#"{"format":"pure-note-v1","ref":"John 3:16","text":"the words I typed"#;
        std::fs::write(&path, damaged).unwrap();

        set_note(&home, &v, "written again", "2026-02-02T00:00:00Z").unwrap();

        let (notes, errs) = load_notes(&home);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(notes[&v].note.text, "written again", "the new note did not land");
        let bad = path.with_file_name(format!("{}.bad", path.file_name().unwrap().to_string_lossy()));
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), damaged, "the damaged bytes were not kept");
        // `created` restarts, because the file that knew it was unreadable.
        assert_eq!(notes[&v].note.created, "2026-02-02T00:00:00Z");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A note written by a LATER format is refused, not rewritten. It parsed, so
    /// it is not damaged — it is a file whose meaning this build does not know,
    /// and a sideloaded APK that quietly restamped it as v1 would have taken the
    /// stamp off for good.
    #[test]
    fn a_note_from_a_later_format_is_refused_rather_than_rewritten() {
        let home = scratch("foreign");
        let _ = std::fs::remove_dir_all(&home);
        let v = VRef::new("John", 3, 16);
        let path = note_file(&home, &v);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let later = r#"{"format":"pure-note-v2","ref":"John 3:16","text":"from a later build","created":"2026-01-01T00:00:00Z","updated":"2026-01-01T00:00:00Z","segments":[{"at":0,"len":3}]}"#;
        std::fs::write(&path, later).unwrap();

        let err = set_note(&home, &v, "overwritten", "2026-02-02T00:00:00Z").unwrap_err();
        assert!(format!("{err}").contains("pure-note-v2"), "the error should name the format: {err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), later, "the later build's note was overwritten");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Deleting is not writing: the reader asked for the note to be gone, and a
    /// file this build cannot read is not a reason to keep it against their word.
    #[test]
    fn removing_a_note_works_whatever_shape_the_file_is_in() {
        let home = scratch("remove-foreign");
        let _ = std::fs::remove_dir_all(&home);
        let v = VRef::new("John", 3, 16);
        let path = note_file(&home, &v);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"format":"pure-note-v2","ref":"John 3:16","text":"x","created":"t","updated":"t"}"#).unwrap();

        remove_note(&home, &v).unwrap();
        assert!(!path.exists(), "the reader asked for it to be gone");

        let _ = std::fs::remove_dir_all(&home);
    }
}
