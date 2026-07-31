//! The 1769 KJV translators' margin notes (`kjv-notes.jsonl`).
//!
//! One JSON object per line — `{"b":OSIS,"c":ch,"v":vs,"note":text}` — with no
//! header (unlike the corpus). Ported from overlay `Config.loadNotes`: lines
//! that don't decode are skipped, a missing file yields no notes (they are
//! optional), and multiple notes on one verse keep their file order. The result
//! feeds `search::run_search` (notes are its last-searched tier) and the
//! reader's per-verse note display.

use std::path::Path;

use serde::Deserialize;

use crate::reference::VRef;
use crate::search::Notes;
use crate::Error;

#[derive(Deserialize)]
struct NoteRec {
    b: String,
    c: u16,
    v: u16,
    note: String,
}

/// Load notes from `kjv-notes.jsonl`. A missing file is **not** an error —
/// notes are optional — and yields an empty map; other I/O errors propagate.
pub fn load_notes(path: impl AsRef<Path>) -> Result<Notes, Error> {
    let path = path.as_ref();
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(notes_from_str(&raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Notes::new()),
        Err(e) => Err(Error::Io { path: path.display().to_string(), source: e }),
    }
}

/// Parse notes from an in-memory JSONL string. Lines that don't decode as a
/// note record are skipped; notes on the same verse keep their file order.
pub fn notes_from_str(raw: &str) -> Notes {
    let mut notes: Notes = Notes::new();
    for line in raw.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<NoteRec>(line) {
            notes.entry(VRef::new(rec.b, rec.c, rec.v)).or_default().push(rec.note);
        }
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        r#"{"b":"Gen","c":1,"v":4,"note":"first note on 1:4"}"#,
        "\n",
        r#"{"b":"Gen","c":1,"v":4,"note":"second note on 1:4"}"#,
        "\n",
        "\n",
        r#"garbage line, not json"#,
        "\n",
        r#"{"b":"John","c":3,"v":16,"note":"a note on John 3:16"}"#,
    );

    #[test]
    fn parses_groups_and_skips_bad_lines() {
        let notes = notes_from_str(SAMPLE);
        // Two verses have notes; the blank + garbage lines are skipped.
        assert_eq!(notes.len(), 2);
        // Multiple notes on one verse keep file order.
        let gen = notes.get(&VRef::new("Gen", 1, 4)).unwrap();
        assert_eq!(gen, &["first note on 1:4", "second note on 1:4"]);
        assert_eq!(notes.get(&VRef::new("John", 3, 16)).unwrap().len(), 1);
        assert!(!notes.contains_key(&VRef::new("Gen", 1, 5)));
    }

    #[test]
    fn missing_file_is_empty_not_error() {
        let notes = load_notes("/no/such/kjv-notes.jsonl").unwrap();
        assert!(notes.is_empty());
    }
}
