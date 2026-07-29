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
    /// (testament), strongest first. Function words (articles, conjunctions,
    /// prepositions…) are skipped — they co-occur with everything, so they
    /// cosine-near every content word without meaning anything by it.
    pub fn nearest_concepts(&self, code: &str, k: usize) -> Vec<(String, f32)> {
        let lang = lang_of(code);
        self.neighbours_by(code, k, |c| lang_of(c) == lang && !crate::stopwords::is_function_word(c))
    }

    /// The `k` nearest concepts in the *other* language — the cross-testament
    /// bridge. Empty unless the artifact is aligned, so callers show the
    /// section exactly when it means something.
    pub fn cross_concepts(&self, code: &str, k: usize) -> Vec<(String, f32)> {
        if !self.aligned {
            return Vec::new();
        }
        let lang = lang_of(code);
        self.neighbours_by(code, k, |c| lang_of(c) != lang && !crate::stopwords::is_function_word(c))
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
    let (aligned, aliases) = parse_meta(tok_version, meta_json)?;
    let (dim, keys, vecs) = parse_vec_text(vec_text)?;
    finish(dim, keys, vecs, aligned, &aliases, freq_text)
}

/// Meta: gate on tokenization; pick up `aligned` + the alias map. No meta at all
/// is accepted (artifacts predate the stamp; the tokenization is frozen). `None`
/// means STALE — the vectors address a different text.
fn parse_meta(
    tok_version: &str,
    meta_json: Option<&str>,
) -> Option<(bool, HashMap<String, String>)> {
    match meta_json {
        None => Some((false, HashMap::new())),
        Some(raw) => {
            let m: EmbedMeta = serde_json::from_str(raw).ok()?;
            if m.tokenization != tok_version {
                return None;
            }
            Some((m.aligned.is_some(), m.aliases))
        }
    }
}

/// The word2vec text body → `(dim, keys, RAW row-major floats)`. Rows are left
/// un-normalised; [`finish`] does that for every source alike.
fn parse_vec_text(vec_text: &str) -> Option<(usize, Vec<String>, Vec<f32>)> {
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
        keys.push(key.to_string());
        vecs.extend_from_slice(&row);
    }
    Some((dim, keys, vecs))
}

/// Normalise, index, alias and attach `.freq` — everything after the bytes have
/// been read, shared by the text and packed loaders so the two cannot drift.
fn finish(
    dim: usize,
    keys: Vec<String>,
    mut vecs: Vec<f32>,
    aligned: bool,
    aliases: &HashMap<String, String>,
    freq_text: Option<&str>,
) -> Option<Embedding> {
    if dim == 0 || keys.is_empty() || vecs.len() != keys.len() * dim {
        return None;
    }
    for row in vecs.chunks_mut(dim) {
        normalize(row);
    }

    let mut ix: HashMap<String, usize> = keys.iter().cloned().zip(0..).collect();
    // Alias keys resolve to their root's ROW, so a split Strong's number the
    // KJV tags is found; the row set — and every neighbour list — stays
    // duplicate-free (aliases never add rows, only extra index entries).
    for (alias, root) in aliases {
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

// ── the packed form (`.vecb`) ──────────────────────────────────────────────────
//
// The text `.vec` is 6.4 MB of decimal ASCII — 742,600 floats that cost an atof
// each, every launch. The browser can't keep the PARSED embedding between
// launches (it lives in wasm memory), so a phone paid seconds of `atof` on every
// single start before a concept answer appeared (feedback 2026-07-27). Packed
// f32 turns that parse into a bounded copy.
//
// Deliberately stores RAW, un-normalised rows, exactly as the text does: the
// reader still runs [`finish`], so a packed load and a text load produce the
// same `Embedding` by construction rather than by a writer that remembers to
// normalise the same way. `plumbline-hydrate vecb` writes it.
//
//   0..8    magic "PLVECB01"
//   8..12   dim      u32 LE
//   12..16  count    u32 LE
//   16..20  keys_len u32 LE   (key blob length, padded to a multiple of 4)
//   20..24  reserved u32 LE   (0)
//   24..    key blob: per row a u8 length then its ASCII bytes, zero-padded
//   then    count*dim f32 LE, row-major, RAW

const VECB_MAGIC: &[u8; 8] = b"PLVECB01";
const VECB_HEADER: usize = 24;

/// Pack a word2vec text body into [`parse_embedding_bin`]'s form. `None` if the
/// text isn't a readable `.vec`.
pub fn encode_embedding_bin(vec_text: &str) -> Option<Vec<u8>> {
    let (dim, keys, vecs) = parse_vec_text(vec_text)?;
    if keys.is_empty() {
        return None;
    }
    let mut blob: Vec<u8> = Vec::new();
    for k in &keys {
        let b = k.as_bytes();
        // A key longer than a byte can count cannot round-trip; refuse rather
        // than silently truncate one row's identity.
        if b.len() > u8::MAX as usize {
            return None;
        }
        blob.push(b.len() as u8);
        blob.extend_from_slice(b);
    }
    while blob.len() % 4 != 0 {
        blob.push(0);
    }

    let mut out = Vec::with_capacity(VECB_HEADER + blob.len() + vecs.len() * 4);
    out.extend_from_slice(VECB_MAGIC);
    out.extend_from_slice(&(dim as u32).to_le_bytes());
    out.extend_from_slice(&(keys.len() as u32).to_le_bytes());
    out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&blob);
    for v in &vecs {
        out.extend_from_slice(&v.to_le_bytes());
    }
    Some(out)
}

/// Read the packed form. `None` on a foreign/short/stale file, so a caller can
/// fall back to the text `.vec` exactly as if the packed one weren't there.
pub fn parse_embedding_bin(
    tok_version: &str,
    meta_json: Option<&str>,
    bytes: &[u8],
    freq_text: Option<&str>,
) -> Option<Embedding> {
    let (aligned, aliases) = parse_meta(tok_version, meta_json)?;
    if bytes.len() < VECB_HEADER || &bytes[..8] != VECB_MAGIC {
        return None;
    }
    let u32_at = |o: usize| -> Option<usize> {
        Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?) as usize)
    };
    let dim = u32_at(8)?;
    let count = u32_at(12)?;
    let keys_len = u32_at(16)?;
    if dim == 0 || count == 0 {
        return None;
    }

    let keys_at = VECB_HEADER;
    let floats_at = keys_at.checked_add(keys_len)?;
    let floats_len = count.checked_mul(dim)?.checked_mul(4)?;
    if bytes.len() < floats_at.checked_add(floats_len)? {
        return None;
    }

    let blob = &bytes[keys_at..floats_at];
    let mut keys: Vec<String> = Vec::with_capacity(count);
    let mut at = 0usize;
    for _ in 0..count {
        let len = *blob.get(at)? as usize;
        at += 1;
        let raw = blob.get(at..at + len)?;
        keys.push(std::str::from_utf8(raw).ok()?.to_string());
        at += len;
    }

    let vecs: Vec<f32> = bytes[floats_at..floats_at + floats_len]
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    finish(dim, keys, vecs, aligned, &aliases, freq_text)
}

/// Load `concept-vectors.vec` (with its `.meta` and `.freq` sidecars) from
/// `path`. Returns `None` if the file is missing, stale (tokenization
/// mismatch), or unparseable — so the app runs fine without it.
pub fn load_embedding(tok_version: &str, path: impl AsRef<Path>) -> Option<Embedding> {
    let path = path.as_ref();
    let meta = std::fs::read_to_string(path.with_extension("vec.meta")).ok();
    // `with_extension` replaces after the last dot; build the sidecar paths by
    // appending instead so "concept-vectors.vec" → ".vec.meta"/".vec.freq".
    let meta = meta.or_else(|| std::fs::read_to_string(sidecar(path, "meta")).ok());
    let freq = std::fs::read_to_string(sidecar(path, "freq")).ok();

    // The packed sibling first — same vectors, no 742k-atof parse. A home that
    // only has the text `.vec` (an older pack, a hand-assembled home) still
    // works, and a packed file we can't read falls through to the text as well.
    let packed = vecb_path(path);
    if let Some(bytes) = std::fs::read(&packed).ok() {
        if let Some(e) = parse_embedding_bin(tok_version, meta.as_deref(), &bytes, freq.as_deref()) {
            return Some(e);
        }
    }
    let vec_text = std::fs::read_to_string(path).ok()?;
    parse_embedding(tok_version, meta.as_deref(), &vec_text, freq.as_deref())
}

/// `data/concept-vectors.vec` → `data/concept-vectors.vecb`.
pub fn vecb_path(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push("b");
    std::path::PathBuf::from(s)
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

fn is_zero(v: &[f32]) -> bool {
    v.iter().all(|&x| x == 0.0)
}

/// Row `i` of a row-major `dim`-wide matrix.
fn row_of(rows: &[f32], dim: usize, i: usize) -> &[f32] {
    &rows[i * dim..(i + 1) * dim]
}

/// How many power iterations before the current direction is accepted.
const SIF_PC_ITERS: usize = 100;

/// PCA over a row-major matrix, mean-centred ON READ.
///
/// This used to be `top_principal_component(dim, xs: &[Vec<f32>])`, handed a
/// freshly materialised copy of the centred matrix — a second ~12 MB as ~31k
/// separate 400-byte allocations, built and dropped once per testament. Up to
/// [`SIF_PC_ITERS`] power iterations then walked it, so every iteration
/// pointer-chased scattered heap instead of streaming one contiguous buffer.
///
/// That is the whole explanation for a SIF build costing 226 ms on a desktop and
/// **54,859 ms on a phone** (maintainer's boot trace, 2026-07-28) — a ~240x gap
/// where the CPUs differ by ~6-10x. The arithmetic was never the cost; the layout
/// was, and a desktop's caches hid it.
///
/// The subtraction moved from build time to read time in the SAME order, so every
/// float result is unchanged. `verse_sim_ties_break_in_corpus_order` pins an exact
/// bit-for-bit tie and would catch any reassociation here.
struct Pca<'a> {
    dim: usize,
    rows: &'a [f32],
    /// The rows this component is fitted over — one testament's verses.
    idx: &'a [u32],
    mu: &'a [f32],
}

impl Pca<'_> {
    /// Centred row `idx[k]`, into `out`.
    fn centred(&self, k: usize, out: &mut [f32]) {
        let x = row_of(self.rows, self.dim, self.idx[k] as usize);
        for (o, (xi, m)) in out.iter_mut().zip(x.iter().zip(self.mu)) {
            *o = xi - m;
        }
    }

    /// The starting direction: the first non-zero centred row, normalised.
    /// `None` when every row is zero — the caller reports no component at all.
    fn seed(&self) -> Option<Vec<f32>> {
        let mut buf = vec![0.0f32; self.dim];
        for k in 0..self.idx.len() {
            self.centred(k, &mut buf);
            if !is_zero(&buf) {
                normalize(&mut buf);
                return Some(buf);
            }
        }
        None
    }

    /// ONE power iteration. Split out from the loop so the web can spend a
    /// macrotask per iteration rather than holding the worker for all hundred.
    fn iterate(&self, v: &[f32]) -> Vec<f32> {
        let mut next = vec![0.0f32; self.dim];
        let mut x = vec![0.0f32; self.dim];
        for k in 0..self.idx.len() {
            self.centred(k, &mut x);
            let c = dot(&x, v);
            for (n, xi) in next.iter_mut().zip(&x) {
                *n += c * xi;
            }
        }
        normalize(&mut next);
        next
    }
}

/// Whether power iteration has settled, or collapsed to nothing.
fn pca_settled(v: &[f32], next: &[f32]) -> bool {
    is_zero(next) || 1.0 - dot(v, next).abs() < 1.0e-10
}

// ── the SIF model, saved ──────────────────────────────────────────────────────
//
// Building this model is the single most expensive thing a launch does: 11.2 s of
// phone CPU, 41 sweeps of the whole corpus, repeated on EVERY launch because
// nothing an engine builds survives the tab (2026-07-28). It is a pure function
// of the embedding and the corpus, so it never needed to be computed twice.
//
// Hand-rolled rather than bincode, matching `.vecb` right above: `VRef` carries
// no serde derives and this crate has no bincode, and adding both to persist one
// struct would be a bigger change than the format itself.
//
//   0..8     magic "PLSIF001"
//   8..12    dim       u32 LE
//   12..16   count     u32 LE
//   16..17   aligned   u8
//   17..21   stamp_len u32 LE
//   21..     stamp bytes  — what this model was built FROM
//   then     per verse: nt u8, book_len u8, book bytes, chapter u16 LE, verse u16 LE
//   then     count*dim f32 LE, row-major
//
// THE STAMP IS THE WHOLE SAFETY STORY. A cached model is verse vectors keyed to a
// particular corpus and a particular embedding; serve it against different ones
// and every "verses like this" answer is quietly wrong — wrong in a way no
// exception surfaces and no reader can detect. The caller passes a stamp
// combining the tokenization version and the data pack version, and a mismatch is
// treated as no cache at all rather than as something to repair.
const SIF_MAGIC: &[u8; 8] = b"PLSIF001";

impl VerseSim {
    /// Serialise for storage, stamped with what it was built from.
    pub fn encode(&self, stamp: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.refs.len() * 16 + self.vecs.len() * 4);
        out.extend_from_slice(SIF_MAGIC);
        out.extend_from_slice(&(self.dim as u32).to_le_bytes());
        out.extend_from_slice(&(self.refs.len() as u32).to_le_bytes());
        out.push(self.aligned as u8);
        out.extend_from_slice(&(stamp.len() as u32).to_le_bytes());
        out.extend_from_slice(stamp.as_bytes());
        for (r, &greek) in self.refs.iter().zip(&self.nt) {
            out.push(greek as u8);
            let b = r.book.as_bytes();
            // Book ids are short canon abbreviations; a byte of length is plenty
            // and anything longer is not a book id we wrote.
            out.push(b.len().min(255) as u8);
            out.extend_from_slice(&b[..b.len().min(255)]);
            out.extend_from_slice(&r.chapter.to_le_bytes());
            out.extend_from_slice(&r.verse.to_le_bytes());
        }
        for v in &self.vecs {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// Restore a saved model, or `None`.
    ///
    /// `None` for a different stamp, a different format, or bytes that are
    /// truncated or otherwise not what they claim — every one of which is
    /// "rebuild it" rather than an error, because a wrong model here is
    /// undetectable downstream. Never panics on hostile input: storage can hand
    /// back anything.
    pub fn decode(bytes: &[u8], stamp: &str) -> Option<VerseSim> {
        let u32_at = |o: usize| -> Option<u32> {
            Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
        };
        if !bytes.starts_with(SIF_MAGIC) {
            return None;
        }
        let dim = u32_at(8)? as usize;
        let count = u32_at(12)? as usize;
        let aligned = *bytes.get(16)? != 0;
        let stamp_len = u32_at(17)? as usize;
        let mut at = 21;
        if bytes.get(at..at + stamp_len)? != stamp.as_bytes() {
            return None; // built from something else
        }
        at += stamp_len;
        if dim == 0 {
            return None;
        }
        // A COUNT IS A CLAIM FROM STORAGE, NOT A FACT, and it is about to size an
        // allocation. Every row costs at least 6 bytes of reference (nt, a length
        // byte, chapter, verse) plus `dim * 4` of vector, so a count the file
        // cannot possibly back is refused BEFORE it is believed. Without this,
        // `Vec::with_capacity` on a header claiming `u32::MAX` rows asked the
        // allocator for 137 GB and aborted the process — found by the
        // damaged-input test, which is the entire reason it exists.
        let per_row = 6usize.checked_add(dim.checked_mul(4)?)?;
        if count.checked_mul(per_row)? > bytes.len().saturating_sub(at) {
            return None;
        }

        let mut refs = Vec::with_capacity(count);
        let mut nt = Vec::with_capacity(count);
        for _ in 0..count {
            let greek = *bytes.get(at)? != 0;
            let blen = *bytes.get(at + 1)? as usize;
            let book = std::str::from_utf8(bytes.get(at + 2..at + 2 + blen)?).ok()?;
            let ch = u16::from_le_bytes(bytes.get(at + 2 + blen..at + 4 + blen)?.try_into().ok()?);
            let vs = u16::from_le_bytes(bytes.get(at + 4 + blen..at + 6 + blen)?.try_into().ok()?);
            at += 6 + blen;
            refs.push(VRef::new(book, ch, vs));
            nt.push(greek);
        }

        let need = count.checked_mul(dim)?.checked_mul(4)?;
        let raw = bytes.get(at..at + need)?;
        let mut vecs = Vec::with_capacity(count * dim);
        for c in raw.chunks_exact(4) {
            vecs.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        }
        // `ix` is derived rather than stored — it is exactly this, and storing it
        // would be a second copy that could disagree with the refs it indexes.
        let ix = refs.iter().cloned().zip(0..).collect();
        Some(VerseSim { dim, refs, nt, vecs, ix, aligned })
    }
}

/// Sliced construction of the SIF model — one budgeted slice per call.
///
/// Phase 7 of the web's chunked warm was the one heavy phase that never got
/// sliced (all the others were, 2026-07-27), so it ran as a single synchronous
/// block: **54,859 ms on a real phone**, during which the engine worker answers
/// no layout, no tap and no word study, because it is the only thread that can.
/// That is the "it says loading and the first one takes longer, every time I
/// reopen it" report of 2026-07-28.
///
/// Stages, each resumable, in the order the maths requires:
///
/// | stage | unit of work | why it must be its own stage |
/// |-------|--------------|------------------------------|
/// | `Rows`   | verses | needs the whole corpus before any mean exists |
/// | `Means`  | rows   | a mean is only a mean once every row is in |
/// | `Pc*`    | one power iteration | each iteration reads every row of a testament |
/// | `Adjust` | rows   | needs both means and both components |
///
/// SLICING MUST NOT CHANGE THE ANSWER, and that is not obvious for floating
/// point: a different accumulation order gives different last bits.
/// `sliced_build_matches_one_shot` steps this one verse at a time and compares
/// bit-for-bit against a whole-corpus budget.
pub struct VerseSimBuilder {
    stage: SifStage,
    /// Position within the current stage's unit of work.
    cursor: usize,
    dim: usize,
    aligned: bool,
    /// `a / (a + p(concept))` per code, precomputed once: the frequency table is
    /// walked here rather than on every step.
    weights: HashMap<String, f32>,
    refs: Vec<VRef>,
    nt: Vec<bool>,
    /// Row-major, `refs.len() * dim`. ONE contiguous buffer — see [`Pca`] for why
    /// that is the whole fix and not a tidiness preference.
    rows: Vec<f32>,
    /// Row indices per testament, so a component is fitted over one of them.
    hebrew: Vec<u32>,
    greek: Vec<u32>,
    mu_h: Vec<f32>,
    mu_g: Vec<f32>,
    pc_h: Vec<f32>,
    pc_g: Vec<f32>,
    /// Power-iteration state for whichever testament is being fitted now.
    pc_v: Option<Vec<f32>>,
    pc_iter: usize,
    out: Option<VerseSim>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum SifStage {
    Rows,
    Means,
    PcHebrew,
    PcGreek,
    Adjust,
    Done,
}

impl VerseSimBuilder {
    /// The weights are the only thing that needs the corpus up front, and only
    /// when the embedding shipped no trained frequency table (the packs do ship
    /// one, so this normally reads `emb.freq` and never walks the corpus).
    pub fn new(emb: &Embedding, corpus: &Corpus) -> VerseSimBuilder {
        let (counts, total): (HashMap<&str, u64>, f64) = match &emb.freq {
            Some(fm) if !fm.is_empty() => {
                let t = fm.values().sum::<u64>().max(1) as f64;
                (fm.iter().map(|(k, v)| (k.as_str(), *v)).collect(), t)
            }
            _ => {
                let mut c: HashMap<&str, u64> = HashMap::new();
                for v in corpus.verses_iter() {
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
        // Exactly the expression the one-shot build used, evaluated per code
        // instead of per (verse, code): `as f64 as f32` and the division order are
        // preserved deliberately, because the tie test compares bits.
        let weights = counts
            .iter()
            .map(|(k, &c)| ((*k).to_string(), SIF_A / (SIF_A + c as f64 as f32 / total as f32)))
            .collect();
        let d = emb.dim;
        VerseSimBuilder {
            stage: SifStage::Rows,
            cursor: 0,
            dim: d,
            aligned: emb.aligned,
            weights,
            refs: Vec::new(),
            nt: Vec::new(),
            rows: Vec::new(),
            hebrew: Vec::new(),
            greek: Vec::new(),
            mu_h: vec![0.0; d],
            mu_g: vec![0.0; d],
            pc_h: vec![0.0; d],
            pc_g: vec![0.0; d],
            pc_v: None,
            pc_iter: 0,
            out: None,
        }
    }

    /// The finished model, once `step` has returned false.
    pub fn take(&mut self) -> Option<VerseSim> {
        self.out.take()
    }

    /// A code's SIF weight. Absent from the table means `p = 0`, which the
    /// one-shot form evaluated to `SIF_A / SIF_A` — exactly 1.0.
    fn weight(&self, code: &str) -> f32 {
        self.weights.get(code).copied().unwrap_or(1.0)
    }

    /// Do up to `budget` units of the next stage. Returns true while work remains.
    pub fn step(&mut self, emb: &Embedding, corpus: &Corpus, budget: usize) -> bool {
        let budget = budget.max(1);
        match self.stage {
            SifStage::Rows => {
                let len = corpus.len();
                let end = self.cursor.saturating_add(budget).min(len);
                let mut acc = vec![0.0f32; self.dim];
                for i in self.cursor..end {
                    let Some(v) = corpus.verse_at(i) else { continue };
                    // The SIF-weighted mean of this verse's in-vocabulary concept
                    // vectors, straight into a reused scratch buffer. The one-shot
                    // form built a `Vec<String>` of cloned codes per verse purely
                    // to hand them to a closure — ~31k allocations and as many
                    // String clones, for nothing.
                    acc.fill(0.0);
                    let mut n = 0usize;
                    let mut greek = false;
                    let mut first = true;
                    for t in &v.tokens {
                        for s in &t.strongs {
                            if first {
                                greek = s.starts_with('G');
                                first = false;
                            }
                            if let Some(cv) = emb.concept_vector(s) {
                                let w = self.weight(s);
                                for (a, x) in acc.iter_mut().zip(cv) {
                                    *a += w * x;
                                }
                                n += 1;
                            }
                        }
                    }
                    if n == 0 {
                        continue; // no in-vocabulary concept: this verse has no vector
                    }
                    let inv = 1.0 / n as f32;
                    for a in acc.iter_mut() {
                        *a *= inv;
                    }
                    let row = self.refs.len() as u32;
                    if greek {
                        self.greek.push(row);
                    } else {
                        self.hebrew.push(row);
                    }
                    self.refs.push(v.vref());
                    self.nt.push(greek);
                    self.rows.extend_from_slice(&acc);
                }
                self.cursor = end;
                if end < len {
                    return true;
                }
                self.enter(SifStage::Means)
            }
            SifStage::Means => {
                // Both sums in one pass, each in its own testament's row order —
                // the division by the count waits for the last row, so the result
                // matches summing the testaments separately.
                let end = self.cursor.saturating_add(budget).min(self.refs.len());
                for i in self.cursor..end {
                    let row = row_of(&self.rows, self.dim, i);
                    if self.nt[i] {
                        add_into(&mut self.mu_g, row);
                    } else {
                        add_into(&mut self.mu_h, row);
                    }
                }
                self.cursor = end;
                if end < self.refs.len() {
                    return true;
                }
                for (mu, n) in [(&mut self.mu_h, self.hebrew.len()), (&mut self.mu_g, self.greek.len())] {
                    if n == 0 {
                        continue; // stays all-zero, as the one-shot `mean_of` did
                    }
                    let inv = 1.0 / n as f32;
                    for a in mu.iter_mut() {
                        *a *= inv;
                    }
                }
                self.enter(SifStage::PcHebrew)
            }
            SifStage::PcHebrew => self.pc_step(false),
            SifStage::PcGreek => self.pc_step(true),
            SifStage::Adjust => {
                let end = self.cursor.saturating_add(budget).min(self.refs.len());
                for i in self.cursor..end {
                    let (mu, pc) = if self.nt[i] {
                        (&self.mu_g, &self.pc_g)
                    } else {
                        (&self.mu_h, &self.pc_h)
                    };
                    // In place: subtract the mean, remove the component's
                    // projection, normalise. The one-shot form allocated a fresh
                    // `Vec<f32>` per verse here and copied it into the output.
                    let row = &mut self.rows[i * self.dim..(i + 1) * self.dim];
                    for (x, m) in row.iter_mut().zip(mu) {
                        *x -= m;
                    }
                    let proj = dot(row, pc);
                    for (x, p) in row.iter_mut().zip(pc) {
                        *x -= proj * p;
                    }
                    normalize(row);
                }
                self.cursor = end;
                if end < self.refs.len() {
                    return true;
                }
                let refs = std::mem::take(&mut self.refs);
                let ix = refs.iter().cloned().zip(0..).collect();
                self.out = Some(VerseSim {
                    dim: self.dim,
                    refs,
                    nt: std::mem::take(&mut self.nt),
                    vecs: std::mem::take(&mut self.rows),
                    ix,
                    aligned: self.aligned,
                });
                self.enter(SifStage::Done)
            }
            SifStage::Done => false,
        }
    }

    /// One power iteration for a testament, or its whole set-up/tear-down.
    /// Below [`SIF_PC_MIN_VERSES`] the component stays zero — a direction fitted
    /// to a handful of verses is noise, and mean subtraction still runs.
    fn pc_step(&mut self, greek: bool) -> bool {
        let next_stage = if greek { SifStage::Adjust } else { SifStage::PcGreek };
        let (idx, mu) = if greek {
            (&self.greek, &self.mu_g)
        } else {
            (&self.hebrew, &self.mu_h)
        };
        if idx.len() < SIF_PC_MIN_VERSES {
            return self.enter(next_stage);
        }
        let pca = Pca { dim: self.dim, rows: &self.rows, idx, mu };
        let v = match self.pc_v.take() {
            Some(v) => v,
            None => match pca.seed() {
                Some(seed) => seed,
                None => return self.enter(next_stage), // every row zero: no component
            },
        };
        let next = pca.iterate(&v);
        let settled = pca_settled(&v, &next);
        self.pc_iter += 1;
        if settled || self.pc_iter >= SIF_PC_ITERS {
            // `settled` accepts `next`; running out of iterations accepts the
            // direction we came in with, exactly as the old loop's fallthrough did.
            let pc = if settled { next } else { v };
            if greek {
                self.pc_g = pc;
            } else {
                self.pc_h = pc;
            }
            return self.enter(next_stage);
        }
        self.pc_v = Some(next);
        true
    }

    /// Move to `stage` with a fresh cursor. Returns whether work remains.
    fn enter(&mut self, stage: SifStage) -> bool {
        self.stage = stage;
        self.cursor = 0;
        self.pc_v = None;
        self.pc_iter = 0;
        stage != SifStage::Done
    }
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
    /// vector per verse); build it once at startup.
    ///
    /// The native shells' one-shot path. It runs [`VerseSimBuilder`] to
    /// completion rather than having a second implementation, so the sliced build
    /// the web uses and this one cannot drift apart.
    pub fn build(emb: &Embedding, corpus: &Corpus) -> VerseSim {
        let mut b = VerseSimBuilder::new(emb, corpus);
        while b.step(emb, corpus, usize::MAX) {}
        b.take().unwrap_or_else(|| VerseSim {
            dim: emb.dim,
            refs: Vec::new(),
            nt: Vec::new(),
            vecs: Vec::new(),
            ix: HashMap::new(),
            aligned: emb.aligned,
        })
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
        let q = corpus.verse_at(0).unwrap().vref();
        let sim = vs.similar_verses_in(&q, 5);
        assert_eq!(sim.len(), 2);
        assert_eq!(sim[0].1, sim[1].1, "fixture must produce an exact tie");
        assert_eq!(sim[0].0, corpus.verse_at(1).unwrap().vref());
        assert_eq!(sim[1].0, corpus.verse_at(2).unwrap().vref());
    }

    // ── the packed `.vecb` form ────────────────────────────────────────────────

    /// The packed form must be the SAME embedding, not merely a similar one:
    /// identical dim/size/keys, identical vectors, identical neighbour answers
    /// and the same alias + freq behaviour. Compared through the API rather than
    /// by byte-equality with the text file, which would prove nothing about what
    /// a reader actually gets.
    #[test]
    fn packed_vectors_load_identically_to_the_text() {
        let text = emb();
        let bytes = encode_embedding_bin(VEC).expect("encodes");
        let packed = parse_embedding_bin("kjv1769-tok2", Some(META), &bytes, Some(FREQ)).unwrap();

        assert_eq!(packed.dim(), text.dim());
        assert_eq!(packed.size(), text.size());
        assert_eq!(packed.aligned(), text.aligned());
        assert_eq!(packed.has_trained_freq(), text.has_trained_freq());
        for key in ["G1", "G2", "H1", "H2"] {
            assert_eq!(
                packed.concept_vector(key),
                text.concept_vector(key),
                "{key} differs between the text and packed loaders"
            );
            assert_eq!(packed.nearest_concepts(key, 3), text.nearest_concepts(key, 3));
            assert_eq!(packed.cross_concepts(key, 3), text.cross_concepts(key, 3));
            assert_eq!(packed.freq_of(key), text.freq_of(key));
        }
        // The alias map lives in the meta, so it must survive the packed path too.
        assert_eq!(packed.concept_vector("G9"), packed.concept_vector("G1"));
    }

    /// Rows are stored RAW so the reader's own normalisation is what makes the
    /// two paths agree — a packed file of already-normalised rows would be
    /// double-normalised. Unit vectors either way, and the encoder's floats must
    /// be the pre-normalisation values.
    #[test]
    fn packed_rows_are_stored_unnormalised() {
        let bytes = encode_embedding_bin(VEC).unwrap();
        let floats_at = bytes.len() - 4 * 2 * 4; // 4 rows × dim 2 × f32
        let first: Vec<f32> = bytes[floats_at..floats_at + 8]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(first, vec![1.0, 0.0], "G1 stored as written, not normalised");
        // G2 is 0.9/0.1 raw; normalised it is not.
        let g2: Vec<f32> = bytes[floats_at + 8..floats_at + 16]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(g2, vec![0.9, 0.1]);
        let loaded = parse_embedding_bin("kjv1769-tok2", Some(META), &bytes, None).unwrap();
        let n: f32 = loaded.concept_vector("G2").unwrap().iter().map(|x| x * x).sum();
        assert!((n - 1.0).abs() < 1e-5, "loaded rows are unit length");
    }

    /// A foreign, truncated or stale packed file must read as "absent" so the
    /// caller falls back to the text rather than losing the feature.
    #[test]
    fn a_bad_packed_file_is_none_not_garbage() {
        let good = encode_embedding_bin(VEC).unwrap();
        assert!(parse_embedding_bin("kjv1769-tok2", Some(META), b"not a vecb at all", None).is_none());
        assert!(parse_embedding_bin("kjv1769-tok2", Some(META), &good[..good.len() - 8], None).is_none());
        assert!(parse_embedding_bin("kjv1769-tok2", Some(META), &[], None).is_none());
        // Stale tokenization is refused for the packed form exactly as for text.
        let stale = r#"{"format":"overlay-embedding-meta-v1","tokenization":"kjv1611-tok1"}"#;
        assert!(parse_embedding_bin("kjv1769-tok2", Some(stale), &good, None).is_none());
    }

    #[test]
    fn power_iteration_finds_the_dominant_axis() {
        // Points strung along the x-axis (with y jitter) → top PC ≈ ±x. Row-major
        // and flat now, with a zero mean so this still tests the maths rather
        // than the centring.
        let rows: Vec<f32> = vec![3.0, 0.1, -2.0, -0.1, 5.0, 0.05, -4.0, 0.0, 1.0, -0.05];
        let idx: Vec<u32> = (0..5).collect();
        let mu = vec![0.0f32; 2];
        let pca = Pca { dim: 2, rows: &rows, idx: &idx, mu: &mu };
        let mut v = pca.seed().expect("a non-zero row to seed from");
        for _ in 0..SIF_PC_ITERS {
            let next = pca.iterate(&v);
            let settled = pca_settled(&v, &next);
            v = next;
            if settled {
                break;
            }
        }
        assert!(v[0].abs() > 0.98, "dominant axis should be x, got {v:?}");
        assert!(v[1].abs() < 0.2);
        // Unit length.
        assert!((v.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-4);
    }

    /// A corpus of `n` Hebrew verses then `n` Greek ones over the fixture vocab.
    /// Canonical order is enforced by the loader, so Genesis comes first and each
    /// book's verses are contiguous.
    fn two_testament_corpus(n: usize) -> Corpus {
        let mut jsonl = format!("{{\"tokenization\":\"kjv1769-tok2\",\"verses\":{}}}\n", n * 2);
        for i in 0..n {
            let h = if i % 3 == 0 { "H1" } else { "H2" };
            jsonl.push_str(&format!(
                "{{\"b\":\"Gen\",\"c\":1,\"v\":{},\"t\":[[\"\",\"w\",\"\",[\"{h}\"],0]]}}\n",
                i + 1
            ));
        }
        for i in 0..n {
            let g = if i % 2 == 0 { "G1" } else { "G2" };
            jsonl.push_str(&format!(
                "{{\"b\":\"John\",\"c\":1,\"v\":{},\"t\":[[\"\",\"w\",\"\",[\"{g}\"],0]]}}\n",
                i + 1
            ));
        }
        plumbline_core::corpus::from_str(&jsonl).unwrap()
    }

    /// A saved model must answer exactly like the one it was saved from.
    ///
    /// Checked through the API a reader actually reaches — the neighbours, in
    /// order, for queries in both testaments — and not by re-encoding and
    /// comparing bytes, which would only prove `encode` is deterministic while
    /// saying nothing about whether `decode` reconstructed a working model.
    #[test]
    fn a_saved_sif_model_answers_exactly_like_the_built_one() {
        let corpus = two_testament_corpus(SIF_PC_MIN_VERSES + 20);
        let e = emb();
        let built = VerseSim::build(&e, &corpus);

        let bytes = built.encode("kjv1769-tok2/packv1");
        let back = VerseSim::decode(&bytes, "kjv1769-tok2/packv1").expect("round-trips");

        assert_eq!(back.count(), built.count());
        assert_eq!(back.dim, built.dim);
        assert_eq!(back.aligned(), built.aligned());
        assert_eq!(back.refs, built.refs);
        assert_eq!(back.nt, built.nt);
        assert_eq!(back.vecs, built.vecs, "f32 round-trip is exact or it is not a cache");
        for q in [VRef::new("John", 1, 1), VRef::new("Gen", 1, 1), VRef::new("Gen", 1, 7)] {
            assert_eq!(
                back.similar_verses_in(&q, 5),
                built.similar_verses_in(&q, 5),
                "same-testament neighbours differ for {q:?}"
            );
            assert_eq!(
                back.similar_verses_cross(&q, 5),
                built.similar_verses_cross(&q, 5),
                "cross-testament neighbours differ for {q:?}"
            );
        }
    }

    /// A model saved against different data must be REFUSED, not served.
    ///
    /// This is the failure mode that has no symptom: the vectors decode fine and
    /// every "verses like this" answer is quietly keyed to a corpus the reader is
    /// not reading. Nothing throws, nothing looks wrong, and the answers are
    /// simply the wrong verses. A stamp mismatch is therefore "no cache", never
    /// "a cache to repair".
    #[test]
    fn a_sif_model_from_other_data_is_refused() {
        let corpus = two_testament_corpus(SIF_PC_MIN_VERSES + 20);
        let bytes = VerseSim::build(&emb(), &corpus).encode("kjv1769-tok2/packv1");

        assert!(VerseSim::decode(&bytes, "kjv1769-tok2/packv2").is_none(), "newer pack");
        assert!(VerseSim::decode(&bytes, "kjv1611-tok1/packv1").is_none(), "other tokenization");
        assert!(VerseSim::decode(&bytes, "").is_none(), "no stamp at all");
        assert!(VerseSim::decode(&bytes, "kjv1769-tok2/packv1").is_some(), "the right one still works");
    }

    /// Storage hands back whatever it hands back. Truncation, a foreign file and
    /// a lying header must all be `None` rather than a panic or a wrong model.
    #[test]
    fn a_damaged_sif_model_is_none_not_garbage() {
        let corpus = two_testament_corpus(SIF_PC_MIN_VERSES + 20);
        let good = VerseSim::build(&emb(), &corpus).encode("s");

        assert!(VerseSim::decode(&[], "s").is_none(), "empty");
        assert!(VerseSim::decode(b"not a sif model at all", "s").is_none(), "foreign bytes");
        // Every truncation point, so no length is trusted without being checked.
        for cut in [8, 12, 17, 21, 40, good.len() / 3, good.len() / 2, good.len() - 1] {
            assert!(VerseSim::decode(&good[..cut], "s").is_none(), "truncated at {cut}");
        }
        // A header claiming far more rows than the body carries.
        let mut lying = good.clone();
        lying[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(VerseSim::decode(&lying, "s").is_none(), "a count the body cannot back");
        assert!(VerseSim::decode(&good, "s").is_some(), "and the intact one still loads");
    }

    /// Slicing must not change the ANSWER.
    ///
    /// Not a formality: floating-point addition is not associative, so "the same
    /// maths in smaller chunks" is a claim that has to be tested rather than
    /// assumed — and the sliced path is the one the web actually runs, while every
    /// other test in this file exercises the one-shot `build`. A builder that
    /// finalised a mean before its last row, or restarted a power iteration on
    /// resume, would pass every other test here and quietly serve different
    /// neighbours on the web than on Android.
    #[test]
    fn sliced_build_matches_one_shot() {
        // Above SIF_PC_MIN_VERSES per testament, so the power iteration really
        // runs — below it the component is zeroed and the interesting stage is
        // skipped entirely, which would make this test vacuous.
        let corpus = two_testament_corpus(SIF_PC_MIN_VERSES + 20);
        let e = emb();

        let one_shot = VerseSim::build(&e, &corpus);

        // A verse at a time: every stage boundary is crossed mid-work.
        let mut b = VerseSimBuilder::new(&e, &corpus);
        let mut steps = 0;
        while b.step(&e, &corpus, 1) {
            steps += 1;
            assert!(steps < 100_000, "sliced build did not terminate");
        }
        let sliced = b.take().expect("the sliced build produced a model");

        assert!(steps > 10, "the slicing was not actually exercised ({steps} steps)");
        assert_eq!(sliced.count(), one_shot.count());
        assert_eq!(sliced.dim, one_shot.dim);
        assert_eq!(sliced.nt, one_shot.nt);
        assert_eq!(sliced.refs, one_shot.refs);
        assert_eq!(sliced.aligned, one_shot.aligned);
        // BIT-for-bit, not approximately: an epsilon comparison here would hide
        // exactly the reassociation this test exists to catch.
        assert_eq!(sliced.vecs, one_shot.vecs, "sliced vectors differ from the one-shot build");

        // And the thing a reader would notice: the same neighbours, in the same
        // order, for a query in each testament.
        for q in [VRef::new("John", 1, 1), VRef::new("Gen", 1, 1)] {
            assert_eq!(
                sliced.similar_verses_in(&q, 5),
                one_shot.similar_verses_in(&q, 5),
                "neighbours differ for {q:?}"
            );
        }
    }
}

