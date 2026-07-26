//! The symbolic concept engine: derived, era-neutral statistics over the
//! Strong's-tagged corpus — counting and relating *lemmas*, never the 1769
//! English surface, so it cannot drift with modern English.
//!
//! Ported from overlay `Concept.hs` (the reader-facing core: per-concept
//! occurrence stats, book distribution, and the co-occurrence → PPMI →
//! mutual-kNN → label-propagation *community* graph that backs the concept
//! neighbourhood diagram). The keyness / quotation-candidate / chain machinery
//! is the offline-analysis side and is not ported here.
//!
//! No ML data: everything is a fold over the corpus. To bound memory, only the
//! mutual-kNN collocation edges are retained (the full PPMI matrix is dropped
//! after the kNN filter).

use std::collections::{HashMap, HashSet};

use plumbline_core::corpus::Corpus;
use plumbline_core::reference::OT_NT_DIVIDE;
use plumbline_core::canon;

/// How many of a concept's strongest cross-testament partners feed the
/// dispersion strip's "bridge" row — kept small so the row shows the
/// equivalents that matter (Christ↔Messiah), not every faint lexical echo.
pub const BRIDGE_ROW_PARTNERS: usize = 6;

/// A Strong's number's occurrence statistics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConceptStat {
    /// Token instances corpus-wide (a token tagged with several codes counts
    /// once for each).
    pub total: u32,
    /// Token instances per OSIS book id.
    pub by_book: HashMap<String, u32>,
}

/// Strong's number → its statistics.
pub type ConceptIx = HashMap<String, ConceptStat>;

/// Build the per-concept index in one fold over the corpus.
pub fn build_concept_ix(corpus: &Corpus) -> ConceptIx {
    let mut ix: ConceptIx = HashMap::new();
    for v in corpus.verses() {
        for t in &v.tokens {
            for s in &t.strongs {
                let e = ix.entry(s.clone()).or_default();
                e.total += 1;
                *e.by_book.entry(v.book.clone()).or_insert(0) += 1;
            }
        }
    }
    ix
}

/// The `n` books a concept occurs in most, strongest first.
pub fn top_books(stat: &ConceptStat, n: usize) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = stat.by_book.iter().map(|(b, c)| (b.clone(), *c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

/// (OT, NT) instance split by testament.
pub fn testament_split(stat: &ConceptStat) -> (u32, u32) {
    let (mut ot, mut nt) = (0u32, 0u32);
    for (b, c) in &stat.by_book {
        if is_nt(b) {
            nt += c;
        } else {
            ot += c;
        }
    }
    (ot, nt)
}

fn is_nt(book: &str) -> bool {
    canon::book_order(book).is_some_and(|o| o >= OT_NT_DIVIDE)
}

// ── co-occurrence graph ────────────────────────────────────────────────────────
//
// The pipeline (co-occurrence → PPMI → mutual-kNN → label propagation) runs
// end-to-end over interned `u32` ids with unordered pairs packed into `u64`
// keys: at corpus scale every stage visits ~600k distinct pairs, and carrying
// `String` pair keys between stages made this warm-up the most expensive block
// of a cold boot (890 ms native, several seconds on a phone — TODO #28). Ids
// are assigned in **lexicographic order** of the code string, so id
// comparisons and string comparisons agree everywhere: pair canonicalization
// (`a < b`) and every ordering tie-break below match the String-keyed public
// wrappers bit for bit.

/// The interned corpus: sorted vocabulary + everything the pipeline consumes.
struct IdGraph {
    /// id → code; ids follow the lexicographic order of the code strings.
    names: Vec<String>,
    /// Packed unordered pair (`lo << 32 | hi`, lo < hi) → shared-verse count.
    co: HashMap<u64, u32>,
    /// id → distinct-verse frequency.
    df: Vec<u32>,
    n_verses: usize,
}

fn pack(a: u32, b: u32) -> u64 {
    (u64::from(a) << 32) | u64::from(b)
}
fn unpack(key: u64) -> (u32, u32) {
    ((key >> 32) as u32, (key & 0xFFFF_FFFF) as u32)
}
fn name_pair(names: &[String], key: u64) -> (String, String) {
    let (a, b) = unpack(key);
    (names[a as usize].clone(), names[b as usize].clone())
}

/// Two passes over the corpus: the sorted vocabulary, then per-verse pair
/// counts + verse frequency, all in id space.
fn intern_corpus(corpus: &Corpus) -> IdGraph {
    let mut vocab: HashSet<&str> = HashSet::new();
    for v in corpus.verses() {
        for t in &v.tokens {
            for s in &t.strongs {
                vocab.insert(s.as_str());
            }
        }
    }
    let mut sorted: Vec<&str> = vocab.into_iter().collect();
    sorted.sort_unstable();
    let id_of: HashMap<&str, u32> = sorted.iter().enumerate().map(|(i, &s)| (s, i as u32)).collect();

    let mut co: HashMap<u64, u32> = HashMap::new();
    let mut df = vec![0u32; sorted.len()];
    let mut present: Vec<u32> = Vec::new();
    for v in corpus.verses() {
        present.clear();
        present.extend(v.tokens.iter().flat_map(|t| &t.strongs).map(|s| id_of[s.as_str()]));
        present.sort_unstable(); // id order == string order
        present.dedup();
        for (i, &a) in present.iter().enumerate() {
            df[a as usize] += 1;
            for &b in &present[i + 1..] {
                *co.entry(pack(a, b)).or_insert(0) += 1;
            }
        }
    }
    IdGraph { names: sorted.into_iter().map(String::from).collect(), co, df, n_verses: corpus.verses().len() }
}

/// Intern a String-pair edge map for the public wrappers (ids lexicographic,
/// pairs canonicalized by id — identical to canonicalizing by string).
fn intern_edges<V: Copy>(edges: &HashMap<(String, String), V>) -> (Vec<String>, HashMap<u64, V>) {
    let mut names: Vec<&str> = edges.keys().flat_map(|(a, b)| [a.as_str(), b.as_str()]).collect();
    names.sort_unstable();
    names.dedup();
    let id_of: HashMap<&str, u32> = names.iter().enumerate().map(|(i, &s)| (s, i as u32)).collect();
    let ided = edges
        .iter()
        .map(|((a, b), v)| {
            let (x, y) = (id_of[a.as_str()], id_of[b.as_str()]);
            (pack(x.min(y), x.max(y)), *v)
        })
        .collect();
    (names.into_iter().map(String::from).collect(), ided)
}

fn ppmi_ids(n_verses: usize, df: &[u32], co: &HashMap<u64, u32>) -> HashMap<u64, f32> {
    let n = n_verses.max(1) as f64;
    let df_of = |i: u32| f64::from(df.get(i as usize).copied().unwrap_or(1).max(1));
    let mut out = HashMap::new();
    for (&key, &c) in co {
        let (a, b) = unpack(key);
        let v = (f64::from(c) * n / (df_of(a) * df_of(b))).ln();
        if v > 0.0 {
            out.insert(key, v as f32);
        }
    }
    out
}

fn mutual_knn_ids(k: usize, n_ids: usize, edges: &HashMap<u64, f32>) -> HashMap<u64, f32> {
    // Each node's top-k neighbour set.
    let mut nbrs: Vec<Vec<(u32, f32)>> = vec![Vec::new(); n_ids];
    for (&key, &w) in edges {
        let (a, b) = unpack(key);
        nbrs[a as usize].push((b, w));
        nbrs[b as usize].push((a, w));
    }
    let top: Vec<HashSet<u32>> = nbrs
        .into_iter()
        .map(|mut list| {
            list.sort_by(|x, y| y.1.total_cmp(&x.1));
            list.truncate(k);
            list.into_iter().map(|(n, _)| n).collect()
        })
        .collect();
    edges
        .iter()
        .filter(|(&key, _)| {
            let (a, b) = unpack(key);
            top[a as usize].contains(&b) && top[b as usize].contains(&a)
        })
        .map(|(&key, &w)| (key, w))
        .collect()
}

fn communities_ids(max_rounds: usize, n_ids: usize, edges: &HashMap<u64, f32>) -> Vec<Vec<u32>> {
    // Weighted adjacency; ids with no edges never participate.
    let mut adj: Vec<Vec<(u32, f32)>> = vec![Vec::new(); n_ids];
    for (&key, &w) in edges {
        let (a, b) = unpack(key);
        adj[a as usize].push((b, w));
        adj[b as usize].push((a, w));
    }
    // Label per node (start: itself), updated synchronously per round.
    let mut labels: Vec<u32> = (0..n_ids as u32).collect();
    for _ in 0..max_rounds {
        let mut next = labels.clone();
        let mut changed = false;
        for v in 0..n_ids {
            if adj[v].is_empty() {
                continue;
            }
            // Sum edge weight per neighbouring label.
            let mut pull: HashMap<u32, f32> = HashMap::new();
            for &(u, w) in &adj[v] {
                *pull.entry(labels[u as usize]).or_insert(0.0) += w;
            }
            let best = pull.values().copied().fold(f32::MIN, f32::max);
            // Smallest label among the heaviest — order-independent, and id
            // order == string order, so this matches the String tie-break.
            let chosen = pull
                .iter()
                .filter(|(_, w)| **w >= best)
                .map(|(l, _)| *l)
                .min()
                .unwrap_or(labels[v]);
            if chosen != next[v] {
                next[v] = chosen;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        labels = next;
    }

    let mut grouped: HashMap<u32, Vec<u32>> = HashMap::new();
    for v in 0..n_ids {
        if !adj[v].is_empty() {
            grouped.entry(labels[v]).or_default().push(v as u32);
        }
    }
    let mut out: Vec<Vec<u32>> = grouped.into_values().filter(|g| g.len() >= 3).collect();
    for g in &mut out {
        g.sort_unstable(); // == lexicographic member order
    }
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.first().cmp(&b.first())));
    out
}

/// Undirected co-occurrence counts: how many verses each unordered code pair
/// shares. Pairs are stored canonically (`a < b`). (String-keyed wrapper over
/// the id-space pipeline; [`Concept::build`] stays in id space throughout.)
pub fn co_occurrence(corpus: &Corpus) -> HashMap<(String, String), u32> {
    let g = intern_corpus(corpus);
    g.co.iter().map(|(&key, &c)| (name_pair(&g.names, key), c)).collect()
}

/// Verse frequency: how many distinct verses each code appears in.
pub fn verse_frequency(corpus: &Corpus) -> HashMap<String, u32> {
    let g = intern_corpus(corpus);
    g.names.into_iter().zip(g.df).collect()
}

/// Positive pointwise mutual information over the co-occurrence counts, keyed by
/// verse frequency. Only positive associations are kept.
pub fn ppmi(
    n_verses: usize,
    df: &HashMap<String, u32>,
    co: &HashMap<(String, String), u32>,
) -> HashMap<(String, String), f32> {
    let (names, co_ids) = intern_edges(co);
    let df_vec: Vec<u32> = names.iter().map(|n| df.get(n).copied().unwrap_or(1)).collect();
    ppmi_ids(n_verses, &df_vec, &co_ids)
        .iter()
        .map(|(&key, &w)| (name_pair(&names, key), w))
        .collect()
}

/// Keep only mutual top-`k` edges: `(a,b)` survives iff `b` is among `a`'s
/// `k` strongest neighbours *and* vice versa.
pub fn mutual_knn(
    k: usize,
    edges: &HashMap<(String, String), f32>,
) -> HashMap<(String, String), f32> {
    let (names, ids) = intern_edges(edges);
    mutual_knn_ids(k, names.len(), &ids)
        .iter()
        .map(|(&key, &w)| (name_pair(&names, key), w))
        .collect()
}

/// Label-propagation communities over an edge graph: groups of ≥3 codes,
/// largest first. Deterministic (ties break to the smallest label among the
/// heaviest), bounded to `max_rounds`.
pub fn communities(max_rounds: usize, edges: &HashMap<(String, String), f32>) -> Vec<Vec<String>> {
    let (names, ids) = intern_edges(edges);
    communities_ids(max_rounds, names.len(), &ids)
        .into_iter()
        .map(|g| g.into_iter().map(|i| names[i as usize].clone()).collect())
        .collect()
}

// ── the assembled engine ───────────────────────────────────────────────────────

/// The concept engine: per-concept stats plus the retained mutual-kNN
/// collocation graph and its communities. Built once over the corpus.
#[derive(Debug, Clone)]
pub struct Concept {
    ix: ConceptIx,
    /// code → its kNN collocates (code, ppmi), strongest first.
    collocates: HashMap<String, Vec<(String, f32)>>,
    communities: Vec<Vec<String>>,
    /// code → index into `communities`, when it belongs to one.
    community_of: HashMap<String, usize>,
}

/// How many mutual nearest neighbours seed the collocation graph.
const KNN: usize = 10;
/// Label-propagation round cap.
const COMMUNITY_ROUNDS: usize = 30;

impl Concept {
    pub fn build(corpus: &Corpus) -> Concept {
        let ix = build_concept_ix(corpus);
        // The whole pipeline stays in id space (see the module note on
        // interning); only the small retained results become Strings.
        let IdGraph { names, co, df, n_verses } = intern_corpus(corpus);
        let edges = ppmi_ids(n_verses, &df, &co);
        drop(co); // free the dense count matrix
        let knn = mutual_knn_ids(KNN, names.len(), &edges);
        drop(edges);
        let comm_ids = communities_ids(COMMUNITY_ROUNDS, names.len(), &knn);

        let mut collocates: HashMap<String, Vec<(String, f32)>> = HashMap::new();
        for (&key, &w) in &knn {
            let (a, b) = unpack(key);
            let (a, b) = (&names[a as usize], &names[b as usize]);
            collocates.entry(a.clone()).or_default().push((b.clone(), w));
            collocates.entry(b.clone()).or_default().push((a.clone(), w));
        }
        for list in collocates.values_mut() {
            list.sort_by(|x, y| y.1.total_cmp(&x.1));
        }
        let communities: Vec<Vec<String>> = comm_ids
            .into_iter()
            .map(|g| g.into_iter().map(|i| names[i as usize].clone()).collect())
            .collect();
        let mut community_of = HashMap::new();
        for (i, grp) in communities.iter().enumerate() {
            for code in grp {
                community_of.insert(code.clone(), i);
            }
        }
        Concept { ix, collocates, communities, community_of }
    }

    pub fn stat(&self, code: &str) -> Option<&ConceptStat> {
        self.ix.get(code)
    }

    /// Union the per-book dispersion of several codes, summing counts book by
    /// book. Codes with no stats contribute nothing; the keys are OSIS book ids
    /// like [`ConceptStat::by_book`]. Used to lay a concept's cross-testament
    /// partners' occurrences — the dispersion strip's "bridge" row — over its
    /// own, so viewing *Christ* (G5547) reveals where *Messiah* (H4899) occurs.
    pub fn union_by_book<'a>(&self, codes: impl IntoIterator<Item = &'a str>) -> HashMap<String, u32> {
        let mut acc: HashMap<String, u32> = HashMap::new();
        for code in codes {
            if let Some(s) = self.ix.get(code) {
                for (book, cnt) in &s.by_book {
                    *acc.entry(book.clone()).or_insert(0) += *cnt;
                }
            }
        }
        acc
    }

    /// The `n` books this concept occurs in most.
    pub fn top_books(&self, code: &str, n: usize) -> Vec<(String, u32)> {
        self.ix.get(code).map(|s| top_books(s, n)).unwrap_or_default()
    }

    /// (OT, NT) occurrence split.
    pub fn testament_split(&self, code: &str) -> (u32, u32) {
        self.ix.get(code).map(testament_split).unwrap_or((0, 0))
    }

    /// The strongest collocates of a code (mutual-kNN, by PPMI), function
    /// words excluded (see [`crate::stopwords`]).
    pub fn collocates(&self, code: &str, k: usize) -> Vec<(String, f32)> {
        self.collocates
            .get(code)
            .map(|v| {
                v.iter()
                    .filter(|(c, _)| !crate::stopwords::is_function_word(c))
                    .take(k)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The collocation community `code` belongs to (its co-occurring field),
    /// excluding the code itself and any function words; empty if it's in none.
    pub fn community(&self, code: &str) -> Vec<String> {
        match self.community_of.get(code) {
            Some(&i) => self.communities[i]
                .iter()
                .filter(|c| c.as_str() != code && !crate::stopwords::is_function_word(c))
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }

    /// Total number of communities found.
    pub fn community_count(&self) -> usize {
        self.communities.len()
    }
}

/// The concept map's spokes: the embedding neighbours (`near`, semantic — drawn
/// gold) unioned with the collocation `community` (green), each capped at `n`
/// and deduped so a code that is both stays a single **semantic** spoke (near
/// wins). `near`/`community` are the raw code lists the callers already hold
/// (from the embedding + this engine); passing them in keeps this helper free
/// of the embedding dependency.
///
/// The one spoke assembly behind the concept-map popup (review item 4): GTK
/// calls it directly; the non-Rust shells get the same spokes (with labels
/// pre-baked) through `plumbline_engine_concept_map_json`.
pub fn radial_spokes(near: &[String], community: &[String], n: usize) -> Vec<(String, bool)> {
    let mut spokes: Vec<(String, bool)> = Vec::new();
    for c in near.iter().take(n) {
        spokes.push((c.clone(), true));
    }
    for c in community.iter().take(n) {
        if !spokes.iter().any(|(x, _)| x == c) {
            spokes.push((c.clone(), false));
        }
    }
    spokes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pairs: &[(&str, &str, f32)]) -> HashMap<(String, String), f32> {
        pairs.iter().map(|(a, b, w)| ((a.to_string(), b.to_string()), *w)).collect()
    }

    #[test]
    fn top_books_and_split() {
        let mut s = ConceptStat::default();
        s.total = 10;
        s.by_book = [("Gen", 5), ("John", 3), ("Ps", 2)].iter().map(|(b, c)| (b.to_string(), *c)).collect();
        assert_eq!(top_books(&s, 2), vec![("Gen".into(), 5), ("John".into(), 3)]);
        // Gen + Ps are OT (8), John is NT (2).
        assert_eq!(testament_split(&s), (7, 3));
    }

    #[test]
    fn radial_spokes_union_semantic_wins_and_caps() {
        let near = vec!["G1".to_string(), "G2".to_string(), "G3".to_string()];
        // G2 is in both — it must stay a single semantic spoke; extra community
        // members past the cap are dropped.
        let community = vec!["G2".to_string(), "G4".to_string(), "G5".to_string(), "G6".to_string()];
        let spokes = radial_spokes(&near, &community, 3);
        assert_eq!(
            spokes,
            vec![
                ("G1".to_string(), true),
                ("G2".to_string(), true),
                ("G3".to_string(), true),
                ("G4".to_string(), false),
                ("G5".to_string(), false),
            ]
        );
        // G2 appears once, and as semantic.
        assert_eq!(spokes.iter().filter(|(c, _)| c == "G2").count(), 1);
        // Community is capped at n=3 before dedup, so G6 never appears.
        assert!(!spokes.iter().any(|(c, _)| c == "G6"));
    }

    #[test]
    fn co_occurrence_pairs_are_canonical_and_counted_per_verse() {
        // Verse 1 introduces H5 then H9 (interner ids 0, 1); verse 2 introduces
        // H1 (id 2), which sorts *before* H5 as a string — so id order and
        // string order disagree, and a pair canonicalized by id instead of by
        // string would come out as ("H5","H1"). Verse 2 also repeats H5 across
        // tokens to exercise the per-verse dedup (one increment, not two).
        let jsonl = concat!(
            "{\"tokenization\":\"t\",\"verses\":3}\n",
            "{\"b\":\"Gen\",\"c\":1,\"v\":1,\"t\":[[\"\",\"w\",\"\",[\"H9\",\"H5\"],0]]}\n",
            "{\"b\":\"Gen\",\"c\":1,\"v\":2,\"t\":[[\"\",\"w\",\"\",[\"H5\"],0],[\"\",\"w\",\"\",[\"H1\",\"H5\"],0]]}\n",
            "{\"b\":\"Gen\",\"c\":1,\"v\":3,\"t\":[[\"\",\"w\",\"\",[\"H1\"],0],[\"\",\"w\",\"\",[\"H5\"],0]]}\n",
        );
        let corpus = plumbline_core::corpus::from_str(jsonl).unwrap();
        let co = co_occurrence(&corpus);
        let key = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(co.get(&key("H5", "H9")).copied(), Some(1));
        assert_eq!(co.get(&key("H1", "H5")).copied(), Some(2)); // canonical a < b, verse-deduped
        assert_eq!(co.len(), 2);
    }

    #[test]
    fn ppmi_is_positive_only() {
        let df: HashMap<String, u32> = [("A", 10), ("B", 10), ("C", 2)].iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let co: HashMap<(String, String), u32> =
            [(("A", "B"), 1u32), (("A", "C"), 2)].iter().map(|((a, b), c)| ((a.to_string(), b.to_string()), *c)).collect();
        let p = ppmi(100, &df, &co);
        // A·C are rare + co-occur → positive; A·B common but weak → check A·C kept.
        assert!(p.contains_key(&("A".to_string(), "C".to_string())));
    }

    #[test]
    fn mutual_knn_keeps_reciprocal_only() {
        // A-B strong both ways; A-C only strong from A (C prefers D).
        let edges = m(&[("A", "B", 5.0), ("A", "C", 1.0), ("C", "D", 9.0), ("C", "E", 8.0)]);
        let knn = mutual_knn(1, &edges);
        assert!(knn.contains_key(&("A".into(), "B".into())));
        assert!(!knn.contains_key(&("A".into(), "C".into()))); // not reciprocal at k=1
    }

    #[test]
    fn communities_group_connected_clusters() {
        // Triangle A-B-C (one community of 3) + a separate pair D-E (dropped, <3).
        let edges = m(&[("A", "B", 1.0), ("B", "C", 1.0), ("A", "C", 1.0), ("D", "E", 1.0)]);
        let comms = communities(20, &edges);
        assert_eq!(comms.len(), 1);
        assert_eq!(comms[0], vec!["A".to_string(), "B".to_string(), "C".to_string()]);
    }
}
