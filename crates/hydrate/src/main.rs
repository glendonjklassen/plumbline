//! `pure-hydrate` — assemble and verify the pure-study data pack into a home.
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
//!   pure-hydrate check [--home <dir>]        # inspect a home (default: resolved)
//!   pure-hydrate copy  --from <dir> [--to <dir>]   # copy the pack, then verify
//!
//! All paths join cross-platform; copies create the target `data/` as needed.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use pure_core::canon::TOKENIZATION_VERSION;
use pure_core::{corpus, crossref, home, notes, strongs};
use pure_rnd::{bridge, embed, morph};

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
];

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
        "help" | "-h" | "--help" => {
            print!("{}", HELP);
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command '{other}'. Try: check | copy | help");
            ExitCode::from(2)
        }
    }
}

const HELP: &str = "\
pure-hydrate — assemble + verify the pure-study data pack

  pure-hydrate check [--home <dir>]
      Inspect a home and report which tiers will light up.

  pure-hydrate copy --from <dir> [--to <dir>]
      Copy the pack from <dir>/data into <dir-to>/data, then verify.
      --to defaults to the resolved home (env / working tree / user data dir).

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
fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to.join("data"))?;
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
            let bytes = std::fs::copy(&src, &dst)?;
            println!("  copied {rel} ({})", human(bytes));
            copied += 1;
        }
    }
    println!("copied {copied} file(s) from {} → {}\n", from.display(), to.display());
    Ok(())
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
