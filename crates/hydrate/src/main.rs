//! `plumbline-hydrate` — assemble and verify the Plumbline data pack into a home.
//!
//! The reader needs only `kjv.jsonl` + `strongs.json` (+ optional notes); the
//! R&D tier adds `cross-references.tsv`, `concept-vectors.vec` (with its `.meta`
//! / `.freq` sidecars), and `morphology.jsonl`. These are produced once by the
//! offline pipeline (see `data-prep/README.md`) — this tool does not generate
//! them; it **places** them into a home and **verifies** each by actually
//! loading it through the same code the app uses, so "will this light up?" is
//! answered concretely rather than by guessing from file presence.
//!
//! Usage:
//!   plumbline-hydrate check [--home <dir>]        # inspect a home (default: resolved)
//!   plumbline-hydrate copy  --from <dir> [--to <dir>]   # copy the pack, then verify
//!
//! All paths join cross-platform; copies create the target `data/` as needed.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use plumbline_core::canon::TOKENIZATION_VERSION;
use plumbline_core::{corpus, crossref, home, notes, strongs};
use plumbline_rnd::{bridge, embed, morph};

/// The pack files, relative to a home. Core files gate the reader; the R&D
/// files are optional tiers. Sidecars ride with their primary file.
const CORE_FILES: &[(&str, bool)] = &[
    ("data/kjv.jsonl", true),
    ("data/strongs.json", true),
    ("data/kjv-notes.jsonl", false),
];
const RND_FILES: &[&str] = &[
    "data/cross-references.tsv",
    "data/concept-vectors.vec",
    "data/concept-vectors.vec.meta",
    "data/concept-vectors.vec.freq",
    "data/morphology.jsonl",
    // Fused bridge: committed external witnesses + fitted trust priors, plus the
    // optional hydrated/harvested source files.
    "bridge/abbott-smith.json",
    "bridge/lxx-alignment.json",
    "bridge/stepbible-tipnr.json",
    "data/source-priors.json",
    "data/bridge-sources.json",
    "data/quotation-pairs.json",
    // The graded text-as-witness (its gate keeps it silent until qualified).
    "data/text-witness.json",
];
/// Authored / seed content copied as whole directories. The overlay home keeps
/// the user's weaves (parallel passages), threads, tags, the suggested-weave
/// review queue, and patches here; seeding them means a fresh Plumbline home
/// opens with the same study aids instead of an empty reader.
const USER_DIRS: &[&str] = &["weaves", "suggested", "threads", "tags", "patches"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("check");
    let flag = |name: &str| -> Option<PathBuf> {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(PathBuf::from)
    };

    match cmd {
        "check" => {
            let home = flag("--home").unwrap_or_else(resolve_home);
            check(&home)
        }
        "copy" => {
            let Some(from) = flag("--from") else {
                eprintln!("copy needs --from <dir> (a home holding data/…)");
                return ExitCode::from(2);
            };
            let to = flag("--to").or_else(|| flag("--home")).unwrap_or_else(resolve_home);
            match copy(&from, &to) {
                Ok(()) => check(&to),
                Err(e) => {
                    eprintln!("hydrate copy failed: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "web-cache" => {
            let Some(data) = flag("--data") else {
                eprintln!("web-cache needs --data <kjv.jsonl> [--out <file>]");
                return ExitCode::from(2);
            };
            let out = flag("--out").unwrap_or_else(|| {
                let mut s = data.as_os_str().to_os_string();
                s.push(".idxcache");
                PathBuf::from(s)
            });
            // Stamped mtime 0: the browser WASI shim reports 0 for every file,
            // so this cache validates on the web's very first boot (native
            // runtimes see their real mtime and correctly ignore it).
            match corpus::build_cache_stamped(&data, &out, 0) {
                Ok(()) => {
                    println!("wrote {} (web-stamped idxcache)", out.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("web-cache failed: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "help" | "-h" | "--help" => {
            print!("{}", HELP);
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command '{other}'. Try: check | copy | web-cache | help");
            ExitCode::from(2)
        }
    }
}

const HELP: &str = "\
plumbline-hydrate — assemble + verify the Plumbline data pack

  plumbline-hydrate check [--home <dir>]
      Inspect a home and report which tiers will light up.

  plumbline-hydrate copy --from <dir> [--to <dir>]
      Copy the pack from <dir>/data into <dir-to>/data, seed the authored
      dirs (weaves/threads/tags/suggested/patches) without overwriting any
      existing files there, then verify.
      --to defaults to the resolved home (env / working tree / user data dir).

  plumbline-hydrate web-cache --data <kjv.jsonl> [--out <file>]
      Parse the corpus and write its idxcache stamped for the web shell
      (mtime 0 — what the browser WASI shim reports), so the PWA's first
      boot skips the ~19 MB re-parse. Run by scripts/build-web-pack.mjs.

The R&D artifacts are produced offline (see data-prep/README.md); this tool
places and checks them, it does not train or generate.
";

fn resolve_home() -> PathBuf {
    match home::resolve_home() {
        Some((p, _)) => p,
        None => PathBuf::from("."),
    }
}

/// Copy every pack file that exists under `from` into `to`, creating `to/data`.
///
/// Pack files land atomically (temp sibling → rename, mirroring
/// `plumbline_core::store::write_atomic`) so a crash mid-copy can never leave a
/// truncated file where a good one stood. The authored dirs are seeded without
/// ever overwriting an existing file — those hold the user's own study aids.
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to.join("data"))?;
    // Refuse to copy a home onto itself: `fs::copy` onto the source truncates
    // it, so this would silently destroy the pack. Canonicalize so different
    // spellings of the same directory are caught too.
    if let (Ok(f), Ok(t)) = (from.canonicalize(), to.canonicalize()) {
        if f == t {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "--from {} and --to {} are the same directory; refusing to copy a home onto itself",
                    from.display(),
                    to.display()
                ),
            ));
        }
    }
    let all: Vec<&str> =
        CORE_FILES.iter().map(|(r, _)| *r).chain(RND_FILES.iter().copied()).collect();
    let mut copied = 0usize;
    for rel in all {
        let src = from.join(rel);
        if src.is_file() {
            let dst = to.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let bytes = copy_atomic(&src, &dst)?;
            println!("  copied {rel} ({})", human(bytes));
            copied += 1;
        }
    }
    // Seed the authored-content directories (weaves/threads/tags/…) so the home
    // opens with its study aids, not empty — but never clobber what the user
    // already has there.
    for dir in USER_DIRS {
        let src = from.join(dir);
        if src.is_dir() {
            let (n, kept) = seed_dir_all(&src, &to.join(dir), dir)?;
            if n > 0 {
                println!("  copied {dir}/ ({n} file(s))");
                copied += n;
            }
            if kept > 0 {
                println!("  kept {kept} existing file(s) in {dir}/ (yours; not overwritten)");
            }
        }
    }
    println!("copied {copied} file(s) from {} → {}\n", from.display(), to.display());
    Ok(())
}

/// Copy `src` over `dst` atomically: stream into a hidden temp sibling in the
/// destination directory, fsync, close, then rename into place — the same
/// dance as `plumbline_core::store::write_atomic` (a rename within one directory is
/// atomic on Unix and Windows, so a crash mid-copy never leaves a truncated
/// pack file behind). Returns the number of bytes copied.
fn copy_atomic(src: &Path, dst: &Path) -> std::io::Result<u64> {
    let name = dst
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_string());
    let tmp = dst.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut reader = std::fs::File::open(src)?;
        let bytes = {
            let mut writer = std::fs::File::create(&tmp)?;
            let n = std::io::copy(&mut reader, &mut writer)?;
            writer.sync_all()?;
            n
            // writer drops (closes) here — Windows will not replace a file
            // that still has an open handle.
        };
        std::fs::rename(&tmp, dst)?;
        Ok(bytes)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp); // best-effort cleanup on failure
    }
    result
}

/// Recursively seed `src` into `dst` without ever overwriting an existing file
/// (these directories hold user-authored content). Cross-platform:
/// `create_dir_all` + atomic file copy, `Path::join` for every path; `rel` is
/// the destination-relative prefix used for messages. Returns
/// `(copied, skipped)` counts, printing each skipped file so nothing is
/// silently left stale.
fn seed_dir_all(src: &Path, dst: &Path, rel: &str) -> std::io::Result<(usize, usize)> {
    std::fs::create_dir_all(dst)?;
    let (mut copied, mut skipped) = (0, 0);
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let (s, d) = (entry.path(), dst.join(entry.file_name()));
        let r = format!("{rel}/{}", entry.file_name().to_string_lossy());
        if ft.is_dir() {
            let (c, k) = seed_dir_all(&s, &d, &r)?;
            copied += c;
            skipped += k;
        } else if ft.is_file() {
            if d.exists() {
                println!("  kept   {r} (already exists; not overwritten)");
                skipped += 1;
            } else {
                copy_atomic(&s, &d)?;
                copied += 1;
            }
        }
    }
    Ok((copied, skipped))
}

/// Load each tier through the real code and report status. Exit FAILURE only if
/// a required core file is missing or unreadable.
fn check(home: &Path) -> ExitCode {
    println!("home: {}", home.display());
    println!("tokenization: {TOKENIZATION_VERSION}\n");
    let data = home.join("data");
    let mut core_ok = true;

    // ── reader core ──────────────────────────────────────────────────────────
    println!("Reader (core):");
    match corpus::load_corpus(data.join("kjv.jsonl")) {
        Ok(c) => println!("  ✓ kjv.jsonl — {} verses", c.verses().len()),
        Err(e) => {
            println!("  ✗ kjv.jsonl — {e}");
            core_ok = false;
        }
    }
    match strongs::load_strongs(data.join("strongs.json")) {
        Ok(d) => println!("  ✓ strongs.json — {} entries", d.len()),
        Err(e) => {
            println!("  ✗ strongs.json — {e}");
            core_ok = false;
        }
    }
    match notes::load_notes(data.join("kjv-notes.jsonl")) {
        Ok(n) if !n.is_empty() => println!("  ✓ kjv-notes.jsonl — {} verses with notes", n.len()),
        _ => println!("  · kjv-notes.jsonl — absent (margin notes off)"),
    }

    // ── R&D tiers ──────────────────────────────────────────────────────────────
    println!("\nR&D tiers (Full study):");

    // Fused bridge: etymology (from strongs.json) + external witnesses + priors.
    match strongs::load_strongs(data.join("strongs.json")) {
        Ok(d) => {
            let ety = bridge::Bridge::from_etymology(&d).len();
            let fused = bridge::FusedBridge::build(&d, home);
            let ext = fused.source_link_count();
            if ext == 0 {
                println!("  ✓ etymology bridge — {ety} codes linked (from strongs.json; no external sources)");
            } else {
                println!("  ✓ fused bridge — {ety} etymology codes + {ext} external source links (bridge/*.json + priors)");
            }
        }
        Err(_) => println!("  · etymology bridge — needs strongs.json"),
    }

    let xr = crossref::load_cross_refs(data.join("cross-references.tsv"));
    if xr.is_empty() {
        println!("  · cross-references.tsv — absent (TSK tier off)");
    } else {
        println!("  ✓ cross-references.tsv — {} refs over {} verses", crossref::xref_count(&xr), xr.len());
    }

    match embed::load_embedding(TOKENIZATION_VERSION, data.join("concept-vectors.vec")) {
        Some(e) => println!(
            "  ✓ concept-vectors.vec — {} vectors, dim {}, {}, {}",
            e.size(),
            e.dim(),
            if e.aligned() { "aligned (cross-testament on)" } else { "unaligned" },
            if e.has_trained_freq() { "trained freq" } else { "no freq" },
        ),
        None => println!("  · concept-vectors.vec — absent or stale (concept neighbours / verses-like-this off)"),
    }

    match morph::load_morph(TOKENIZATION_VERSION, data.join("morphology.jsonl")) {
        Some(m) => println!("  ✓ morphology.jsonl — {} verses annotated", m.verse_count()),
        None => println!("  · morphology.jsonl — absent or stale (morphology off)"),
    }

    println!();
    if core_ok {
        println!("Reader is hydrated. ✓");
        ExitCode::SUCCESS
    } else {
        println!("Reader is NOT hydrated — supply the core files (see data-prep/README.md).");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        // A unique-per-process scratch dir under the OS temp dir (portable).
        std::env::temp_dir().join(format!("plumbline-hydrate-{}-{tag}", std::process::id()))
    }

    #[test]
    fn rejects_copy_onto_self() {
        let dir = scratch("selfcopy");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let err = copy(&dir, &dir).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

        // Different spellings of the same directory are caught too.
        let err = copy(&dir.join("."), &dir).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn seeding_never_overwrites_user_files() {
        let root = scratch("seed");
        let _ = fs::remove_dir_all(&root);
        let (from, to) = (root.join("from"), root.join("to"));

        // Pack file: should be (atomically) replaced on re-copy.
        fs::create_dir_all(from.join("data")).unwrap();
        fs::write(from.join("data").join("kjv.jsonl"), "new pack").unwrap();
        fs::create_dir_all(to.join("data")).unwrap();
        fs::write(to.join("data").join("kjv.jsonl"), "old pack").unwrap();

        // Authored dir: existing files are the user's — keep them.
        fs::create_dir_all(from.join("weaves")).unwrap();
        fs::write(from.join("weaves").join("a.json"), "seeded").unwrap();
        fs::write(from.join("weaves").join("b.json"), "fresh").unwrap();
        fs::create_dir_all(to.join("weaves")).unwrap();
        fs::write(to.join("weaves").join("a.json"), "mine").unwrap();

        copy(&from, &to).unwrap();

        assert_eq!(fs::read_to_string(to.join("data").join("kjv.jsonl")).unwrap(), "new pack");
        assert_eq!(fs::read_to_string(to.join("weaves").join("a.json")).unwrap(), "mine");
        assert_eq!(fs::read_to_string(to.join("weaves").join("b.json")).unwrap(), "fresh");

        // No temp files left behind by the atomic copies.
        let leftovers: Vec<_> = fs::read_dir(to.join("data"))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files should be gone");

        let _ = fs::remove_dir_all(&root);
    }
}

/// Human-readable byte size.
fn human(bytes: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}
