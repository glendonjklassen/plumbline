//! Deletion records ("tombstones"): what the reader deleted, and when.
//!
//! A restore merges a backup into the device ([`crate::merge`]). Deleting
//! removes a file, so without a record a merge cannot tell "deleted here" from
//! "never had it", and every restore of an older backup would bring deleted
//! things back. One record per deleted thing, keyed as below, holding the UTC
//! stamp of the deletion; a merge drops anything the record is at least as new
//! as, and keeps anything re-created after it.
//!
//! One file, `home/deleted/tombstones.json`. Records are an id and a stamp,
//! some sixty bytes each, so they are kept rather than pruned: pruning would
//! let a backup older than the cutoff resurrect what was deleted before it.
//!
//! The keys are built here and nowhere else, so the ABI that records a deletion
//! and the merge that honours one cannot disagree about a spelling.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Error;

pub const FORMAT: &str = "plumbline-tombstones-v1";

/// Where the records live, relative to the home.
pub const REL_PATH: &str = "deleted/tombstones.json";

#[derive(Serialize, Deserialize, Default)]
struct File {
    format: String,
    #[serde(default)]
    deleted: BTreeMap<String, String>,
}

pub fn path(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join(REL_PATH)
}

/// Parse a tombstone file's bytes. Anything unreadable is no records: a
/// damaged file must not stop a restore, and the worst it costs is a deleted
/// item coming back.
pub fn parse(bytes: &[u8]) -> BTreeMap<String, String> {
    match serde_json::from_slice::<File>(bytes) {
        Ok(f) if f.format == FORMAT => f.deleted,
        _ => BTreeMap::new(),
    }
}

/// Serialize records as the file's bytes.
pub fn to_bytes(records: &BTreeMap<String, String>) -> Vec<u8> {
    let f = File { format: FORMAT.to_string(), deleted: records.clone() };
    (serde_json::to_string_pretty(&f).unwrap_or_default() + "\n").into_bytes()
}

/// Every record in the home.
pub fn load(home: impl AsRef<Path>) -> BTreeMap<String, String> {
    std::fs::read(path(home)).map(|b| parse(&b)).unwrap_or_default()
}

/// Record that `keys` were deleted at `stamp`. A key already recorded keeps the
/// later of the two stamps.
pub fn record(home: impl AsRef<Path>, keys: &[String], stamp: &str) -> Result<(), Error> {
    if keys.is_empty() {
        return Ok(());
    }
    let mut all = load(&home);
    for k in keys {
        let e = all.entry(k.clone()).or_default();
        if stamp > e.as_str() {
            *e = stamp.to_string();
        }
    }
    let text = String::from_utf8(to_bytes(&all)).map_err(|e| Error::Parse(e.to_string()))?;
    crate::store::write_atomic(path(home), &text)
}

/// Both sides' records, each key at its later stamp. Commutative, associative
/// and idempotent, as a merge of records must be.
pub fn union(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = a.clone();
    for (k, v) in b {
        let e = out.entry(k.clone()).or_default();
        if v > e {
            *e = v.clone();
        }
    }
    out
}

/// Whether a thing last touched at `stamp` was deleted afterwards (or at the
/// same moment). A thing with no stamp at all predates any deletion.
pub fn kills(records: &BTreeMap<String, String>, key: &str, stamp: &str) -> bool {
    records.get(key).is_some_and(|d| d.as_str() >= stamp)
}

// ── keys ─────────────────────────────────────────────────────────────────────

/// How a named, optionally id-carrying object is identified: by id, or by its
/// lowercased name when it has none yet (written before ids existed).
pub fn identity(id: Option<&str>, name: &str) -> String {
    match id {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => format!("name:{}", name.trim().to_lowercase()),
    }
}

pub fn note(id: &str) -> String {
    format!("note:{id}")
}
pub fn tag(identity: &str) -> String {
    format!("tag:{identity}")
}
/// A member removed from a tag. `target` is [`member_target`]'s spelling.
pub fn tag_member(tag_identity: &str, target: &str) -> String {
    format!("tagmember:{tag_identity}|{target}")
}
pub fn thread(identity: &str) -> String {
    format!("thread:{identity}")
}
pub fn weave(identity: &str) -> String {
    format!("weave:{identity}")
}
/// A suggested weave, by its file name under `weaves/suggested/`.
pub fn suggested(file_name: &str) -> String {
    format!("suggested:{file_name}")
}
/// A memory card, by its file name under `memory/`.
pub fn card(file_name: &str) -> String {
    format!("card:{file_name}")
}
pub fn plan(id: &str) -> String {
    format!("plan:{id}")
}
pub fn devotional(id: &str) -> String {
    format!("devotional:{id}")
}
/// A chapter's reading forgotten. `book` is the OSIS id.
pub fn reading(book: &str, chapter: u16) -> String {
    format!("reading:{}|{chapter}", book.to_lowercase())
}

/// A tag member's target, spelled one way for both a deletion record and a
/// merge: `verse:John 3:16` or `concept:H3068`.
pub fn member_target(t: &crate::tag::TagTarget) -> String {
    match t {
        crate::tag::TagTarget::Verse(v) => format!("verse:{}", v.ref_key()),
        crate::tag::TagTarget::Concept(c) => format!("concept:{c}"),
    }
}

/// [`member_target`] read off a member's JSON target (`{"kind":"verse","ref":…}`).
pub fn member_target_json(target: &Value) -> Option<String> {
    match target.get("kind")?.as_str()? {
        "verse" => Some(format!("verse:{}", target.get("ref")?.as_str()?)),
        "concept" => Some(format!("concept:{}", target.get("strongs")?.as_str()?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_keep_the_later_stamp_and_survive_a_round_trip() {
        let home = std::env::temp_dir().join(format!("plumbline-tomb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert!(load(&home).is_empty());
        record(&home, &[note("a")], "2026-02-01T00:00:00Z").unwrap();
        record(&home, &[note("a"), tag("t")], "2026-01-01T00:00:00Z").unwrap();
        let all = load(&home);
        assert_eq!(all[&note("a")], "2026-02-01T00:00:00Z", "an earlier stamp must not overwrite a later one");
        assert_eq!(all[&tag("t")], "2026-01-01T00:00:00Z");
        assert_eq!(parse(&to_bytes(&all)), all);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn union_is_order_free_and_idempotent() {
        let a = BTreeMap::from([("x".to_string(), "2".to_string()), ("y".to_string(), "1".to_string())]);
        let b = BTreeMap::from([("x".to_string(), "3".to_string()), ("z".to_string(), "1".to_string())]);
        assert_eq!(union(&a, &b), union(&b, &a));
        assert_eq!(union(&union(&a, &b), &b), union(&a, &b));
        assert_eq!(union(&a, &b)["x"], "3");
    }

    #[test]
    fn a_record_kills_what_it_is_newer_than_and_spares_what_came_after() {
        let r = BTreeMap::from([(note("a"), "2026-02-01T00:00:00Z".to_string())]);
        assert!(kills(&r, &note("a"), "2026-01-01T00:00:00Z"));
        assert!(kills(&r, &note("a"), "2026-02-01T00:00:00Z"), "a tie is the deletion");
        assert!(!kills(&r, &note("a"), "2026-03-01T00:00:00Z"), "re-created after the deletion");
        assert!(!kills(&r, &note("b"), ""), "no record, nothing killed");
    }

    #[test]
    fn a_damaged_file_is_no_records() {
        assert!(parse(b"{not json").is_empty());
        assert!(parse(br#"{"format":"something-else","deleted":{"a":"b"}}"#).is_empty());
    }

    #[test]
    fn identity_prefers_the_id_and_falls_back_to_the_name() {
        assert_eq!(identity(Some("abc"), "Grace"), "abc");
        assert_eq!(identity(None, " Grace "), "name:grace");
        assert_eq!(identity(Some(""), "Grace"), "name:grace");
    }
}
