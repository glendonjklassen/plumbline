//! Concept embeddings: a dense vector per Strong's number, learned offline by
//! `ml/train_concept2vec.py` (skip-gram over the corpus read as sentences of
//! Strong's numbers). This module *loads* that artifact — it does no training —
//! and answers what the symbolic indices cannot: **concepts near this one**
//! (cosine nearest neighbours, surfacing words that share contexts even when
//! they never co-occur), and — via the aligned space — the **cross-testament**
//! bridge (Greek neighbours of a Hebrew concept and vice versa).
//!
//! Ported from overlay `Embed.hs` (the loader + neighbour queries; the SIF
//! verse-similarity model is a later phase). Era-faithful by construction: the
//! vectors are trained only on scripture's own words and their Strong's tags,
//! never the English surface. Everything degrades gracefully — a missing or
//! stale artifact yields `None` and callers fall back to the symbolic layer.
//!
//! The `.vec` is word2vec text format (`<vocab> <dim>` header, then
//! `STRONGS v1 … vd` per row). If `ml/align_hg.py` has run, the Hebrew subspace
//! is already Procrustes-rotated onto the Greek in the stored vectors and the
//! `.meta` records it; the rotation is orthogonal, so within-language cosines
//! are unchanged and cross-language ones become meaningful.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// A loaded embedding: unit-normalised vectors (so cosine is a plain dot
/// product) packed row-major into one flat array (row `i` is the `dim`-slice at
/// `i*dim`) to keep neighbour scans tight.
#[derive(Debug, Clone)]
pub struct Embedding {
    dim: usize,
    keys: Vec<String>,
    ix: HashMap<String, usize>,
    vecs: Vec<f32>,
    aligned: bool,
    freq: Option<HashMap<String, u64>>,
}

#[derive(Deserialize)]
struct EmbedMeta {
    tokenization: String,
    #[serde(default)]
    aligned: Option<String>,
    #[serde(default)]
    aliases: HashMap<String, String>,
}

impl Embedding {
    pub fn dim(&self) -> usize {
        self.dim
    }
    /// Number of concept rows (alias keys share a row and don't add to this).
    pub fn size(&self) -> usize {
        self.keys.len()
    }
    /// Hebrew was Procrustes-rotated onto Greek, so cross-testament queries are
    /// live.
    pub fn aligned(&self) -> bool {
        self.aligned
    }
    /// Whether the trainer's own frequency table shipped alongside (`.freq`).
    pub fn has_trained_freq(&self) -> bool {
        self.freq.is_some()
    }
    /// The trainer's count for a concept, if the `.freq` sidecar was present.
    pub fn freq_of(&self, code: &str) -> Option<u64> {
        self.freq.as_ref().and_then(|f| f.get(code).copied())
    }

    fn row(&self, i: usize) -> &[f32] {
        &self.vecs[i * self.dim..(i + 1) * self.dim]
    }

    /// The unit-length vector for a Strong's number, if present.
    pub fn concept_vector(&self, code: &str) -> Option<&[f32]> {
        self.ix.get(code).map(|&i| self.row(i))
    }

    /// The `k` nearest concepts by cosine among rows passing `keep`, strongest
    /// first, excluding the query row itself.
    fn neighbours_by(&self, code: &str, k: usize, keep: impl Fn(&str) -> bool) -> Vec<(String, f32)> {
        let Some(&i) = self.ix.get(code) else { return Vec::new() };
        let q = self.row(i);
        let mut scored: Vec<(String, f32)> = (0..self.size())
            .filter(|&j| j != i)
            .filter(|&j| keep(&self.keys[j]))
            .map(|j| (self.keys[j].clone(), dot(q, self.row(j))))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored
    }

    /// The `k` nearest concepts by cosine, restricted to the same language
    /// (testament), strongest first.
    pub fn nearest_concepts(&self, code: &str, k: usize) -> Vec<(String, f32)> {
        let lang = lang_of(code);
        self.neighbours_by(code, k, |c| lang_of(c) == lang)
    }

    /// The `k` nearest concepts in the *other* language — the cross-testament
    /// bridge. Empty unless the artifact is aligned, so callers show the
    /// section exactly when it means something.
    pub fn cross_concepts(&self, code: &str, k: usize) -> Vec<(String, f32)> {
        if !self.aligned {
            return Vec::new();
        }
        let lang = lang_of(code);
        self.neighbours_by(code, k, |c| lang_of(c) != lang)
    }
}

/// First byte (`H`/`G`) marks the testament/language of a Strong's number.
fn lang_of(code: &str) -> u8 {
    code.bytes().next().unwrap_or(b'?')
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

/// Parse an embedding from its text parts (the `.meta` JSON if present, the
/// `.vec` body, and the `.freq` body if present). Returns `None` when the meta
/// names a different tokenization (the vectors address another text) or the
/// body is empty/malformed. Split out from [`load_embedding`] so it is unit
/// testable without touching the filesystem.
pub fn parse_embedding(
    tok_version: &str,
    meta_json: Option<&str>,
    vec_text: &str,
    freq_text: Option<&str>,
) -> Option<Embedding> {
    // Meta: gate on tokenization; pick up `aligned` + the alias map. No meta at
    // all is accepted (artifacts predate the stamp; the tokenization is frozen).
    let (aligned, aliases) = match meta_json {
        None => (false, HashMap::new()),
        Some(raw) => {
            let m: EmbedMeta = serde_json::from_str(raw).ok()?;
            if m.tokenization != tok_version {
                return None; // stale: vectors address a different text
            }
            (m.aligned.is_some(), m.aliases)
        }
    };

    let mut lines = vec_text.lines();
    let dim: usize = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    if dim == 0 {
        return None;
    }

    let mut keys: Vec<String> = Vec::new();
    let mut vecs: Vec<f32> = Vec::new();
    for line in lines {
        let mut it = line.split_whitespace();
        let Some(key) = it.next() else { continue };
        let row: Vec<f32> = it.filter_map(|t| t.parse::<f32>().ok()).collect();
        if row.len() != dim {
            continue; // skip a malformed row rather than mis-slice the array
        }
        let mut row = row;
        normalize(&mut row);
        keys.push(key.to_string());
        vecs.extend_from_slice(&row);
    }
    if keys.is_empty() {
        return None;
    }

    let mut ix: HashMap<String, usize> = keys.iter().cloned().zip(0..).collect();
    // Alias keys resolve to their root's ROW, so a split Strong's number the
    // KJV tags is found; the row set — and every neighbour list — stays
    // duplicate-free (aliases never add rows, only extra index entries).
    for (alias, root) in &aliases {
        if !ix.contains_key(alias) {
            if let Some(&r) = ix.get(root) {
                ix.insert(alias.clone(), r);
            }
        }
    }

    let freq = freq_text.and_then(|t| {
        let m: HashMap<String, u64> = t
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let w = it.next()?;
                let c = it.next()?.parse().ok()?;
                Some((w.to_string(), c))
            })
            .collect();
        if m.is_empty() {
            None
        } else {
            Some(m)
        }
    });

    Some(Embedding { dim, keys, ix, vecs, aligned, freq })
}

/// Load `concept-vectors.vec` (with its `.meta` and `.freq` sidecars) from
/// `path`. Returns `None` if the file is missing, stale (tokenization
/// mismatch), or unparseable — so the app runs fine without it.
pub fn load_embedding(tok_version: &str, path: impl AsRef<Path>) -> Option<Embedding> {
    let path = path.as_ref();
    let vec_text = std::fs::read_to_string(path).ok()?;
    let meta = std::fs::read_to_string(path.with_extension("vec.meta")).ok();
    // `with_extension` replaces after the last dot; build the sidecar paths by
    // appending instead so "concept-vectors.vec" → ".vec.meta"/".vec.freq".
    let meta = meta.or_else(|| std::fs::read_to_string(sidecar(path, "meta")).ok());
    let freq = std::fs::read_to_string(sidecar(path, "freq")).ok();
    parse_embedding(tok_version, meta.as_deref(), &vec_text, freq.as_deref())
}

/// `<path>.<ext>` (append, not replace-extension).
fn sidecar(path: &Path, ext: &str) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".");
    s.push(ext);
    std::path::PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A tiny aligned space: two Greek, two Hebrew. G1 ‖ G2 point nearly the same
    // way; H1 is aligned near G1 (cross-testament partner).
    const VEC: &str = "4 2\n\
        G1 1.0 0.0\n\
        G2 0.9 0.1\n\
        H1 0.95 0.05\n\
        H2 -1.0 0.0\n";
    const META: &str = r#"{"format":"overlay-embedding-meta-v1","tokenization":"kjv1769-tok2","aligned":"procrustes","aliases":{"G9":"G1"}}"#;
    const FREQ: &str = "G1 100\nG2 50\nH1 40\nH2 5\n";

    fn emb() -> Embedding {
        parse_embedding("kjv1769-tok2", Some(META), VEC, Some(FREQ)).unwrap()
    }

    #[test]
    fn loads_normalizes_and_indexes() {
        let e = emb();
        assert_eq!(e.dim(), 2);
        assert_eq!(e.size(), 4);
        assert!(e.aligned());
        assert!(e.has_trained_freq());
        assert_eq!(e.freq_of("G1"), Some(100));
        // Rows are unit length.
        let v = e.concept_vector("G2").unwrap();
        assert!((v.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-5);
        // Alias resolves to its root's row (same vector as G1), no extra row.
        assert_eq!(e.size(), 4);
        assert_eq!(e.concept_vector("G9"), e.concept_vector("G1"));
    }

    #[test]
    fn nearest_is_same_language_cross_is_other() {
        let e = emb();
        let near = e.nearest_concepts("G1", 5);
        // Same-language only: G2, never H*.
        assert!(near.iter().all(|(k, _)| k.starts_with('G')));
        assert_eq!(near[0].0, "G2");

        let cross = e.cross_concepts("G1", 5);
        assert!(cross.iter().all(|(k, _)| k.starts_with('H')));
        assert_eq!(cross[0].0, "H1"); // the aligned partner, not the opposite H2
    }

    #[test]
    fn cross_is_empty_when_unaligned() {
        let e = parse_embedding("kjv1769-tok2", None, VEC, None).unwrap();
        assert!(!e.aligned());
        assert!(e.cross_concepts("G1", 5).is_empty());
        assert!(!e.has_trained_freq());
    }

    #[test]
    fn stale_tokenization_is_refused() {
        let stale = r#"{"tokenization":"other-tok","aligned":"x"}"#;
        assert!(parse_embedding("kjv1769-tok2", Some(stale), VEC, None).is_none());
    }
}
