//! The cross-reference tier: TSK-derived, vote-ranked topical study links per
//! verse.
//!
//! Ported from overlay `CrossRef.hs`. Hydrated offline into
//! `data/cross-references.tsv` (~344k references covering ~94% of verses;
//! openbible.info, substantially the public-domain Treasury of Scripture
//! Knowledge plus reader votes). This is a *topical* tier — human-curated
//! pointers shown clearly labelled, never auto-blessed into weaves — and
//! involves no ML at all, so it ports as a plain parser.
//!
//! The file is parsed with plain tab-splitting rather than JSON: at 344k rows
//! the difference is startup time you can feel. Absent the file, the index is
//! empty and the reader runs fine (the house "graceful absence" pattern).

use std::collections::HashMap;
use std::path::Path;

use crate::reference::VRef;

/// One reference out of a verse: the target (with an optional range end when
/// the pointer spans verses) and its vote count — the community's ranking of
/// how illuminating the link is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossRef {
    pub to: VRef,
    pub end: Option<VRef>,
    pub votes: i32,
}

/// Source verse → its references, best-voted first.
pub type XRefIx = HashMap<VRef, Vec<CrossRef>>;

/// The standard path under a home.
pub fn cross_refs_path(home: impl AsRef<Path>) -> std::path::PathBuf {
    home.as_ref().join("data").join("cross-references.tsv")
}

/// Parse one TSV row: `from \t to \t range-end-or-empty \t votes`. Comment
/// lines (`#…`) and malformed rows yield `None`.
pub fn parse_xref_line(line: &str) -> Option<(VRef, CrossRef)> {
    if line.starts_with('#') {
        return None;
    }
    // Exactly four tab-separated columns, split without a per-row Vec (this
    // runs ~344k times at load).
    let mut it = line.split('\t');
    let (Some(from), Some(to), Some(end), Some(votes), None) = (it.next(), it.next(), it.next(), it.next(), it.next())
    else {
        return None;
    };
    let from = VRef::parse_ref_key(from)?;
    let to = VRef::parse_ref_key(to)?;
    let votes: i32 = votes.trim().parse().ok()?;
    let end = if end.is_empty() { None } else { Some(VRef::parse_ref_key(end)?) };
    Some((from, CrossRef { to, end, votes }))
}

/// Parse the TSV text into a per-verse index, references sorted by votes
/// (descending, stable) within each verse.
pub fn parse_cross_refs(text: &str) -> XRefIx {
    let mut ix: XRefIx = HashMap::new();
    for line in text.lines() {
        if let Some((from, r)) = parse_xref_line(line) {
            ix.entry(from).or_default().push(r);
        }
    }
    for refs in ix.values_mut() {
        refs.sort_by_key(|r| std::cmp::Reverse(r.votes));
    }
    ix
}

/// The same parse, one bite at a time — for the boot warm.
///
/// [`parse_cross_refs`] is ~344k rows in one call, and the engine worker is the
/// only thread that answers layout, taps and word studies, so for however long it
/// runs the reader's app is unavailable. Measured 2026-07-30 on the maintainer's
/// desktop: **89 ms** for the whole file, against a ~300 ms warm-chunk budget —
/// which on a phone (5–10× slower) is the budget, spent on one phase.
///
/// So the warm feeds it instead: [`feed`](Self::feed) parses `n` lines and
/// returns whether more remain, and [`finish`](Self::finish) does the per-verse
/// vote sort. Same index, same order, in pieces a tap can get between — there is
/// a test that the two agree exactly.
pub struct XRefIxBuilder {
    text: String,
    /// Byte offset of the next unparsed line. A cursor, not a `Lines` iterator,
    /// because the builder is stored between calls and cannot borrow its own text.
    at: usize,
    ix: XRefIx,
}

impl XRefIxBuilder {
    /// A builder over the file at `path`. A missing or unreadable file yields a
    /// builder with nothing to do, which finishes as an empty index — the house
    /// "graceful absence" rule, same as [`load_cross_refs`].
    pub fn from_path(path: impl AsRef<Path>) -> XRefIxBuilder {
        let text = match std::fs::read(path.as_ref()) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(_) => String::new(),
        };
        XRefIxBuilder { text, at: 0, ix: HashMap::new() }
    }

    /// A builder with nothing to parse, for an engine opened without a home —
    /// it finishes as an empty index, which is what that engine had before.
    pub fn empty() -> XRefIxBuilder {
        XRefIxBuilder { text: String::new(), at: 0, ix: HashMap::new() }
    }

    /// Parse up to `n` more lines. `true` while any remain.
    pub fn feed(&mut self, n: usize) -> bool {
        for _ in 0..n {
            if self.at >= self.text.len() {
                return false;
            }
            let rest = &self.text[self.at..];
            let (line, step) = match rest.find('\n') {
                Some(i) => (&rest[..i], i + 1),
                None => (rest, rest.len()),
            };
            // `str::lines` also drops a trailing \r; match it exactly.
            let line = line.strip_suffix('\r').unwrap_or(line);
            if let Some((from, r)) = parse_xref_line(line) {
                self.ix.entry(from).or_default().push(r);
            }
            self.at += step;
        }
        self.at < self.text.len()
    }

    /// Sort each verse's references best-voted first and hand over the index.
    pub fn finish(mut self) -> XRefIx {
        for refs in self.ix.values_mut() {
            refs.sort_by_key(|r| std::cmp::Reverse(r.votes));
        }
        self.ix
    }
}

/// Load the hydrated cross-reference file into a per-verse index. A missing
/// file yields an empty index (not an error) — the reader runs without it.
pub fn load_cross_refs(path: impl AsRef<Path>) -> XRefIx {
    match std::fs::read(path.as_ref()) {
        Ok(bytes) => parse_cross_refs(&String::from_utf8_lossy(&bytes)),
        Err(_) => HashMap::new(),
    }
}

/// Total references across the index.
pub fn xref_count(ix: &XRefIx) -> usize {
    ix.values().map(|v| v.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows_skipping_comments_and_junk() {
        assert_eq!(parse_xref_line("# a comment"), None);
        assert_eq!(parse_xref_line("garbage"), None);
        let (from, r) = parse_xref_line("John 3:16\tRom 5:8\t\t42").unwrap();
        assert_eq!(from, VRef::new("John", 3, 16));
        assert_eq!(r.to, VRef::new("Rom", 5, 8));
        assert_eq!(r.end, None);
        assert_eq!(r.votes, 42);
        // A ranged target.
        let (_, r2) = parse_xref_line("Gen 1:1\tJohn 1:1\tJohn 1:3\t7").unwrap();
        assert_eq!(r2.end, Some(VRef::new("John", 1, 3)));
    }

    #[test]
    fn indexes_and_sorts_by_votes() {
        let tsv = "# header\n\
                   John 3:16\tRom 5:8\t\t5\n\
                   John 3:16\tEph 2:4\t\t20\n\
                   John 3:16\t1John 4:9\t\t12\n\
                   bad line without tabs\n";
        let ix = parse_cross_refs(tsv);
        assert_eq!(xref_count(&ix), 3);
        let refs = &ix[&VRef::new("John", 3, 16)];
        // best-voted first
        assert_eq!(refs[0].to, VRef::new("Eph", 2, 4));
        assert_eq!(refs[1].to, VRef::new("1John", 4, 9));
        assert_eq!(refs[2].to, VRef::new("Rom", 5, 8));
    }

    #[test]
    fn missing_file_is_empty() {
        let ix = load_cross_refs("/no/such/cross-references.tsv");
        assert!(ix.is_empty());
    }

    /// The sliced parse and the one-shot parse must produce the SAME index —
    /// same verses, same references, same vote order. Slicing is a scheduling
    /// change, and a scheduling change that alters the answer is a bug.
    ///
    /// Driven at several slice sizes, including 1 (every line its own bite) and a
    /// size past the end of the file, because the interesting failures are at the
    /// boundaries: a cursor that skips the line after a chunk, or one that
    /// re-parses it and doubles a verse's references.
    #[test]
    fn the_sliced_parse_agrees_with_the_one_shot_parse() {
        // Deliberately awkward: a comment, a blank line, junk, CRLF, a ranged
        // target, votes that need sorting, and NO trailing newline.
        let text = "# TSK\r\nJohn 3:16\tRom 5:8\t\t5\r\n\njunk\nJohn 3:16\tGen 1:1\t\t42\nGen 1:1\tJohn 1:1\tJohn 1:3\t7\nJohn 3:16\t1John 4:9\t\t19";
        let want = parse_cross_refs(text);
        assert_eq!(want.len(), 2, "fixture should hold two source verses");
        assert_eq!(want[&VRef::new("John", 3, 16)].len(), 3);

        for n in [1usize, 2, 3, 7, 1000] {
            let mut b = XRefIxBuilder { text: text.to_string(), at: 0, ix: HashMap::new() };
            let mut rounds = 0;
            while b.feed(n) {
                rounds += 1;
                assert!(rounds < 1000, "feed({n}) never finished — the cursor is not advancing");
            }
            let got = b.finish();
            assert_eq!(got, want, "slice size {n} produced a different index");
        }
    }

    /// A missing file is not an error, in either shape.
    #[test]
    fn an_absent_file_finishes_empty() {
        let mut b = XRefIxBuilder::from_path("/nonexistent/cross-references.tsv");
        assert!(!b.feed(64), "there is nothing to feed");
        assert!(b.finish().is_empty());
    }

    /// Where the sliced parse's time actually goes, and what the worst single
    /// slice costs — the number the slicing exists to hold down.
    /// `cargo test --release -p plumbline-core -- --ignored --nocapture xref_slice_profile`
    #[test]
    #[ignore]
    fn xref_slice_profile() {
        use std::time::Instant;
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = cross_refs_path(&repo);
        if !path.exists() {
            println!("no data pack; skipping");
            return;
        }

        let t = Instant::now();
        let one_shot = load_cross_refs(&path);
        let whole = t.elapsed().as_micros();

        let t = Instant::now();
        let mut b = XRefIxBuilder::from_path(&path);
        let read = t.elapsed().as_micros();
        let (mut worst, mut steps) = (0u128, 0usize);
        loop {
            let t = Instant::now();
            let more = b.feed(2048 * 8);
            worst = worst.max(t.elapsed().as_micros());
            steps += 1;
            if !more {
                break;
            }
        }
        let t = Instant::now();
        let sliced = b.finish();
        let fin = t.elapsed().as_micros();

        assert_eq!(sliced, one_shot, "the profile run disagreed with the one-shot parse");
        println!(
            "xref: one shot {:.1}ms | sliced: read {:.1}ms, {steps} slices, worst {:.1}ms, finish {:.1}ms",
            whole as f64 / 1000.0,
            read as f64 / 1000.0,
            worst as f64 / 1000.0,
            fin as f64 / 1000.0
        );
    }
}
