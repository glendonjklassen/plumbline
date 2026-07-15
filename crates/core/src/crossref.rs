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
    let (Some(from), Some(to), Some(end), Some(votes), None) =
        (it.next(), it.next(), it.next(), it.next(), it.next())
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
        refs.sort_by(|a, b| b.votes.cmp(&a.votes));
    }
    ix
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
}
