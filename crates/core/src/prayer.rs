//! The prayer list — what the reader is praying for, and what was answered.
//!
//! One JSON file per prayer under `home/prayers/`, named `<id>.json` and written
//! through the atomic store like notes. The id is the prayer's identity across
//! devices: a restore merges by it ([`crate::merge`]) — the newer edit wins, and
//! the days prayed on are a union of both sides.
//!
//! A prayer has a title, the days the reader marked it prayed for, and — once
//! answered — the day and an optional comment. It stays until the reader
//! deletes it; there is no expiry.
//!
//! Days are the reader's own calendar days (`YYYY-MM-DD`), handed in by the
//! shell; `created` and `updated` are UTC stamps. Marking a day prayed does not
//! touch `updated` — it is not an edit, and the merge unions the days anyway.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Error;

/// The format every prayer is written in. Frozen: these files ship inside
/// backup zips.
pub const FORMAT: &str = "plumbline-prayer-v1";

/// The directory under the home, and the prefix of every prayer's
/// home-relative path.
pub const DIR: &str = "prayers";

/// One prayer, as it is on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prayer {
    pub format: String,
    /// 32 lowercase hex chars (`store::new_id`).
    pub id: String,
    pub title: String,
    pub created: String,
    #[serde(default)]
    pub updated: String,
    /// The days it was prayed for, ascending, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prayed: Vec<String>,
    /// The day it was marked answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<String>,
    /// The reader's comment on the answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// Keys this build has never heard of, carried back out on save (the
    /// formats evolve additively — see `usernote::NoteRepr::extra`).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A prayer plus the file it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedPrayer {
    pub file: PathBuf,
    pub prayer: Prayer,
}

fn dir(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join(DIR)
}

/// The file prayer `id` lives in: `home/prayers/<id>.json`.
pub fn prayer_file(home: impl AsRef<Path>, id: &str) -> PathBuf {
    dir(home).join(format!("{id}.json"))
}

/// Parse a prayer file's bytes. `Err` names what is wrong, without the path.
pub fn parse(bytes: &[u8]) -> Result<Prayer, String> {
    let p: Prayer = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if p.format != FORMAT {
        return Err(format!("unknown prayer format {}", p.format));
    }
    if p.id.is_empty() {
        return Err("a prayer with no id".to_string());
    }
    Ok(p)
}

/// Load every `home/prayers/*.json`, oldest first, plus any read/parse errors.
/// A missing directory yields no prayers. Two files with one id keep the newer,
/// in memory only (`store::resolve_duplicate_ids`).
pub fn load(home: impl AsRef<Path>) -> (Vec<LoadedPrayer>, Vec<String>) {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir(home)) {
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
                Ok(prayer) => out.push(LoadedPrayer { file: path, prayer }),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    let mut out =
        crate::store::resolve_duplicate_ids(out, |p| Some(p.prayer.id.as_str()), |p| Some(p.prayer.updated.as_str()));
    out.sort_by(|a, b| (&a.prayer.created, &a.prayer.id).cmp(&(&b.prayer.created, &b.prayer.id)));
    (out, errors)
}

fn write(path: &Path, prayer: &Prayer) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(prayer).map(|s| s + "\n").map_err(|e| Error::Parse(e.to_string()))?;
    crate::store::write_atomic(path, &json)
}

/// A day argument: `YYYY-MM-DD`, a real date.
fn day(value: &str) -> Result<String, Error> {
    let v = value.trim();
    // Round-tripped, because `date_to_days` takes any three numbers: month 13
    // would otherwise be stored and then sorted as a date.
    if crate::civil::date_to_days(v).is_some_and(|d| crate::civil::days_to_date(d) == v) {
        Ok(v.to_string())
    } else {
        Err(Error::Corpus(format!("not a date: {value}")))
    }
}

fn title_of(title: &str) -> Result<String, Error> {
    let t = title.trim();
    if t.is_empty() {
        return Err(Error::Corpus("a prayer needs a title".to_string()));
    }
    Ok(t.to_string())
}

/// Add a prayer. `id` is the caller's fresh identity (`store::new_id`), `stamp`
/// a UTC timestamp. An empty title is refused.
pub fn add(home: impl AsRef<Path>, title: &str, id: &str, stamp: &str) -> Result<PathBuf, Error> {
    let prayer = Prayer {
        format: FORMAT.to_string(),
        id: id.to_string(),
        title: title_of(title)?,
        created: stamp.to_string(),
        updated: stamp.to_string(),
        prayed: Vec::new(),
        answered: None,
        answer: None,
        extra: crate::i18n::stamped_extra(),
    };
    let path = prayer_file(&home, id);
    write(&path, &prayer)?;
    Ok(path)
}

/// Load prayer `id`, change it, and write it back where it was found.
fn change(home: impl AsRef<Path>, id: &str, f: impl FnOnce(&mut Prayer) -> Result<(), Error>) -> Result<(), Error> {
    let (all, _) = load(&home);
    let Some(lp) = all.into_iter().find(|p| p.prayer.id == id) else {
        return Err(Error::Corpus(format!("no prayer with id {id}")));
    };
    let mut prayer = lp.prayer;
    f(&mut prayer)?;
    write(&lp.file, &prayer)
}

/// Replace a prayer's title.
pub fn edit(home: impl AsRef<Path>, id: &str, title: &str, stamp: &str) -> Result<(), Error> {
    let title = title_of(title)?;
    change(home, id, |p| {
        p.title = title;
        p.updated = stamp.to_string();
        Ok(())
    })
}

/// Record that prayer `id` was prayed for on `on` (`YYYY-MM-DD`). A day already
/// recorded is left as it is.
pub fn mark_prayed(home: impl AsRef<Path>, id: &str, on: &str) -> Result<(), Error> {
    let on = day(on)?;
    change(home, id, |p| {
        if !p.prayed.contains(&on) {
            p.prayed.push(on);
            p.prayed.sort();
        }
        Ok(())
    })
}

/// Mark prayer `id` answered on `on`, with an optional comment (`None` or empty
/// is no comment). Answering an answered prayer replaces the day and comment.
pub fn answer(home: impl AsRef<Path>, id: &str, comment: Option<&str>, on: &str, stamp: &str) -> Result<(), Error> {
    let on = day(on)?;
    let comment = comment.map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
    change(home, id, |p| {
        p.answered = Some(on);
        p.answer = comment;
        p.updated = stamp.to_string();
        Ok(())
    })
}

/// Move an answered prayer back to the list, dropping the day and the comment.
pub fn reopen(home: impl AsRef<Path>, id: &str, stamp: &str) -> Result<(), Error> {
    change(home, id, |p| {
        p.answered = None;
        p.answer = None;
        p.updated = stamp.to_string();
        Ok(())
    })
}

/// Delete prayer `id`. Answers the deleted prayer's `updated` stamp — what the
/// caller records the deletion after — or `None` when there was no such prayer.
pub fn delete(home: impl AsRef<Path>, id: &str) -> Result<Option<String>, Error> {
    let (all, _) = load(&home);
    let Some(lp) = all.into_iter().find(|p| p.prayer.id == id) else { return Ok(None) };
    match std::fs::remove_file(&lp.file) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io { path: lp.file.display().to_string(), source: e }),
    }
    Ok(Some(lp.prayer.updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("plumbline-prayer-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    const ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn one(home: &Path, id: &str) -> Prayer {
        load(home).0.into_iter().find(|p| p.prayer.id == id).unwrap().prayer
    }

    /// The format every prayer is written in, byte for byte: these files ship
    /// inside backup zips.
    #[test]
    fn a_new_prayer_is_written_exactly_so() {
        let home = scratch("golden");
        let path = add(&home, "  Anna's surgery ", ID_A, "2026-11-21T10:00:00Z").unwrap();
        assert_eq!(path.file_name().unwrap(), format!("{ID_A}.json").as_str());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!(
                r#"{{
  "format": "plumbline-prayer-v1",
  "id": "{ID_A}",
  "title": "Anna's surgery",
  "created": "2026-11-21T10:00:00Z",
  "updated": "2026-11-21T10:00:00Z",
  "lang": "en"
}}
"#
            )
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn a_prayer_is_prayed_for_answered_reopened_and_deleted() {
        let home = scratch("life");
        assert!(load(&home).0.is_empty());
        add(&home, "Anna's surgery", ID_A, "2026-11-01T00:00:00Z").unwrap();
        add(&home, "A new job", ID_B, "2026-11-02T00:00:00Z").unwrap();
        assert_eq!(load(&home).0.iter().map(|p| p.prayer.id.as_str()).collect::<Vec<_>>(), [ID_A, ID_B]);

        // Praying is recorded once a day, and is not an edit.
        mark_prayed(&home, ID_A, "2026-11-05").unwrap();
        mark_prayed(&home, ID_A, "2026-11-03").unwrap();
        mark_prayed(&home, ID_A, "2026-11-05").unwrap();
        let a = one(&home, ID_A);
        assert_eq!(a.prayed, ["2026-11-03", "2026-11-05"]);
        assert_eq!(a.updated, "2026-11-01T00:00:00Z");

        answer(&home, ID_A, Some("  went well "), "2026-11-10", "2026-11-10T08:00:00Z").unwrap();
        let a = one(&home, ID_A);
        assert_eq!(a.answered.as_deref(), Some("2026-11-10"));
        assert_eq!(a.answer.as_deref(), Some("went well"));
        assert_eq!(a.updated, "2026-11-10T08:00:00Z");
        assert_eq!(a.prayed.len(), 2, "answering lost the days prayed");
        // The comment is optional.
        answer(&home, ID_B, Some("  "), "2026-11-11", "2026-11-11T08:00:00Z").unwrap();
        assert_eq!(one(&home, ID_B).answer, None);

        reopen(&home, ID_A, "2026-11-12T08:00:00Z").unwrap();
        let a = one(&home, ID_A);
        assert_eq!((a.answered, a.answer), (None, None));

        assert_eq!(delete(&home, ID_A).unwrap().as_deref(), Some("2026-11-12T08:00:00Z"));
        assert_eq!(delete(&home, ID_A).unwrap(), None, "deleting nothing is not an error");
        assert_eq!(load(&home).0.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_edit_changes_the_title_and_keeps_unknown_keys() {
        let home = scratch("edit");
        std::fs::create_dir_all(home.join(DIR)).unwrap();
        std::fs::write(
            prayer_file(&home, ID_A),
            format!(
                r#"{{"format":"plumbline-prayer-v1","id":"{ID_A}","title":"old","created":"2026-11-01T00:00:00Z","updated":"2026-11-01T00:00:00Z","colour":"gold"}}"#
            ),
        )
        .unwrap();
        edit(&home, ID_A, "new", "2026-11-02T00:00:00Z").unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(prayer_file(&home, ID_A)).unwrap()).unwrap();
        assert_eq!(back["title"], "new");
        assert_eq!(back["created"], "2026-11-01T00:00:00Z");
        assert_eq!(back["updated"], "2026-11-02T00:00:00Z");
        assert_eq!(back["colour"], "gold", "an unknown key was stripped");
        assert!(back.get("lang").is_none(), "a re-save invented a provenance the file never had");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn empty_titles_bad_days_and_unknown_ids_are_refused() {
        let home = scratch("refuse");
        assert!(add(&home, "   ", ID_A, "t").is_err());
        assert!(load(&home).0.is_empty(), "a refused prayer was written");
        add(&home, "x", ID_A, "t").unwrap();
        assert!(edit(&home, ID_A, " ", "t2").is_err());
        assert!(mark_prayed(&home, ID_A, "today").is_err());
        assert!(mark_prayed(&home, ID_A, "2026-13-40").is_err());
        assert!(mark_prayed(&home, ID_B, "2026-11-01").is_err());
        assert!(answer(&home, ID_B, None, "2026-11-01", "t2").is_err());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Unreadable bytes are reported, not silently treated as "no prayer".
    #[test]
    fn unreadable_and_foreign_prayers_are_reported_rather_than_dropped() {
        let home = scratch("malformed");
        let d = home.join(DIR);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("truncated.json"), r#"{"format":"plumbline-prayer-v1","id":"x","title":"ha"#).unwrap();
        std::fs::write(d.join("later.json"), r#"{"format":"plumbline-prayer-v2","id":"x","title":"x","created":"t"}"#)
            .unwrap();
        add(&home, "x", ID_A, "t").unwrap();
        let (prayers, errs) = load(&home);
        assert_eq!(prayers.len(), 1);
        assert_eq!(errs.len(), 2, "{errs:?}");
        assert!(errs.join(" | ").contains("unknown prayer format plumbline-prayer-v2"));
        let _ = std::fs::remove_dir_all(&home);
    }
}
