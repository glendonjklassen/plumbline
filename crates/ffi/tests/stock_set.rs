//! The shipped stock study set has to be loadable by the build that ships it.
//!
//! `apps/android/app/src/main/assets/stock/{tags,threads,weaves}` is the one
//! source of truth for the bundled study aids (CLAUDE.md §Layout): Android
//! copies it out of its APK assets on first launch, and
//! `scripts/build-web-pack.mjs` packs the same directory into the web data pack
//! with `seedOnce`. Nothing validated it. A stock file with a typo in a refKey,
//! a stale tokenization stamp, or a format tag the core does not recognise would
//! ship in both shells and fail at SEED time — on a reader's device, on first
//! launch, where the only symptom is a study aid that quietly is not there.
//!
//! These tests parse every shipped file through the core's own `Deserialize`
//! impls, which is what the shells do, so "it is valid JSON" is not enough to
//! pass: the format tag, the tokenization stamp and every refKey have to be
//! right.
//!
//! HONEST LIMIT: refs are checked STRUCTURALLY — the book is one of the 66 and
//! the chapter and verse are sane — not against the corpus, because the canon
//! table carries no chapter counts and loading `data/kjv.jsonl` here would put a
//! 19 MB parse on every `cargo test`. So `Jude 1:3` is verified to be a real
//! reference shape naming a real book; `Jude 1:99` would pass this and fail on a
//! device. Closing that needs the corpus, and belongs with whatever else earns
//! its cost.

use std::path::{Path, PathBuf};

use plumbline_core::canon::{book_by_id, TOKENIZATION_VERSION};
use plumbline_core::reference::VRef;

fn stock() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/android/app/src/main/assets/stock")
}

/// Every `*.json` under `dir`, recursively (weaves has a `suggested/` subtree).
fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(json_files(&p));
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// A refKey names one of the 66 books, with a sane chapter and verse.
fn check_ref(key: &str, at: &Path) {
    let r = VRef::parse_ref_key(key)
        .unwrap_or_else(|| panic!("{}: `{key}` is not a refKey the core can parse", at.display()));
    assert!(
        book_by_id(&r.book).is_some(),
        "{}: `{key}` names `{}`, which is not one of the 66 OSIS book ids",
        at.display(),
        r.book,
    );
    assert!(r.chapter >= 1 && r.verse >= 1, "{}: `{key}` has a zero chapter or verse", at.display(),);
}

#[test]
fn every_stock_tag_loads() {
    let dir = stock().join("tags");
    let files = json_files(&dir);
    // Not merely "whatever is there parses": there IS a stock tag, because it is
    // the only example a reader ever sees of what a tag is for, and the set
    // shipped with none until 2026-07-29.
    assert!(!files.is_empty(), "no stock tag ships in {}", dir.display());
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let tag: plumbline_core::tag::Tag = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{}: the core cannot load this tag: {e}", f.display()));
        assert_eq!(
            tag.tok_version,
            TOKENIZATION_VERSION,
            "{}: tokenization stamp is stale — a loader refuses a mismatch",
            f.display(),
        );
        assert!(!tag.name.trim().is_empty(), "{}: a tag with no name", f.display());
        assert!(!tag.members.is_empty(), "{}: a tag with no members", f.display());
        for m in &tag.members {
            if let plumbline_core::tag::TagTarget::Verse(v) = &m.target {
                check_ref(&v.ref_key(), f);
            }
        }
    }
}

#[test]
fn every_stock_thread_loads() {
    for f in json_files(&stock().join("threads")) {
        let bytes = std::fs::read(&f).unwrap();
        let t: plumbline_core::thread::Thread = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{}: the core cannot load this thread: {e}", f.display()));
        assert_eq!(t.tok_version, TOKENIZATION_VERSION, "{}: tokenization stamp is stale", f.display(),);
        for e in &t.entries {
            check_ref(&e.vref.ref_key(), &f);
        }
    }
}

#[test]
fn every_stock_weave_loads() {
    let files = json_files(&stock().join("weaves"));
    // 222 weaves and one suggested subtree at the time of writing; the count is
    // deliberately not asserted, only that the set is not empty and every member
    // of it loads.
    assert!(!files.is_empty(), "no stock weaves ship");
    for f in &files {
        let bytes = std::fs::read(f).unwrap();
        let w: plumbline_core::weave::Weave = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{}: the core cannot load this weave: {e}", f.display()));
        assert_eq!(w.tok_version, TOKENIZATION_VERSION, "{}: tokenization stamp is stale", f.display(),);
        assert!(!w.links.is_empty(), "{}: a weave with no links", f.display());
        for l in &w.links {
            check_ref(&l.a.ref_key(), f);
            check_ref(&l.b.ref_key(), f);
        }
    }
}

#[test]
fn a_stock_file_lands_where_its_shell_will_look_for_it() {
    // The filename is not decoration: both shells seed by copying the file, and
    // the core then finds a tag again by slugging its NAME. A stock file whose
    // name does not slug to its filename is seeded once and then invisible to
    // every later save, which would silently fork it in two.
    for f in json_files(&stock().join("tags")) {
        let bytes = std::fs::read(&f).unwrap();
        let tag: plumbline_core::tag::Tag = serde_json::from_slice(&bytes).unwrap();
        let expected = format!("{}.json", plumbline_core::store::slug(&tag.name, "tag"));
        assert_eq!(
            f.file_name().unwrap().to_str().unwrap(),
            expected,
            "{}: a tag named {:?} must be stored as {expected}",
            f.display(),
            tag.name,
        );
    }
}
