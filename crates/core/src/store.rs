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
//!
//! A rename that fails takes its temp file with it, but a process *killed*
//! between the write and the rename cannot: it leaves a stranded temp sibling in
//! an authored directory. This module mints those names, so it also owns the rule
//! for spotting one — [`is_temp_name`], which every enumerator over the reader's
//! directories should consult.

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
    let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "out".to_string());
    let tmp_name = format!(".{name}.{}.tmp", temp_discriminator());
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

/// Whether `name` is one of the names [`temp_sibling`] mints — the leftover of
/// an atomic write whose process died between the write and the rename, or whose
/// rename AND its best-effort cleanup both failed.
///
/// THE RULE, and it is deliberately narrow: `.<name>.<digits>.tmp`. All three
/// parts must be there — the leading dot, an all-ASCII-digit discriminator
/// segment, and the `.tmp` suffix. Only [`temp_sibling`] mints that shape (and
/// Android's `writeThroughTemp`, deliberately the same shape); nothing either
/// shell derives from a name the reader typed can, because [`slug`] maps every
/// non-alphanumeric to a separator and so cannot produce a dot at all.
///
/// Narrow because the loose versions delete the reader's own work:
///
/// * "starts with a dot" takes `.config`, a legitimate authored directory — it
///   is where `config.json` lives on the web;
/// * "ends with `.tmp`", or "contains it", takes a `notes.tmp` that arrived in
///   a backup zip from somewhere else; a name we do not recognise is not ours
///   to throw away.
///
/// `config.json.bad` — the rescue copy of damaged settings, which must keep
/// riding along in backups — matches none of the three, and neither does a
/// dotted name the reader restored, such as `.mine.json`.
///
/// **Anything enumerating an authored directory should skip these**, whether it
/// is loading, persisting, or building a backup. A temp is not the reader's
/// data: the write that made it either landed under its real name or came back
/// as an error, so its bytes are a duplicate or a fragment. Every loader here
/// already skips them incidentally, by taking only `*.json` — but a temp that
/// reaches a backup zip is restored onto the next device as a permanent
/// fixture, and once there nothing ever removes it.
pub fn is_temp_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.') else { return false };
    let Some(rest) = rest.strip_suffix(".tmp") else { return false };
    // Split off the discriminator only — the stem keeps its own dots, so a
    // stranded `.Gen.1.7.json.4242.tmp` is recognised as readily as `.out.9.tmp`.
    let Some((stem, disc)) = rest.rsplit_once('.') else { return false };
    !stem.is_empty() && !disc.is_empty() && disc.bytes().all(|b| b.is_ascii_digit())
}

/// The unique-per-process part of a temp name: the pid natively. Wasm has no
/// pids (`std::process::id` panics: "no pids on this platform"); a wasm engine
/// instance is single-process by construction, so a monotonic counter gives the
/// same collision-freedom there.
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
/// Matches overlay's `threadFileFor` / `tagFileFor` slugging.
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

/// A fresh object identity: 32 lowercase hex chars (128 bits), per
/// docs/STABLE-IDS.md. Threads, tags and weaves are keyed by NAME on disk, so a
/// rename is a new file and a lost identity; this is the identity that survives
/// one.
///
/// The bits come from `RandomState`, std's own hasher seed, because this crate
/// takes no dependencies and both shipped targets already rely on it: every
/// `HashMap` in the core seeds one, so if OS randomness were unavailable
/// (`random_get` under the browser's WASI shim, `getrandom` on Android) nothing
/// here would run at all. Two hashers give two `u64`.
///
/// **Not cryptographic, and it doesn't need to be.** An id is a label to match
/// two copies of the same object by; nobody is guessing at it, and nothing is
/// authorised by it. What it does need is not to collide, and it doesn't: std
/// draws the seed once per thread and bumps it per `RandomState`, so ids minted
/// in one process differ by construction, and two *devices* collide only if
/// their 128-bit seeds do.
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
/// `id`, keep the one with the newer `updated` and drop the other **from memory
/// only**.
///
/// Two files with one id is the rename artifact docs/STABLE-IDS.md describes —
/// a build that renames an object writes the new slug, and a copy of the old
/// file can still arrive from a backup zip. The reader's newest edit is the one
/// they meant.
///
/// **Load never deletes.** Dropping the stale *file* happens only on the next
/// explicit save of that object, through the atomic writer. A loader that
/// deleted would turn a misparse, a half-restored backup or a clock skew into
/// permanent data loss, and there is no undo on a phone.
///
/// `updated` is the frozen `YYYY-MM-DDThh:mm:ssZ` UTC form, so a plain string
/// compare orders it; an object without one (anything written before ids
/// existed) sorts oldest. Ties keep the earlier item, which leaves the caller's
/// own deterministic order — by path — deciding.
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

/// Move an unparseable file to `<name>.bad` before anything writes over it.
///
/// One bad byte otherwise costs the reader whatever was in it — their reading
/// history, their pane layout and their church in the case of `config.json`, the
/// text of a note in the case of `notes/*.json`. This leaves them a file to fix
/// by hand, and on the web shell both directories are user data, so the rescue is
/// persisted and rides along in the backup zip (`is_temp_name` deliberately does
/// not match `.bad`).
///
/// If a `.bad` is already there we KEEP IT and leave the new damage where it
/// is. The first rescue is the one worth having — a second failure is nearly
/// always the same damage read and saved back out — and numbered `.bad.2`,
/// `.bad.3` files would pile up in a directory nobody ever prunes.
///
/// Best-effort by design: an empty (truncated) file has nothing in it to
/// recover, so it doesn't spend the single slot real damage will need, and a
/// rename we cannot do still loads as the default — there is nothing else we
/// could do about it.
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
        // A unique-per-process scratch dir under the OS temp dir (portable).
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

    /// The rule is pinned to the writer, not to a string literal: if
    /// `temp_sibling` ever changes shape, this reddens rather than quietly
    /// leaving every stranded file unrecognised.
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

    /// A stranded temp beside a real thread is invisible to the loader — and
    /// silently so, because a load error is how the reader hears about a file
    /// that is genuinely theirs and genuinely broken.
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

    /// What a backup walk must ship. Modelled here rather than in a shell,
    /// because the rule is the part that has to be right — Android's
    /// `writeBackupZip` and the web's `collectFiles` each walk their own tree.
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
    /// testing directories is exactly how a loose rule eats `.config`.
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

    /// An id is 32 lowercase hex chars and never repeats. The shape matters
    /// because it goes on disk in a frozen format; the uniqueness matters because
    /// the id IS the identity — two objects sharing one would make the duplicate
    /// resolution below discard a tag the reader still has.
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
    /// path, for every loader here — is what decides. Deterministic beats
    /// arbitrary: the same two files must resolve the same way on every launch.
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

    /// An object with no `updated` at all — anything written before ids existed —
    /// is the older one, whichever side of the pair it is on.
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

    // ── the interrupted write (TODO §I) ──────────────────────────────────────

    /// **A reader never sees half a file.** The whole reason for the temp-sibling
    /// dance is that a save is one `rename`, so a concurrent read gets the old
    /// contents or the new ones and never a prefix of either.
    ///
    /// Driven, not argued: one thread rewrites the same path between two very
    /// different bodies while another reads it as fast as it can. Every read has
    /// to be one whole body. Sizes are far past a page, so a non-atomic writer
    /// (truncate-then-write, which is what `fs::write` does) would be caught —
    /// and it is: swapping the body of `write_atomic` for `fs::write` reddens this
    /// with a short read.
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
                    // A read that fails is not the failure under test (Windows can
                    // deny one mid-rename); a read that SUCCEEDS with the wrong
                    // bytes is.
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

    /// The write that was interrupted: a process killed between the temp write
    /// and the rename leaves the temp behind and **the target exactly as it was**.
    /// The reader's previous note is not damaged by a save that never finished.
    #[test]
    fn an_interrupted_write_leaves_the_previous_file_whole() {
        let dir = scratch("interrupted");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("threads").join("romans-road.json");
        write_atomic(&path, &thread_json("Romans Road")).unwrap();

        // Exactly the state a kill between the two steps leaves: the new contents
        // written to the sibling, the rename never reached.
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

        // And the next save completes over the top of it, leaving nothing but the
        // finished file plus that one stranded sibling for the enumerators to skip.
        write_atomic(&path, &thread_json("Romans Road again")).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), thread_json("Romans Road again"));

        let _ = fs::remove_dir_all(&dir);
    }

    /// The rescue lives here so `config.rs` and `usernote.rs` share one rule:
    /// damaged bytes move aside once, and a second failure does not spend the
    /// slot the first one is using.
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
