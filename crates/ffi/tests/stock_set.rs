//! The shipped stock study set has to be loadable by the build that ships it.
//!
//! These fail against a stock file with a bad refKey, a stale tokenization stamp
//! or an unrecognised format tag — defects that otherwise surface only at seed
//! time on a reader's device, as a study aid that quietly is not there. Every
//! file is parsed through the core's own `Deserialize` impls, as the shell does,
//! so "valid JSON" is not enough to pass.
//!
//! Limit: refs are checked structurally (a real book id, sane chapter and verse),
//! not against the corpus — the canon table carries no chapter counts and loading
//! `data/kjv.jsonl` would put a 19 MB parse on every `cargo test`. `Jude 1:99`
//! passes here and fails on a device.

use std::path::{Path, PathBuf};

use plumbline_core::canon::{book_by_id, TOKENIZATION_VERSION};
use plumbline_core::reference::VRef;

fn stock() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../stock")
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
    // Not merely "whatever is there parses": a stock tag has to ship, since it is the
    // only example a reader ever sees of what a tag is for.
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
    // The count is deliberately not asserted, only that the set is not empty and
    // every member of it loads.
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
    // The shell seeds by copying the file, and the core then finds a tag again by
    // slugging its name. A stock file whose name does not slug to its filename is
    // seeded once and then invisible to every later save, forking it in two.
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
