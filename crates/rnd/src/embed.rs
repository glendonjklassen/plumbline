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

use plumbline_core::corpus::Corpus;
use plumbline_core::reference::VRef;

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
    /// first, excluding the query row itself. Scores by row index and clones
    /// only the `k` winners' keys — this runs per query on the FFI hot path,
    /// so a String per vocab row would be an allocation storm. Ties keep
    /// ascending row order (stable sort over an index-ordered scan), exactly
    /// as the old clone-everything version did.
    fn neighbours_by(&self, code: &str, k: usize, keep: impl Fn(&str) -> bool) -> Vec<(String, f32)> {
        let Some(&i) = self.ix.get(code) else { return Vec::new() };
        let q = self.row(i);
        let mut scored: Vec<(usize, f32)> = (0..self.size())
            .filter(|&j| j != i)
            .filter(|&j| keep(&self.keys[j]))
            .map(|j| (j, dot(q, self.row(j))))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored.into_iter().map(|(j, s)| (self.keys[j].clone(), s)).collect()
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

// ── verse similarity (SIF) ─────────────────────────────────────────────────────

/// The SIF smoothing constant `a` (Arora, Liang & Ma 2017).
const SIF_A: f32 = 1.0e-3;
/// Below this many verses in a testament, a fitted principal component is noise,
/// not signal, so the PC-removal step is skipped (mean subtraction still runs).
const SIF_PC_MIN_VERSES: usize = 50;

/// Precomputed verse vectors for "verses like this one". A naive average of a
/// verse's concept vectors is dominated by ubiquitous words, so every verse
/// looks alike; SIF instead weights each concept by `a / (a + p(concept))` to
/// damp the frequent ones, then removes each verse vector's per-testament mean
/// and its projection onto that testament's dominant direction. What survives is
/// the verse's distinctive theme. Ported from overlay `VerseSim`.
#[derive(Debug, Clone)]
pub struct VerseSim {
    dim: usize,
    refs: Vec<VRef>,
    nt: Vec<bool>,
    vecs: Vec<f32>,
    ix: HashMap<VRef, usize>,
    aligned: bool,
}

fn add_into(acc: &mut [f32], x: &[f32]) {
    for (a, b) in acc.iter_mut().zip(x) {
        *a += *b;
    }
}

/// The top principal component of `xs` (assumed mean-centred) by power
/// iteration — a genuine PCA direction, no linear-algebra dependency.
fn top_principal_component(dim: usize, xs: &[Vec<f32>]) -> Vec<f32> {
    let is_zero = |v: &[f32]| v.iter().all(|&x| x == 0.0);
    let mut v = match xs.iter().find(|x| !is_zero(x)) {
        Some(seed) => {
            let mut s = seed.clone();
            normalize(&mut s);
            s
        }
        None => return vec![0.0; dim],
    };
    for _ in 0..100 {
        let mut next = vec![0.0f32; dim];
        for x in xs {
            let c = dot(x, &v);
            for (n, xi) in next.iter_mut().zip(x) {
                *n += c * xi;
            }
        }
        normalize(&mut next);
        if is_zero(&next) || 1.0 - dot(&v, &next).abs() < 1.0e-10 {
            return next;
        }
        v = next;
    }
    v
}

impl VerseSim {
    /// Number of verses with a vector.
    pub fn count(&self) -> usize {
        self.refs.len()
    }
    /// Built from an aligned embedding (so cross-testament similarity is live).
    pub fn aligned(&self) -> bool {
        self.aligned
    }

    fn row(&self, i: usize) -> &[f32] {
        &self.vecs[i * self.dim..(i + 1) * self.dim]
    }

    /// Build the SIF model from an embedding and the corpus. Pure but heavy (one
    /// vector per verse); build it once at startup. The `a/(a+p)` weights use
    /// the trainer's own frequency table when the artifact shipped one, else a
    /// fresh count over the corpus.
    pub fn build(emb: &Embedding, corpus: &Corpus) -> VerseSim {
        let d = emb.dim;
        let verses = corpus.verses();

        // Frequency table + total.
        let (counts, total): (HashMap<&str, u64>, f64) = match &emb.freq {
            Some(fm) if !fm.is_empty() => {
                let t = fm.values().sum::<u64>().max(1) as f64;
                (fm.iter().map(|(k, v)| (k.as_str(), *v)).collect(), t)
            }
            _ => {
                let mut c: HashMap<&str, u64> = HashMap::new();
                for v in verses {
                    for t in &v.tokens {
                        for s in &t.strongs {
                            *c.entry(s.as_str()).or_insert(0) += 1;
                        }
                    }
                }
                let t = c.values().sum::<u64>().max(1) as f64;
                (c, t)
            }
        };
        let w_of = |s: &str| SIF_A / (SIF_A + counts.get(s).copied().unwrap_or(0) as f64 as f32 / total as f32);

        // SIF-weighted average of a verse's in-vocabulary concept vectors.
        let raw_of = |strongs: &[String]| -> Option<Vec<f32>> {
            let mut acc = vec![0.0f32; d];
            let mut n = 0usize;
            for s in strongs {
                if let Some(cv) = emb.concept_vector(s) {
                    let w = w_of(s);
                    for (a, x) in acc.iter_mut().zip(cv) {
                        *a += w * x;
                    }
                    n += 1;
                }
            }
            if n == 0 {
                return None;
            }
            let inv = 1.0 / n as f32;
            for a in acc.iter_mut() {
                *a *= inv;
            }
            Some(acc)
        };

        // (ref, is_greek, raw-vector) per verse that had any in-vocab concept.
        struct Entry {
            reference: VRef,
            greek: bool,
            raw: Vec<f32>,
        }
        let mut entries: Vec<Entry> = Vec::new();
        for v in verses {
            let strongs: Vec<String> =
                v.tokens.iter().flat_map(|t| t.strongs.iter().cloned()).collect();
            let greek = strongs.first().is_some_and(|s| s.starts_with('G'));
            if let Some(raw) = raw_of(&strongs) {
                entries.push(Entry { reference: v.vref(), greek, raw });
            }
        }

        // Per-testament mean + top principal component.
        let mean_of = |greek: bool| -> Vec<f32> {
            let rows: Vec<&Vec<f32>> = entries.iter().filter(|e| e.greek == greek).map(|e| &e.raw).collect();
            if rows.is_empty() {
                return vec![0.0; d];
            }
            let mut acc = vec![0.0f32; d];
            for r in &rows {
                add_into(&mut acc, r);
            }
            let inv = 1.0 / rows.len() as f32;
            for a in acc.iter_mut() {
                *a *= inv;
            }
            acc
        };
        let (mu_h, mu_g) = (mean_of(false), mean_of(true));
        let centered = |greek: bool, mu: &[f32]| -> Vec<Vec<f32>> {
            entries
                .iter()
                .filter(|e| e.greek == greek)
                .map(|e| e.raw.iter().zip(mu).map(|(x, m)| x - m).collect())
                .collect()
        };
        let pc_of = |rows: &[Vec<f32>]| -> Vec<f32> {
            if rows.len() >= SIF_PC_MIN_VERSES {
                top_principal_component(d, rows)
            } else {
                vec![0.0; d]
            }
        };
        let pc_h = pc_of(&centered(false, &mu_h));
        let pc_g = pc_of(&centered(true, &mu_g));

        // Adjust each raw vector: subtract the mean, remove the PC projection,
        // normalise.
        let mut refs = Vec::with_capacity(entries.len());
        let mut nt = Vec::with_capacity(entries.len());
        let mut vecs = Vec::with_capacity(entries.len() * d);
        for e in &entries {
            let (mu, pc) = if e.greek { (&mu_g, &pc_g) } else { (&mu_h, &pc_h) };
            let mut c: Vec<f32> = e.raw.iter().zip(mu).map(|(x, m)| x - m).collect();
            let proj = dot(&c, pc);
            for (ci, p) in c.iter_mut().zip(pc) {
                *ci -= proj * p;
            }
            normalize(&mut c);
            refs.push(e.reference.clone());
            nt.push(e.greek);
            vecs.extend_from_slice(&c);
        }
        let ix = refs.iter().cloned().zip(0..).collect();
        VerseSim { dim: d, refs, nt, vecs, ix, aligned: emb.aligned }
    }

    /// Scores by verse index and clones only the `k` winners' refs — one VRef
    /// per verse (~31k) per call would be an allocation storm on the FFI hot
    /// path. Ties keep ascending verse order (stable sort over an
    /// index-ordered scan), exactly as the old clone-everything version did.
    fn similar_by(&self, reference: &VRef, k: usize, keep: impl Fn(bool, bool) -> bool) -> Vec<(VRef, f32)> {
        let Some(&i) = self.ix.get(reference) else { return Vec::new() };
        let q = self.row(i);
        let g = self.nt[i];
        let mut scored: Vec<(usize, f32)> = (0..self.count())
            .filter(|&j| j != i && keep(g, self.nt[j]))
            .map(|j| (j, dot(q, self.row(j))))
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(k);
        scored.into_iter().map(|(j, s)| (self.refs[j].clone(), s)).collect()
    }

    /// The `k` most similar verses in the same testament, strongest first.
    pub fn similar_verses_in(&self, reference: &VRef, k: usize) -> Vec<(VRef, f32)> {
        self.similar_by(reference, k, |g, other| g == other)
    }

    /// The `k` most similar verses in the *other* testament — meaningful only
    /// when the embedding was aligned; empty otherwise.
    pub fn similar_verses_cross(&self, reference: &VRef, k: usize) -> Vec<(VRef, f32)> {
        if !self.aligned {
            return Vec::new();
        }
        self.similar_by(reference, k, |g, other| g != other)
    }
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

    #[test]
    fn neighbour_ties_keep_row_order() {
        // G2 and G3 are exact mirror images across the query's axis, so their
        // cosines against G1 are bit-for-bit equal (only the sign of a term
        // multiplied by the query's 0.0 differs). The stable sort must keep
        // exact ties in row order — G2 (row 1) before G3 (row 2) — as the
        // pre-index-scoring implementation did.
        const TIE: &str = "3 2\nG1 1.0 0.0\nG2 0.6 0.8\nG3 0.6 -0.8\n";
        let e = parse_embedding("kjv1769-tok2", None, TIE, None).unwrap();
        let near = e.nearest_concepts("G1", 5);
        assert_eq!(near.len(), 2);
        assert_eq!(near[0].1, near[1].1, "fixture must produce an exact tie");
        assert_eq!(near[0].0, "G2");
        assert_eq!(near[1].0, "G3");
    }

    #[test]
    fn similar_verse_ties_keep_verse_order() {
        // Three one-concept verses over the mirror-image space above: the
        // verses tagged G2 and G3 land on exact mirror vectors (the shared
        // testament mean's y-component cancels to 0.0 exactly), so their
        // similarity to the G1 verse ties bit-for-bit. The stable sort must
        // keep corpus order — John 1:2 before John 1:3.
        const TIE: &str = "3 2\nG1 1.0 0.0\nG2 0.6 0.8\nG3 0.6 -0.8\n";
        let jsonl = concat!(
            "{\"tokenization\":\"kjv1769-tok2\",\"verses\":3}\n",
            "{\"b\":\"John\",\"c\":1,\"v\":1,\"t\":[[\"\",\"a\",\"\",[\"G1\"],0]]}\n",
            "{\"b\":\"John\",\"c\":1,\"v\":2,\"t\":[[\"\",\"b\",\"\",[\"G2\"],0]]}\n",
            "{\"b\":\"John\",\"c\":1,\"v\":3,\"t\":[[\"\",\"c\",\"\",[\"G3\"],0]]}\n",
        );
        let corpus = plumbline_core::corpus::from_str(jsonl).unwrap();
        let e = parse_embedding("kjv1769-tok2", None, TIE, None).unwrap();
        let vs = VerseSim::build(&e, &corpus);
        assert_eq!(vs.count(), 3);
        let q = corpus.verses()[0].vref();
        let sim = vs.similar_verses_in(&q, 5);
        assert_eq!(sim.len(), 2);
        assert_eq!(sim[0].1, sim[1].1, "fixture must produce an exact tie");
        assert_eq!(sim[0].0, corpus.verses()[1].vref());
        assert_eq!(sim[1].0, corpus.verses()[2].vref());
    }

    #[test]
    fn power_iteration_finds_the_dominant_axis() {
        // Points strung along the x-axis (with y jitter) → top PC ≈ ±x.
        let xs: Vec<Vec<f32>> = vec![
            vec![3.0, 0.1],
            vec![-2.0, -0.1],
            vec![5.0, 0.05],
            vec![-4.0, 0.0],
            vec![1.0, -0.05],
        ];
        let pc = top_principal_component(2, &xs);
        assert!(pc[0].abs() > 0.98, "dominant axis should be x, got {pc:?}");
        assert!(pc[1].abs() < 0.2);
        // Unit length.
        assert!((pc.iter().map(|v| v * v).sum::<f32>() - 1.0).abs() < 1e-4);
    }
}
