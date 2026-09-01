//! Threads: named trails of passages through the text, with notes.
//!
//! A thread (e.g. "Romans Road") collects entries — a verse ref plus an
//! inclusive word span and a snapshot of the words it covered — each with an
//! optional note, alongside a running notes document on the thread itself.
//! Threads are personal study data: plain JSON, one file per thread under
//! `home/threads`. Ported from overlay `Thread.hs` (read side; the atomic
//! writer / `addToThread` land with the store layer later).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::reference::VRef;
use crate::Error;

/// Inclusive word-index span within a verse, like a patch span.
pub type Span = (u16, u16);

const FORMAT: &str = "overlay-thread-v1";

/// The threads every install is seeded with, from `stock/threads/`.
///
/// This is what a SHARE may offer. A link carries a thread by NAME, and the only
/// names the recipient is guaranteed to have are the ones their own install
/// seeded — offering a thread the sender authored would build a link that lands
/// on the plain app for everyone except the person who made it.
///
/// A list rather than a single name because the stock set is expected to grow.
/// `stock_threads_are_the_shipped_set` holds it to `stock/threads/`, so adding a
/// file there without adding it here fails rather than quietly staying
/// unshareable.
pub const STOCK_THREADS: [&str; 1] = ["Romans Road"];

/// The thread "share the gospel" opens when the reader has not chosen another.
/// The shell's `GOSPEL_THREAD_DEFAULT` is this same string.
pub const GOSPEL_DEFAULT: &str = STOCK_THREADS[0];

/// One passage on a thread: where it is, which words it covered (a snapshot),
/// and an optional note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadEntry {
    pub vref: VRef,
    pub span: Span,
    /// Snapshot of the canonical words when added, so the file reads on its own.
    pub text: Vec<String>,
    pub note: Option<String>,
    /// UTC timestamp the entry was added.
    pub added: String,
}

// On-disk form: the ref is the compact key string ("Rom 3:23").
#[derive(Serialize, Deserialize)]
struct EntryRepr {
    #[serde(rename = "ref")]
    ref_key: String,
    span: Span,
    text: Vec<String>,
    #[serde(default)]
    note: Option<String>,
    added: String,
    /// Unknown keys on this entry — see [`Thread::extra`]. Unlike the thread's
    /// own, these are carried by the *file* rather than by the loaded value:
    /// [`ThreadEntry`] is built field by field in `crates/ffi`, so it cannot gain
    /// a field without a change outside this crate. [`write_thread`] lifts them
    /// off the file it is replacing.
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl EntryRepr {
    /// The entry's natural key — `(refKey, added)`, per docs/STABLE-IDS.md.
    fn key(&self) -> (String, String) {
        (self.ref_key.clone(), self.added.clone())
    }
}

impl Serialize for ThreadEntry {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_repr(Map::new()).serialize(s)
    }
}

impl ThreadEntry {
    fn to_repr(&self, extra: Map<String, Value>) -> EntryRepr {
        EntryRepr {
            ref_key: self.vref.ref_key(),
            span: self.span,
            text: self.text.clone(),
            note: self.note.clone(),
            added: self.added.clone(),
            extra,
        }
    }
}

impl<'de> Deserialize<'de> for ThreadEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = EntryRepr::deserialize(d)?;
        let vref =
            VRef::parse_ref_key(&r.ref_key).ok_or_else(|| D::Error::custom(format!("bad entry ref: {}", r.ref_key)))?;
        Ok(ThreadEntry { vref, span: r.span, text: r.text, note: r.note, added: r.added })
    }
}

/// A named trail of passages plus a running notes document.
#[derive(Debug, Clone, PartialEq)]
pub struct Thread {
    pub name: String,
    pub tok_version: String,
    pub notes: String,
    pub entries: Vec<ThreadEntry>,
    pub created: String,
    /// Stable identity — 32 hex chars, minted once, never derived from the name
    /// (docs/STABLE-IDS.md). See [`crate::tag::Tag::id`] for why it exists; the
    /// rules are identical, and [`write_thread`] assigns one lazily.
    ///
    /// The ENTRIES need none: `(refKey, added)` is already a natural key, which
    /// is what [`EntryRepr::key`] leans on.
    pub id: Option<String>,
    /// UTC stamp of the last mutating save. See [`crate::tag::Tag::updated`].
    pub updated: Option<String>,
    /// Every key in the file this build has never heard of, carried back out
    /// again on save.
    ///
    /// The on-disk formats evolve **additively** (CLAUDE.md §Data formats), and
    /// a sideloaded APK never auto-updates: a build that drops the fields of a
    /// later one drops them for good on that device. So a thread written by a
    /// v1.1 and re-saved here comes back whole.
    ///
    /// Serde fills this with the leftovers after the known fields are matched, so
    /// a known key can never be swallowed, and a key a later version promotes to
    /// a real field stops arriving here the moment that field exists — it can
    /// never be written twice. Empty for every thread on disk today, and an empty
    /// flattened map writes no key at all, so those files are written exactly as
    /// they were.
    pub extra: Map<String, Value>,
}

#[derive(Deserialize)]
struct ThreadRepr {
    format: String,
    name: String,
    tokenization: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    entries: Vec<ThreadEntry>,
    created: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    updated: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

/// The writing side of the file, with each entry's unknown keys attached. Kept
/// separate from [`ThreadRepr`] only because the reader hands entries back as
/// [`ThreadEntry`], which has nowhere to put them.
#[derive(Serialize)]
struct ThreadOut<'a> {
    format: &'a str,
    name: &'a str,
    tokenization: &'a str,
    notes: &'a str,
    entries: Vec<EntryRepr>,
    created: &'a str,
    /// Both additive, and both skipped when absent — a thread from before ids
    /// existed is written exactly as it was.
    #[serde(skip_serializing_if = "Option::is_none")]
    id: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: &'a Option<String>,
    #[serde(flatten)]
    extra: &'a Map<String, Value>,
}

impl Serialize for Thread {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_out(&HashMap::new()).serialize(s)
    }
}

impl Thread {
    /// The file as it will be written: known fields in their frozen order, then
    /// whatever `entry_extras` has for each entry, then the thread's own unknown
    /// keys.
    fn to_out<'a>(&'a self, entry_extras: &EntryExtras) -> ThreadOut<'a> {
        ThreadOut {
            format: FORMAT,
            name: &self.name,
            tokenization: &self.tok_version,
            notes: &self.notes,
            entries: self
                .entries
                .iter()
                .map(|e| {
                    let repr = e.to_repr(Map::new());
                    let extra = entry_extras.get(&repr.key()).cloned().unwrap_or_default();
                    EntryRepr { extra, ..repr }
                })
                .collect(),
            created: &self.created,
            id: &self.id,
            updated: &self.updated,
            extra: &self.extra,
        }
    }
}

impl<'de> Deserialize<'de> for Thread {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = ThreadRepr::deserialize(d)?;
        if r.format != FORMAT {
            return Err(D::Error::custom(format!("unknown thread format: {}", r.format)));
        }
        Ok(Thread {
            name: r.name,
            tok_version: r.tokenization,
            notes: r.notes,
            entries: r.entries,
            created: r.created,
            id: r.id,
            updated: r.updated,
            extra: r.extra,
        })
    }
}

/// Unknown keys per entry, by natural key. Entries whose key is not unique in
/// the file are left out entirely: a key that might belong to either of two
/// entries belongs to neither.
type EntryExtras = HashMap<(String, String), Map<String, Value>>;

/// The unknown keys carried by the entries of the file at `path`, if it is one we
/// can read and it has any. Bytes we cannot parse yield nothing — that file is
/// refused elsewhere, never merged.
fn entry_extras(path: &Path) -> EntryExtras {
    let Ok(bytes) = std::fs::read(path) else { return EntryExtras::new() };
    // The entries only: the thread's own unknown keys ride on `Thread` itself.
    #[derive(Deserialize)]
    struct EntriesOnly {
        #[serde(default)]
        entries: Vec<EntryRepr>,
    }
    let Ok(file) = serde_json::from_slice::<EntriesOnly>(&bytes) else { return EntryExtras::new() };
    let mut seen: HashMap<(String, String), u32> = HashMap::new();
    for e in &file.entries {
        *seen.entry(e.key()).or_insert(0) += 1;
    }
    file.entries
        .into_iter()
        .filter(|e| !e.extra.is_empty() && seen.get(&e.key()) == Some(&1))
        .map(|e| (e.key(), e.extra))
        .collect()
}

/// A thread plus the file it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedThread {
    pub file: PathBuf,
    pub thread: Thread,
}

/// Load every `home/threads/*.json`, sorted by (lowercased) name. Files that
/// fail to parse are reported rather than silently dropped; a missing directory
/// yields no threads. Ported from `loadThreads`.
pub fn load_threads(home: impl AsRef<Path>) -> (Vec<LoadedThread>, Vec<String>) {
    let dir = home.as_ref().join("threads");
    let mut files: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(entries) => {
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json")).collect()
        }
        Err(_) => Vec::new(),
    };
    files.sort();

    let mut loaded = Vec::new();
    let mut errors = Vec::new();
    for path in files {
        match std::fs::read(&path) {
            Err(e) => errors.push(format!("{}: {e}", path.display())),
            Ok(bytes) => match serde_json::from_slice::<Thread>(&bytes) {
                Ok(thread) => loaded.push(LoadedThread { file: path, thread }),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    loaded.sort_by_key(|a| a.thread.name.to_lowercase());
    let loaded =
        crate::store::resolve_duplicate_ids(loaded, |lt| lt.thread.id.as_deref(), |lt| lt.thread.updated.as_deref());
    (loaded, errors)
}

/// Serialize a thread to pretty JSON with a trailing newline (matches overlay's
/// on-disk form).
pub fn to_json(thread: &Thread) -> Result<String, Error> {
    to_json_keeping(thread, &EntryExtras::new())
}

fn to_json_keeping(thread: &Thread, entry_extras: &EntryExtras) -> Result<String, Error> {
    serde_json::to_string_pretty(&thread.to_out(entry_extras))
        .map(|s| s + "\n")
        .map_err(|e| Error::Parse(e.to_string()))
}

/// The file a thread named `name` lives in, under `home/threads`.
pub fn thread_file(home: impl AsRef<Path>, name: &str) -> PathBuf {
    home.as_ref().join("threads").join(format!("{}.json", crate::store::slug(name, "thread")))
}

/// Atomically write a thread to `path`.
///
/// Reads whatever is at `path` first, to carry each entry's unknown keys forward
/// (see [`EntryRepr::extra`]) — the thread's own ride on the value being written.
/// Stamps as it writes — `updated` becomes `now`, and a thread with no
/// [`id`](Thread::id) is assigned one. See [`crate::tag::write_tag`] for why the
/// stamping lives in the writer.
pub fn write_thread(path: impl AsRef<Path>, thread: &Thread, now: &str) -> Result<(), Error> {
    let mut stamped = thread.clone();
    stamped.updated = Some(now.to_string());
    if stamped.id.is_none() {
        stamped.id = Some(crate::store::new_id());
    }
    let json = to_json_keeping(&stamped, &entry_extras(path.as_ref()))?;
    crate::store::write_atomic(path, &json)
}

/// Delete a whole thread — its file and everything on it.
///
/// Matched case-insensitively among `loaded`, the same way [`add_to_thread`]
/// matches, so "Romans Road" and "romans road" are the same thread here as they
/// are there. A name that isn't loaded is a no-op rather than an error: the
/// thread the caller wanted gone is gone either way.
pub fn remove_thread(loaded: &[LoadedThread], name: &str) -> Result<bool, Error> {
    let wanted = name.trim().to_lowercase();
    let Some(lt) = loaded.iter().find(|lt| lt.thread.name.to_lowercase() == wanted) else {
        return Ok(false);
    };
    match std::fs::remove_file(&lt.file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::Io { path: lt.file.display().to_string(), source: e }),
    }
}

/// Append `entry` to the thread named `name` (case-insensitive match among
/// `loaded`), creating its file on first use. Returns the file written. A file
/// that exists but is absent from `loaded` (i.e. it failed to parse) is refused
/// rather than clobbered. Ported from `addToThread` (caller supplies the
/// timestamp on `entry`, keeping this deterministic and testable).
pub fn add_to_thread(
    home: impl AsRef<Path>,
    loaded: &[LoadedThread],
    name: &str,
    tok_version: &str,
    entry: ThreadEntry,
) -> Result<PathBuf, Error> {
    let wanted = name.trim().to_lowercase();
    if let Some(lt) = loaded.iter().find(|lt| lt.thread.name.to_lowercase() == wanted) {
        let now = entry.added.clone();
        let mut thread = lt.thread.clone();
        thread.entries.push(entry);
        write_thread(&lt.file, &thread, &now)?;
        Ok(lt.file.clone())
    } else {
        let path = thread_file(&home, name);
        if path.exists() {
            return Err(Error::Corpus(format!(
                "{} exists but could not be read — refusing to overwrite",
                path.display()
            )));
        }
        let created = entry.added.clone();
        let thread = Thread {
            name: name.trim().to_string(),
            tok_version: tok_version.to_string(),
            notes: String::new(),
            entries: vec![entry],
            created: created.clone(),
            // Both filled by the writer, so a new thread is stamped through the
            // same path an edited one is.
            id: None,
            updated: None,
            // The one place a new file's provenance can honestly be recorded:
            // its refKeys are being written NOW, in the language the reader is
            // reading. A writer would be the wrong place — it also runs on
            // re-save, where an unstamped older file would gain a confident
            // wrong answer. See `i18n::stamp`.
            extra: crate::i18n::stamped_extra(),
        };
        write_thread(&path, &thread, &created)?;
        Ok(path)
    }
}

/// Find the loaded thread named `name` (case-insensitive) or error — the shared
/// preamble for the note editors, which edit an existing thread rather than
/// create one.
fn find_thread<'a>(loaded: &'a [LoadedThread], name: &str) -> Result<&'a LoadedThread, Error> {
    let wanted = name.trim().to_lowercase();
    loaded
        .iter()
        .find(|lt| lt.thread.name.to_lowercase() == wanted)
        .ok_or_else(|| Error::Corpus(format!("no thread named {name}")))
}

/// Replace the running notes document of the thread named `name`. The thread
/// must already exist among `loaded`.
pub fn set_thread_notes(loaded: &[LoadedThread], name: &str, notes: &str, now: &str) -> Result<PathBuf, Error> {
    let lt = find_thread(loaded, name)?;
    let mut thread = lt.thread.clone();
    thread.notes = notes.to_string();
    write_thread(&lt.file, &thread, now)?;
    Ok(lt.file.clone())
}

/// Set (or clear, with `None`) the note on entry `index` of the thread named
/// `name`. The thread must exist and `index` be in range.
pub fn set_entry_note(
    loaded: &[LoadedThread],
    name: &str,
    index: usize,
    note: Option<String>,
    now: &str,
) -> Result<PathBuf, Error> {
    let lt = find_thread(loaded, name)?;
    let mut thread = lt.thread.clone();
    let entry =
        thread.entries.get_mut(index).ok_or_else(|| Error::Corpus(format!("thread {name} has no entry {index}")))?;
    // An empty note reads as "no note".
    entry.note = note.filter(|n| !n.trim().is_empty());
    write_thread(&lt.file, &thread, now)?;
    Ok(lt.file.clone())
}

/// Drop entry `index` from the thread named `name`.
///
/// The thread survives its last entry being removed: an empty thread is a
/// heading someone is still filling in, and deleting the whole thing on the way
/// past would throw away its name, its notes and its id — which is
/// [`remove_thread`]'s job, asked for deliberately.
pub fn remove_from_thread(loaded: &[LoadedThread], name: &str, index: usize, now: &str) -> Result<PathBuf, Error> {
    let lt = find_thread(loaded, name)?;
    let mut thread = lt.thread.clone();
    if index >= thread.entries.len() {
        return Err(Error::Corpus(format!("thread {name} has no entry {index}")));
    }
    thread.entries.remove(index);
    write_thread(&lt.file, &thread, now)?;
    Ok(lt.file.clone())
}

/// Move entry `from` to position `to` in the thread named `name`.
///
/// The ORDER IS THE ARGUMENT a thread makes — the Romans Road is a road, and
/// the verses are in the order someone walks them — so this is a reorder, not a
/// sort: whatever the reader arranges is what Present steps through.
///
/// `to` is clamped to the last valid position rather than refused, so a shell
/// that says "move it down" on the last entry gets a no-op instead of an error
/// it has to special-case.
pub fn move_in_thread(
    loaded: &[LoadedThread],
    name: &str,
    from: usize,
    to: usize,
    now: &str,
) -> Result<PathBuf, Error> {
    let lt = find_thread(loaded, name)?;
    let mut thread = lt.thread.clone();
    let n = thread.entries.len();
    if from >= n {
        return Err(Error::Corpus(format!("thread {name} has no entry {from}")));
    }
    let to = to.min(n.saturating_sub(1));
    if to == from {
        // Nothing moved. Still a success, and still NOT a write: rewriting the
        // file would bump `updated` and make a no-op look like an edit to
        // anything diffing or syncing it.
        return Ok(lt.file.clone());
    }
    let entry = thread.entries.remove(from);
    thread.entries.insert(to, entry);
    write_thread(&lt.file, &thread, now)?;
    Ok(lt.file.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`STOCK_THREADS`] against the files actually in `stock/threads/`.
    ///
    /// The share palette offers exactly this list, so a thread added to the
    /// stock set and not to the constant would ship as something every install
    /// has and nobody can share. Reads the NAME out of each file rather than
    /// its filename: the name is what a link carries, and `romans-road.json`
    /// holding "Romans Road" is precisely the mapping that could drift.
    ///
    /// Skips when the tree is not beside the crate (a packaged crate has no
    /// `stock/`), the same shape as the other repo-relative tests here.
    #[test]
    fn stock_threads_are_the_shipped_set() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../stock/threads");
        if !dir.is_dir() {
            eprintln!("no stock/threads beside the crate — skipping");
            return;
        }
        let mut shipped: Vec<String> = std::fs::read_dir(&dir)
            .expect("stock/threads")
            .filter_map(|e| {
                let path = e.ok()?.path();
                (path.extension()? == "json").then_some(path)
            })
            .map(|p| {
                let raw = std::fs::read_to_string(&p).expect("stock thread");
                let v: Value = serde_json::from_str(&raw).expect("stock thread is JSON");
                v["name"].as_str().expect("a stock thread has a name").to_string()
            })
            .collect();
        shipped.sort();
        let mut declared: Vec<String> = STOCK_THREADS.iter().map(|s| s.to_string()).collect();
        declared.sort();
        assert_eq!(shipped, declared, "stock/threads/ and thread::STOCK_THREADS disagree");
    }

    const SAMPLE: &str = r#"{
      "format":"overlay-thread-v1","name":"Romans Road",
      "tokenization":"kjv1769-tok2","notes":"the gospel in Romans","created":"2026-07-03T00:00:00Z",
      "entries":[
        {"ref":"Rom 3:23","span":[0,11],"text":["For","all","have","sinned"],"note":null,"added":"2026-07-03T00:00:00Z"},
        {"ref":"Rom 6:23","span":[0,19],"text":["For","the","wages"],"note":"contrast","added":"2026-07-03T00:00:00Z"}
      ]}"#;

    /// Rearranging and removing, the two edits a thread accumulates a need for
    /// — a road gets a verse in the wrong place, or one that turned out not to
    /// belong.
    #[test]
    fn entries_can_be_reordered_and_removed() {
        let dir = std::env::temp_dir().join(format!("pl-thread-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // `load_threads` reads `<home>/threads`, so the sample goes there.
        std::fs::create_dir_all(dir.join("threads")).unwrap();
        let file = dir.join("threads").join("romans-road.json");
        std::fs::write(&file, SAMPLE).unwrap();
        let reload = || -> Vec<LoadedThread> { load_threads(&dir).0 };
        let refs =
            |lts: &[LoadedThread]| -> Vec<String> { lts[0].thread.entries.iter().map(|e| e.vref.ref_key()).collect() };

        let loaded = reload();
        assert_eq!(refs(&loaded), ["Rom 3:23", "Rom 6:23"]);

        // Down, then back up: the order is the reader's, and it round-trips.
        move_in_thread(&loaded, "Romans Road", 0, 1, "2026-08-18T00:00:00Z").unwrap();
        let loaded = reload();
        assert_eq!(refs(&loaded), ["Rom 6:23", "Rom 3:23"]);
        move_in_thread(&loaded, "Romans Road", 1, 0, "2026-08-18T00:00:00Z").unwrap();
        let loaded = reload();
        assert_eq!(refs(&loaded), ["Rom 3:23", "Rom 6:23"]);

        // "Move the last one down" is a no-op, not an error — the shell should
        // not have to know which end it is on. And a no-op must not rewrite the
        // file: `updated` would move and make it look edited.
        let before = std::fs::read_to_string(&file).unwrap();
        move_in_thread(&loaded, "Romans Road", 1, 9, "2026-08-18T00:00:00Z").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before, "a no-op move rewrote the file");

        // An entry that turned out not to belong. The note on the survivor
        // rides along — entries are removed by position, not rebuilt.
        remove_from_thread(&reload(), "Romans Road", 0, "2026-08-18T00:00:00Z").unwrap();
        let loaded = reload();
        assert_eq!(refs(&loaded), ["Rom 6:23"]);
        assert_eq!(loaded[0].thread.entries[0].note.as_deref(), Some("contrast"));

        // THE THREAD SURVIVES ITS LAST ENTRY. An empty thread is a heading
        // someone is still filling in; throwing away its name and notes is
        // `remove_thread`'s job and has to be asked for.
        remove_from_thread(&loaded, "Romans Road", 0, "2026-08-18T00:00:00Z").unwrap();
        let loaded = reload();
        assert_eq!(loaded.len(), 1, "the thread itself was deleted with its last entry");
        assert!(loaded[0].thread.entries.is_empty());
        assert_eq!(loaded[0].thread.notes, "the gospel in Romans");

        // Out of range says so rather than silently doing nothing.
        assert!(remove_from_thread(&loaded, "Romans Road", 0, "2026-08-18T00:00:00Z").is_err());
        assert!(move_in_thread(&loaded, "Romans Road", 0, 0, "2026-08-18T00:00:00Z").is_err());
        assert!(move_in_thread(&loaded, "Nope", 0, 0, "2026-08-18T00:00:00Z").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_a_thread() {
        let t: Thread = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(t.name, "Romans Road");
        assert_eq!(t.notes, "the gospel in Romans");
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[0].vref, VRef::new("Rom", 3, 23));
        assert_eq!(t.entries[0].span, (0, 11));
        assert_eq!(t.entries[1].note.as_deref(), Some("contrast"));
    }

    #[test]
    fn rejects_wrong_format_and_bad_ref() {
        assert!(
            serde_json::from_str::<Thread>(r#"{"format":"nope","name":"x","tokenization":"t","created":"c"}"#).is_err()
        );
        let bad = r#"{"format":"overlay-thread-v1","name":"x","tokenization":"t","created":"c",
            "entries":[{"ref":"garbage","span":[0,1],"text":[],"added":"a"}]}"#;
        assert!(serde_json::from_str::<Thread>(bad).is_err());
    }

    #[test]
    fn roundtrips_through_json() {
        let t: Thread = serde_json::from_str(SAMPLE).unwrap();
        let s = to_json(&t).unwrap();
        let back: Thread = serde_json::from_str(&s).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn missing_dir_is_empty() {
        let (threads, errs) = load_threads("/no/such/home");
        assert!(threads.is_empty());
        assert!(errs.is_empty());
    }

    #[test]
    fn add_creates_then_appends_and_reloads() {
        let home = std::env::temp_dir().join(format!("plumbline-thread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let entry = |v, note: Option<&str>| ThreadEntry {
            vref: v,
            span: (0, 1),
            text: vec!["a".into(), "b".into()],
            note: note.map(String::from),
            added: "2026-01-01T00:00:00Z".into(),
        };

        // First add creates the file.
        let (loaded, _) = load_threads(&home);
        let path =
            add_to_thread(&home, &loaded, "Romans Road", "kjv1769-tok2", entry(VRef::new("Rom", 3, 23), None)).unwrap();
        assert!(path.exists());

        // Second add (reloading first) appends to the same thread.
        let (loaded, _) = load_threads(&home);
        assert_eq!(loaded.len(), 1);
        add_to_thread(&home, &loaded, "romans road", "kjv1769-tok2", entry(VRef::new("Rom", 6, 23), Some("n")))
            .unwrap();

        let (loaded, errs) = load_threads(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        let t = &loaded[0].thread;
        assert_eq!(t.name, "Romans Road");
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[1].vref, VRef::new("Rom", 6, 23));

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn edits_thread_and_entry_notes() {
        let home = std::env::temp_dir().join(format!("plumbline-thread-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let entry = ThreadEntry {
            vref: VRef::new("Rom", 3, 23),
            span: (0, 1),
            text: vec!["For".into()],
            note: None,
            added: "2026-01-01T00:00:00Z".into(),
        };
        let (loaded, _) = load_threads(&home);
        add_to_thread(&home, &loaded, "Romans Road", "kjv1769-tok2", entry).unwrap();

        // Set the thread's running notes and the first entry's note.
        let (loaded, _) = load_threads(&home);
        set_thread_notes(&loaded, "romans road", "the gospel", "2026-07-30T00:00:00Z").unwrap();
        let (loaded, _) = load_threads(&home);
        set_entry_note(&loaded, "Romans Road", 0, Some("all have sinned".into()), "2026-07-30T00:00:00Z").unwrap();

        let (loaded, _) = load_threads(&home);
        assert_eq!(loaded[0].thread.notes, "the gospel");
        assert_eq!(loaded[0].thread.entries[0].note.as_deref(), Some("all have sinned"));

        // A blank note clears it; a missing thread / out-of-range index errors.
        set_entry_note(&loaded, "Romans Road", 0, Some("  ".into()), "2026-07-30T00:00:00Z").unwrap();
        let (loaded, _) = load_threads(&home);
        assert_eq!(loaded[0].thread.entries[0].note, None);
        assert!(set_entry_note(&loaded, "Nope", 0, None, "2026-07-30T00:00:00Z").is_err());
        assert!(set_entry_note(&loaded, "Romans Road", 9, None, "2026-07-30T00:00:00Z").is_err());

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Forward compatibility: the on-disk formats evolve
    /// **additively** (CLAUDE.md §Data formats), and a sideloaded APK never
    /// auto-updates — so a key this build drops is dropped for good on that
    /// device. A thread written by a later build has to come back out whole, and
    /// that includes its ENTRIES: appending to a thread rewrites every one of
    /// them.
    #[test]
    fn a_thread_keeps_the_keys_of_a_later_build() {
        let home = std::env::temp_dir().join(format!("plumbline-thread-forward-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let path = thread_file(&home, "Romans Road");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
              "format":"overlay-thread-v1","name":"Romans Road","tokenization":"kjv1769-tok2",
              "notes":"the gospel in Romans","created":"2026-07-03T00:00:00Z",
              "entries":[
                {"ref":"Rom 3:23","span":[0,11],"text":["For","all","have","sinned"],
                 "added":"2026-07-03T00:00:00Z",
                 "colour":"amber","voice":{"clip":"rom3.ogg"},"seenAt":["2026-07-04"]}
              ],
              "id":"4b81aa","shared":{"with":"study group"},"aliases":["The Road"]
            }"#,
        )
        .unwrap();

        // Appending rewrites the file — the existing entry with it.
        let (loaded, errs) = load_threads(&home);
        assert!(errs.is_empty(), "{errs:?}");
        add_to_thread(
            &home,
            &loaded,
            "Romans Road",
            "kjv1769-tok2",
            ThreadEntry {
                vref: VRef::new("Rom", 6, 23),
                span: (0, 19),
                text: vec!["For".into(), "the".into(), "wages".into()],
                note: None,
                added: "2026-09-01T00:00:00Z".into(),
            },
        )
        .unwrap();

        let back: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["entries"].as_array().unwrap().len(), 2, "the append itself must land");
        assert_eq!(back["id"], "4b81aa", "an unknown scalar was stripped");
        assert_eq!(back["shared"], serde_json::json!({"with":"study group"}), "an unknown object was stripped");
        assert_eq!(back["aliases"], serde_json::json!(["The Road"]), "an unknown array was stripped");
        assert_eq!(back["entries"][0]["colour"], "amber", "an entry's unknown scalar was stripped");
        assert_eq!(
            back["entries"][0]["voice"],
            serde_json::json!({"clip":"rom3.ogg"}),
            "an entry's unknown object was stripped"
        );
        assert_eq!(
            back["entries"][0]["seenAt"],
            serde_json::json!(["2026-07-04"]),
            "an entry's unknown array was stripped"
        );
        // The entry this build wrote carries nothing of its own.
        assert_eq!(back["entries"][1]["ref"], "Rom 6:23");
        assert!(back["entries"][1].get("colour").is_none());

        // Editing an entry's note keeps the entry's own unknown keys: the note is
        // not part of what identifies it.
        let (loaded, _) = load_threads(&home);
        set_entry_note(&loaded, "Romans Road", 0, Some("all have sinned".into()), "2026-07-30T00:00:00Z").unwrap();
        let back: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back["entries"][0]["note"], "all have sinned");
        assert_eq!(back["entries"][0]["colour"], "amber", "editing the note cost the entry its keys");
        assert_eq!(back["id"], "4b81aa");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Two entries with the same natural key — the same verse added to the same
    /// thread in the same second — cannot be told apart, so a key that might
    /// belong to either belongs to neither. Dropping it is the only safe answer:
    /// copying it onto both would invent a fact about the entry that never had it.
    #[test]
    fn an_entry_key_that_is_not_unique_carries_nothing() {
        let home = std::env::temp_dir().join(format!("plumbline-thread-twins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let path = thread_file(&home, "Twins");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
              "format":"overlay-thread-v1","name":"Twins","tokenization":"kjv1769-tok2",
              "notes":"","created":"2026-07-03T00:00:00Z",
              "entries":[
                {"ref":"Rom 3:23","span":[0,1],"text":["For"],"added":"2026-07-03T00:00:00Z","colour":"amber"},
                {"ref":"Rom 3:23","span":[0,1],"text":["For"],"added":"2026-07-03T00:00:00Z"}
              ]}"#,
        )
        .unwrap();

        let (loaded, _) = load_threads(&home);
        set_thread_notes(&loaded, "Twins", "rewritten", "2026-07-30T00:00:00Z").unwrap();
        let back: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for e in back["entries"].as_array().unwrap() {
            assert!(e.get("colour").is_none(), "a key was copied onto an entry it may not belong to: {e}");
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A thread with nothing unknown in it is written byte for byte as it was
    /// before any of that landed — these files already ship inside backup zips.
    #[test]
    fn a_thread_with_no_unknown_keys_is_written_exactly_as_before() {
        let t: Thread = serde_json::from_str(SAMPLE).unwrap();
        assert_eq!(
            to_json(&t).unwrap(),
            r#"{
  "format": "overlay-thread-v1",
  "name": "Romans Road",
  "tokenization": "kjv1769-tok2",
  "notes": "the gospel in Romans",
  "entries": [
    {
      "ref": "Rom 3:23",
      "span": [
        0,
        11
      ],
      "text": [
        "For",
        "all",
        "have",
        "sinned"
      ],
      "note": null,
      "added": "2026-07-03T00:00:00Z"
    },
    {
      "ref": "Rom 6:23",
      "span": [
        0,
        19
      ],
      "text": [
        "For",
        "the",
        "wages"
      ],
      "note": "contrast",
      "added": "2026-07-03T00:00:00Z"
    }
  ],
  "created": "2026-07-03T00:00:00Z"
}
"#
        );
    }

    // ── stable ids (docs/STABLE-IDS.md) ──────────────────────────────────────

    /// Every mutating path a thread has stamps `updated` and keeps one id. This
    /// is the test that would catch a NEW mutator written to bypass
    /// [`write_thread`]: three different edits, one identity, three stamps.
    #[test]
    fn every_thread_mutator_stamps_updated_and_keeps_one_id() {
        let home = std::env::temp_dir().join(format!("plumbline-thread-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let entry = |v: VRef, added: &str| ThreadEntry {
            vref: v,
            span: (0, 1),
            text: vec!["For".into()],
            note: None,
            added: added.into(),
        };
        let read = |path: &Path| -> Value { serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap() };

        let (loaded, _) = load_threads(&home);
        let path = add_to_thread(
            &home,
            &loaded,
            "Romans Road",
            "kjv1769-tok2",
            entry(VRef::new("Rom", 3, 23), "2026-08-01T00:00:00Z"),
        )
        .unwrap();
        let created = read(&path);
        let id = created["id"].as_str().expect("a new thread was written without an id").to_string();
        assert_eq!(id.len(), 32, "an id is 32 hex chars: {id}");
        assert_eq!(created["updated"], "2026-08-01T00:00:00Z");

        // Appending an entry: the entry's own `added` is the stamp.
        let (loaded, _) = load_threads(&home);
        add_to_thread(
            &home,
            &loaded,
            "romans road",
            "kjv1769-tok2",
            entry(VRef::new("Rom", 6, 23), "2026-08-02T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(read(&path)["updated"], "2026-08-02T00:00:00Z");

        // The notes document, and one entry's note: no stamp of their own, so
        // these are the ones the engine's clock supplies in production.
        let (loaded, _) = load_threads(&home);
        set_thread_notes(&loaded, "Romans Road", "the gospel in Romans", "2026-08-03T00:00:00Z").unwrap();
        assert_eq!(read(&path)["updated"], "2026-08-03T00:00:00Z");

        let (loaded, _) = load_threads(&home);
        set_entry_note(&loaded, "Romans Road", 0, Some("all have sinned".into()), "2026-08-04T00:00:00Z").unwrap();
        let last = read(&path);
        assert_eq!(last["updated"], "2026-08-04T00:00:00Z");
        assert_eq!(last["id"].as_str(), Some(id.as_str()), "the identity moved under the reader");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// Two files, one id: the newer `updated` wins in memory and neither file is
    /// touched. Both orderings, since the loader sorts by name (see the tag
    /// version of this test — "keep the first" passes a one-pair test by luck).
    #[test]
    fn duplicate_thread_ids_keep_the_newer_and_load_deletes_nothing() {
        let home = std::env::temp_dir().join(format!("plumbline-thread-dup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let dir = home.join("threads");
        std::fs::create_dir_all(&dir).unwrap();
        let one = |name: &str, id: &str, updated: &str| {
            format!(
                r#"{{"format":"overlay-thread-v1","name":"{name}","tokenization":"kjv1769-tok2",
                    "notes":"","entries":[],"created":"2026-07-01T00:00:00Z","id":"{id}",
                    "updated":"{updated}"}}"#
            )
        };
        const A: &str = "11111111111111112222222222222222";
        const B: &str = "33333333333333334444444444444444";
        let files = [
            ("alpha.json", one("Alpha", A, "2026-08-01T00:00:00Z")),
            ("omega.json", one("Omega", A, "2026-08-02T00:00:00Z")),
            ("beta.json", one("Beta", B, "2026-08-02T00:00:00Z")),
            ("psi.json", one("Psi", B, "2026-08-01T00:00:00Z")),
        ];
        for (f, body) in &files {
            std::fs::write(dir.join(f), body).unwrap();
        }

        let (loaded, errs) = load_threads(&home);
        assert!(errs.is_empty(), "{errs:?}");
        let names: Vec<&str> = loaded.iter().map(|lt| lt.thread.name.as_str()).collect();
        assert_eq!(names, ["Beta", "Omega"]);
        for (f, _) in &files {
            assert!(dir.join(f).exists(), "load deleted {f}");
        }

        let _ = std::fs::remove_dir_all(&home);
    }
}
