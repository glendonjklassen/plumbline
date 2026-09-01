//! Cross-platform atomic file writes for personal study data (threads, tags,
//! weaves, notes).
//!
//! The contract: write the contents to a sibling temp file in the same
//! directory, flush + fsync, close, then `fs::rename` over the target. A rename
//! within one directory replaces the destination and is atomic on Unix and
//! Windows alike, so a concurrent reader never sees a half-written file;
//! closing the temp handle first is required on Windows.
//!
//! A process killed between the write and the rename strands a temp sibling in
//! an authored directory. This module mints those names, so it also owns the
//! rule for spotting one — [`is_temp_name`], which every enumerator over the
//! reader's directories should consult.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Error;

/// Atomically write `contents` to `path`, creating parent directories as needed.
pub fn write_atomic(path: impl AsRef<Path>, contents: &str) -> Result<(), Error> {
    write_atomic_bytes(path, contents.as_bytes())
}

/// Atomically write raw `bytes` to `path` — the binary sibling of
/// [`write_atomic`], for caches and other non-text artifacts.
pub fn write_atomic_bytes(path: impl AsRef<Path>, bytes: &[u8]) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
        }
    }

    let tmp = temp_sibling(path);
    // Scoped so the handle is closed before the rename: Windows will not replace
    // a file that still has one open.
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

/// A hidden temp path next to `path`, unique per process. The sibling keeps the
/// rename inside one directory, and so one filesystem, which atomicity requires.
fn temp_sibling(path: &Path) -> PathBuf {
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "out".to_string());
    let tmp_name = format!(".{name}.{}.tmp", temp_discriminator());
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Whether `name` is one [`temp_sibling`] mints — the leftover of an atomic
/// write that died between the write and the rename.
///
/// The rule is `.<name>.<digits>.tmp`, all three parts required, and
/// deliberately narrow because the loose versions delete the reader's own work:
/// "starts with a dot" takes `.config`, an authored directory; "ends with
/// `.tmp`" takes a file that arrived in someone else's backup zip. Nothing
/// derived from a name the reader typed can match, because [`slug`] maps every
/// non-alphanumeric to a separator and so cannot produce a dot.
///
/// Anything enumerating an authored directory should skip these, whether it is
/// loading, persisting or building a backup: a temp's bytes are a duplicate or
/// a fragment, and one that reaches a backup zip is restored onto the next
/// device as a permanent fixture nothing ever removes.
pub fn is_temp_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.') else { return false };
    let Some(rest) = rest.strip_suffix(".tmp") else { return false };
    // Split off the discriminator only; the stem keeps its own dots, so
    // `.Gen.1.7.json.4242.tmp` is recognised as readily as `.out.9.tmp`.
    let Some((stem, disc)) = rest.rsplit_once('.') else { return false };
    !stem.is_empty() && !disc.is_empty() && disc.bytes().all(|b| b.is_ascii_digit())
}

/// The unique-per-process part of a temp name: the pid natively. Wasm has none
/// (`std::process::id` panics there), but a wasm engine instance is
/// single-process, so a monotonic counter is equally collision-free.
#[cfg(not(target_arch = "wasm32"))]
fn temp_discriminator() -> u64 {
    std::process::id() as u64
}

#[cfg(target_arch = "wasm32")]
fn temp_discriminator() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Slug a display name into a filename stem: lowercase, non-alphanumerics to
/// separators, words joined by `-`. Empty input falls back to `fallback`.
///
/// The exact behaviour is frozen — the stock study set's filenames are slugs of
/// its names, so a change here orphans them.
pub fn slug(name: &str, fallback: &str) -> String {
    let cleaned: String =
        name.trim().to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    let s = cleaned.split_whitespace().collect::<Vec<_>>().join("-");
    if s.is_empty() {
        fallback.to_string()
    } else {
        s
    }
}

/// A fresh object identity: 32 lowercase hex chars (128 bits). Threads, tags and
/// weaves are keyed by NAME on disk, so a rename is a new file; this is the
/// identity that survives one.
///
/// The bits come from `RandomState`, std's own hasher seed, because this crate
/// takes no dependencies and every `HashMap` in the core already seeds one. Not
/// cryptographic and it need not be — an id is only a label to match two copies
/// of the same object by. It does need not to collide, and doesn't: std draws
/// the seed once per thread and bumps it per `RandomState`.
pub fn new_id() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::with_capacity(32);
    for _ in 0..2 {
        let mut h = RandomState::new().build_hasher();
        h.write(b"plumbline-id");
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// Resolve duplicate ids among loaded objects: where two files carry the same
/// `id`, keep the one with the newer `updated` and drop the other from memory
/// only. Two files with one id is a rename artifact — the rename wrote a new
/// slug, and a copy of the old file arrived from a backup zip.
///
/// Load never deletes: the stale *file* goes only on the next explicit save,
/// through the atomic writer. A deleting loader would turn a misparse, a
/// half-restored backup or a clock skew into permanent data loss.
///
/// `updated` is the frozen `YYYY-MM-DDThh:mm:ssZ` UTC form, so a plain string
/// compare orders it; an object without one sorts oldest. Ties keep the earlier
/// item, leaving the caller's own deterministic order — by path — deciding.
pub(crate) fn resolve_duplicate_ids<T>(
    items: Vec<T>,
    id_of: impl Fn(&T) -> Option<&str>,
    updated_of: impl Fn(&T) -> Option<&str>,
) -> Vec<T> {
    use std::collections::HashMap;
    // Winner per id, by index, decided before anything is moved.
    let mut best: HashMap<&str, usize> = HashMap::new();
    for (i, it) in items.iter().enumerate() {
        let Some(id) = id_of(it) else { continue };
        match best.get(id) {
            Some(&j) if updated_of(&items[j]).unwrap_or("") >= updated_of(it).unwrap_or("") => {}
            _ => {
                best.insert(id, i);
            }
        }
    }
    let keep: std::collections::HashSet<usize> = best.into_values().collect();
    items.into_iter().enumerate().filter(|(i, it)| id_of(it).is_none() || keep.contains(i)).map(|(_, it)| it).collect()
}

/// Move an unparseable file to `<name>.bad` before anything writes over it, so
/// the reader has something to fix by hand instead of losing their config or a
/// note to one bad byte. `.bad` is user data: it persists and rides along in the
/// backup zip, which is why `is_temp_name` does not match it.
///
/// An existing `.bad` is kept and the new damage left where it is — a second
/// failure is nearly always the same damage read and saved back out, and
/// numbered `.bad.2` files would pile up in a directory nobody prunes.
///
/// Best-effort: an empty (truncated) file has nothing to recover, so it does not
/// spend the single slot real damage will need, and a rename that fails still
/// loads as the default.
pub fn move_damaged_aside(path: &Path, bytes: &[u8]) {
    if bytes.trim_ascii().is_empty() {
        return;
    }
    let mut name = match path.file_name() {
        Some(n) => n.to_os_string(),
        None => return,
    };
    name.push(".bad");
    let bad = path.with_file_name(name);
    if bad.exists() {
        return;
    }
    let _ = std::fs::rename(path, &bad);
}

fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io { path: path.display().to_string(), source }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("plumbline-store-{}-{tag}", std::process::id()))
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

    /// A minimal valid thread file, named so a test can tell two apart.
    fn thread_json(name: &str) -> String {
        format!(
            r#"{{"format":"overlay-thread-v1","name":"{name}","tokenization":"kjv1769-tok2","notes":"","entries":[],"created":"2026-07-29T00:00:00Z"}}"#
        )
    }

    /// Pinned to the writer, not to a string literal: a change to
    /// `temp_sibling`'s shape reddens here rather than quietly leaving every
    /// stranded file unrecognised.
    #[test]
    fn every_name_the_writer_mints_is_recognised() {
        for target in [
            "threads/romans-road.json",
            "notes/Gen.1.7.json",
            ".config/plumbline/config.json",
            "data/kjv.jsonl.idxcache",
            "out", // no extension at all
        ] {
            let tmp = temp_sibling(Path::new(target));
            let name = tmp.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                is_temp_name(&name),
                "temp_sibling minted {name}, which the rule does not recognise — \
                 a stranded one would ride into every backup"
            );
        }
    }

    /// The failure that costs real data: a rule wide enough to swallow something
    /// the reader owns. Every name here must survive.
    #[test]
    fn nothing_the_reader_owns_looks_like_a_temp() {
        for kept in [
            "romans-road.json",          // the ordinary case
            "config.json",               // the reader's settings
            ".config",                   // the authored DIRECTORY those live in
            "config.json.bad",           // the rescue copy of damaged settings
            ".mine.json",                // a dotted name from someone else's archive
            "notes.tmp",                 // ends in .tmp, no leading dot
            ".summer.tmp",               // dotted and .tmp, but no discriminator
            ".summer.json.tmp",          // ditto, and the stem has its own dot
            ".summer.json.v2.tmp",       // discriminator-shaped, but not digits
            ".tmp",                      // no stem, no discriminator
            ".1.tmp",                    // a discriminator with no stem
            "romans-road.json.4242.tmp", // temp-shaped but not dotted
        ] {
            assert!(!is_temp_name(kept), "{kept} is the reader's, and the rule would drop it");
        }

        for dropped in [".romans-road.json.4242.tmp", ".config.json.0.tmp", ".out.9.tmp"] {
            assert!(is_temp_name(dropped), "{dropped} is a stranded temp and must be dropped");
        }
    }

    /// A stranded temp is invisible to the loader, and silently so: a load error
    /// is how the reader hears about a file that is genuinely theirs and broken.
    #[test]
    fn a_stranded_temp_is_not_enumerated() {
        let home = scratch("stranded");
        let _ = fs::remove_dir_all(&home);
        let threads = home.join("threads");
        write_atomic(threads.join("romans-road.json"), &thread_json("Romans Road")).unwrap();
        // Exactly what a kill between write and rename leaves behind.
        write_atomic(threads.join(".romans-road.json.4242.tmp"), &thread_json("Stranded copy")).unwrap();
        // And a dotted name that is NOT ours, which must still load.
        write_atomic(threads.join(".mine.json"), &thread_json("Mine")).unwrap();

        let (loaded, errors) = crate::thread::load_threads(&home);
        let names: Vec<&str> = loaded.iter().map(|lt| lt.thread.name.as_str()).collect();
        assert_eq!(names, ["Mine", "Romans Road"], "a stranded temp was enumerated as a thread");
        assert!(errors.is_empty(), "the temp must not even be read: {errors:?}");

        let _ = fs::remove_dir_all(&home);
    }

    /// What a backup walk must ship. Modelled here rather than in the shell:
    /// the shell walks its own tree, but the rule is the part that has to be right.
    #[test]
    fn a_stranded_temp_does_not_reach_a_backup() {
        let home = scratch("backup");
        let _ = fs::remove_dir_all(&home);
        for (rel, body) in [
            ("threads/romans-road.json", thread_json("Romans Road")),
            ("threads/.romans-road.json.4242.tmp", thread_json("Stranded copy")),
            ("threads/.mine.json", thread_json("Mine")),
            ("threads/summer.tmp", "not ours to judge".to_string()),
            (".config/plumbline/config.json", "{}".to_string()),
            (".config/plumbline/config.json.bad", "{ not json".to_string()),
        ] {
            write_atomic(home.join(rel), &body).unwrap();
        }

        let mut shipped = walk_for_backup(&home, &home);
        shipped.sort();
        assert_eq!(
            shipped,
            [
                ".config/plumbline/config.json",
                ".config/plumbline/config.json.bad",
                "threads/.mine.json",
                "threads/romans-road.json",
                "threads/summer.tmp",
            ],
            "the backup shipped the wrong set — a temp got in, or the reader's own went missing"
        );

        let _ = fs::remove_dir_all(&home);
    }

    /// Home-relative paths a backup would carry, temps excluded by the rule.
    /// Directory names are never tested: only files are minted as temps, and
    /// testing directories is how a loose rule eats `.config`.
    fn walk_for_backup(root: &Path, dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk_for_backup(root, &path));
            } else if !is_temp_name(&entry.file_name().to_string_lossy()) {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push(rel);
            }
        }
        out
    }

    #[test]
    fn slugging() {
        assert_eq!(slug("Romans Road", "thread"), "romans-road");
        assert_eq!(slug("  A Priest, after the Order! ", "thread"), "a-priest-after-the-order");
        assert_eq!(slug("", "tag"), "tag");
        assert_eq!(slug("!!!", "tag"), "tag");
    }

    /// An id is 32 lowercase hex chars and never repeats. The shape goes on disk
    /// in a frozen format; a collision would make the duplicate resolution below
    /// discard a tag the reader still has.
    #[test]
    fn every_id_is_thirty_two_lowercase_hex_and_distinct() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..20_000 {
            let id = new_id();
            assert_eq!(id.len(), 32, "not 32 chars: {id}");
            assert!(id.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "not lowercase hex: {id}");
            assert!(seen.insert(id.clone()), "minted {id} twice");
        }
    }

    /// A tie on `updated` keeps the earlier item, so the caller's own order — by
    /// path, for every loader here — decides, and the same two files resolve the
    /// same way on every launch.
    #[test]
    fn a_tie_on_updated_keeps_the_earlier_item() {
        let items = vec![
            ("first", Some("x"), Some("2026-08-01T00:00:00Z")),
            ("second", Some("x"), Some("2026-08-01T00:00:00Z")),
        ];
        let kept = resolve_duplicate_ids(items, |i| i.1, |i| i.2);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].0, "first");
    }

    /// An object with no `updated` is the older one, whichever side of the pair
    /// it is on.
    #[test]
    fn a_missing_updated_sorts_oldest() {
        for (order, want) in [(0, "stamped"), (1, "stamped")] {
            let stamped = ("stamped", Some("x"), Some("2026-08-01T00:00:00Z"));
            let bare = ("bare", Some("x"), None);
            let items = if order == 0 { vec![stamped, bare] } else { vec![bare, stamped] };
            let kept = resolve_duplicate_ids(items, |i| i.1, |i| i.2);
            assert_eq!(kept.len(), 1);
            assert_eq!(kept[0].0, want, "order {order}");
        }
    }

    /// A reader never sees half a file: a save is one `rename`, so a concurrent
    /// read gets the old contents or the new and never a prefix of either. One
    /// thread rewrites the path between two very different bodies while another
    /// reads as fast as it can; both are far past a page, so a truncate-then-write
    /// writer (what `fs::write` does) reddens this with a short read.
    #[test]
    fn a_concurrent_reader_never_sees_a_partial_file() {
        let dir = scratch("atomicity");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("notes").join("john-3-16.json");
        let a = format!("{{\"a\":\"{}\"}}", "a".repeat(200_000));
        let b = format!("{{\"b\":\"{}\"}}", "b".repeat(300_000));
        write_atomic(&path, &a).unwrap();

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let (path, stop, a, b) = (path.clone(), stop.clone(), a.clone(), b.clone());
            std::thread::spawn(move || {
                let mut reads = 0u32;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    // A failed read is not the failure under test (Windows can deny
                    // one mid-rename); a read that succeeds with wrong bytes is.
                    if let Ok(got) = fs::read_to_string(&path) {
                        assert!(
                            got == a || got == b,
                            "a reader saw {} bytes — neither body is that long, so it caught a write in progress",
                            got.len()
                        );
                        reads += 1;
                    }
                }
                reads
            })
        };

        for i in 0..60 {
            write_atomic(&path, if i % 2 == 0 { &b } else { &a }).unwrap();
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let reads = reader.join().expect("the reader saw a partial file");
        assert!(reads > 0, "the reader never managed a read, so this proved nothing");

        let _ = fs::remove_dir_all(&dir);
    }

    /// A process killed between the temp write and the rename leaves the temp
    /// behind and the target exactly as it was, so the reader's previous note is
    /// undamaged by a save that never finished.
    #[test]
    fn an_interrupted_write_leaves_the_previous_file_whole() {
        let dir = scratch("interrupted");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("threads").join("romans-road.json");
        write_atomic(&path, &thread_json("Romans Road")).unwrap();

        // The state a kill between the two steps leaves: new contents in the
        // sibling, the rename never reached.
        let tmp = temp_sibling(&path);
        fs::write(&tmp, "{\"format\":\"overlay-thread-v1\",\"name\":\"half written").unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            thread_json("Romans Road"),
            "the interrupted write damaged the file it was replacing"
        );
        assert!(
            is_temp_name(&tmp.file_name().unwrap().to_string_lossy()),
            "the leftover is not recognised as a temp, so it would ride into every backup"
        );

        // The next save completes over the top of it, leaving the finished file
        // plus one stranded sibling for the enumerators to skip.
        write_atomic(&path, &thread_json("Romans Road again")).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), thread_json("Romans Road again"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// `config.rs` and `usernote.rs` share one rule: damaged bytes move aside
    /// once, and a second failure does not spend the first one's slot.
    #[test]
    fn damaged_bytes_are_set_aside_once() {
        let dir = scratch("aside");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let bad = dir.join("config.json.bad");

        fs::write(&path, "{ not json").unwrap();
        move_damaged_aside(&path, b"{ not json");
        assert_eq!(fs::read_to_string(&bad).unwrap(), "{ not json");

        // A second, different failure leaves the first rescue alone.
        fs::write(&path, "also not json").unwrap();
        move_damaged_aside(&path, b"also not json");
        assert_eq!(fs::read_to_string(&bad).unwrap(), "{ not json", "the first rescue was overwritten");

        // An empty file has nothing to recover, so it does not spend the slot.
        let empty_dir = scratch("aside-empty");
        let _ = fs::remove_dir_all(&empty_dir);
        fs::create_dir_all(&empty_dir).unwrap();
        let empty = empty_dir.join("config.json");
        fs::write(&empty, "   ").unwrap();
        move_damaged_aside(&empty, b"   ");
        assert!(!empty_dir.join("config.json.bad").exists(), "an empty file spent the rescue slot");

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&empty_dir);
    }
}
