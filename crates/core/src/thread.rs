//! Threads: named trails of passages through the text, with notes.
//!
//! A thread (e.g. "Romans Road") collects entries — a verse ref plus an
//! inclusive word span and a snapshot of the words it covered — each with an
//! optional note, alongside a running notes document on the thread itself.
//! Threads are personal study data: plain JSON, one file per thread under
//! `home/threads`. Ported from overlay `Thread.hs` (read side; the atomic
//! writer / `addToThread` land with the store layer later).

use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::reference::VRef;
use crate::Error;

/// Inclusive word-index span within a verse, like a patch span.
pub type Span = (u16, u16);

const FORMAT: &str = "overlay-thread-v1";

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
}

impl Serialize for ThreadEntry {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        EntryRepr {
            ref_key: self.vref.ref_key(),
            span: self.span,
            text: self.text.clone(),
            note: self.note.clone(),
            added: self.added.clone(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for ThreadEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = EntryRepr::deserialize(d)?;
        let vref = VRef::parse_ref_key(&r.ref_key)
            .ok_or_else(|| D::Error::custom(format!("bad entry ref: {}", r.ref_key)))?;
        Ok(ThreadEntry { vref, span: r.span, text: r.text, note: r.note, added: r.added })
    }
}

/// A named trail of passages plus a running notes document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub name: String,
    pub tok_version: String,
    pub notes: String,
    pub entries: Vec<ThreadEntry>,
    pub created: String,
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
}

impl Serialize for Thread {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Thread", 6)?;
        st.serialize_field("format", FORMAT)?;
        st.serialize_field("name", &self.name)?;
        st.serialize_field("tokenization", &self.tok_version)?;
        st.serialize_field("notes", &self.notes)?;
        st.serialize_field("entries", &self.entries)?;
        st.serialize_field("created", &self.created)?;
        st.end()
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
        })
    }
}

/// A thread plus the file it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect(),
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
    loaded.sort_by(|a, b| a.thread.name.to_lowercase().cmp(&b.thread.name.to_lowercase()));
    (loaded, errors)
}

/// Serialize a thread to pretty JSON with a trailing newline (matches overlay's
/// on-disk form).
pub fn to_json(thread: &Thread) -> Result<String, Error> {
    serde_json::to_string_pretty(thread)
        .map(|s| s + "\n")
        .map_err(|e| Error::Parse(e.to_string()))
}

/// The file a thread named `name` lives in, under `home/threads`.
pub fn thread_file(home: impl AsRef<Path>, name: &str) -> PathBuf {
    home.as_ref().join("threads").join(format!("{}.json", crate::store::slug(name, "thread")))
}

/// Atomically write a thread to `path`.
pub fn write_thread(path: impl AsRef<Path>, thread: &Thread) -> Result<(), Error> {
    crate::store::write_atomic(path, &to_json(thread)?)
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
        let mut thread = lt.thread.clone();
        thread.entries.push(entry);
        write_thread(&lt.file, &thread)?;
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
            created,
        };
        write_thread(&path, &thread)?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "format":"overlay-thread-v1","name":"Romans Road",
      "tokenization":"kjv1769-tok2","notes":"the gospel in Romans","created":"2026-07-03T00:00:00Z",
      "entries":[
        {"ref":"Rom 3:23","span":[0,11],"text":["For","all","have","sinned"],"note":null,"added":"2026-07-03T00:00:00Z"},
        {"ref":"Rom 6:23","span":[0,19],"text":["For","the","wages"],"note":"contrast","added":"2026-07-03T00:00:00Z"}
      ]}"#;

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
        assert!(serde_json::from_str::<Thread>(r#"{"format":"nope","name":"x","tokenization":"t","created":"c"}"#).is_err());
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
        let home = std::env::temp_dir().join(format!("pure-thread-{}", std::process::id()));
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
        let path = add_to_thread(&home, &loaded, "Romans Road", "kjv1769-tok2", entry(VRef::new("Rom", 3, 23), None)).unwrap();
        assert!(path.exists());

        // Second add (reloading first) appends to the same thread.
        let (loaded, _) = load_threads(&home);
        assert_eq!(loaded.len(), 1);
        add_to_thread(&home, &loaded, "romans road", "kjv1769-tok2", entry(VRef::new("Rom", 6, 23), Some("n"))).unwrap();

        let (loaded, errs) = load_threads(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        let t = &loaded[0].thread;
        assert_eq!(t.name, "Romans Road");
        assert_eq!(t.entries.len(), 2);
        assert_eq!(t.entries[1].vref, VRef::new("Rom", 6, 23));

        let _ = std::fs::remove_dir_all(&home);
    }
}
