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

use pure_core::corpus::Corpus;
use pure_core::reference::OT_NT_DIVIDE;
use pure_core::canon;

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

/// Undirected co-occurrence counts: how many verses each unordered code pair
/// shares. Pairs are stored canonically (`a < b`).
///
/// Internally the codes are interned to `u32` ids and pairs counted under a
/// packed `u64` key, so the per-pair inner loop (quadratic per verse, across
/// the whole corpus) allocates nothing; the `String`-keyed map is materialized
/// once at the end, one pair of clones per *distinct* pair. Because `present`
/// is sorted (by code string) before pairing, `(i, j)` with `i < j` is already
/// the canonical `a < b` order — the id pair inherits it regardless of the
/// interner's first-seen id assignment.
pub fn co_occurrence(corpus: &Corpus) -> HashMap<(String, String), u32> {
    let mut names: Vec<&str> = Vec::new();
    let mut id_of: HashMap<&str, u32> = HashMap::new();
    let mut counts: HashMap<u64, u32> = HashMap::new();
    let mut present_ids: Vec<u32> = Vec::new();
    for v in corpus.verses() {
        let mut present: Vec<&String> = v.tokens.iter().flat_map(|t| &t.strongs).collect();
        present.sort();
        present.dedup();
        present_ids.clear();
        present_ids.extend(present.iter().map(|s| {
            *id_of.entry(s.as_str()).or_insert_with(|| {
                names.push(s.as_str());
                (names.len() - 1) as u32
            })
        }));
        for i in 0..present_ids.len() {
            for j in (i + 1)..present_ids.len() {
                let key = (u64::from(present_ids[i]) << 32) | u64::from(present_ids[j]);
                *counts.entry(key).or_insert(0) += 1;
            }
        }
    }
    counts
        .into_iter()
        .map(|(key, c)| {
            let a = names[(key >> 32) as usize].to_string();
            let b = names[(key & 0xFFFF_FFFF) as usize].to_string();
            ((a, b), c)
        })
        .collect()
}

/// Verse frequency: how many distinct verses each code appears in.
pub fn verse_frequency(corpus: &Corpus) -> HashMap<String, u32> {
    let mut m: HashMap<String, u32> = HashMap::new();
    for v in corpus.verses() {
        let mut seen: HashSet<&String> = HashSet::new();
        for t in &v.tokens {
            for s in &t.strongs {
                if seen.insert(s) {
                    *m.entry(s.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    m
}

/// Positive pointwise mutual information over the co-occurrence counts, keyed by
/// verse frequency. Only positive associations are kept.
pub fn ppmi(
    n_verses: usize,
    df: &HashMap<String, u32>,
    co: &HashMap<(String, String), u32>,
) -> HashMap<(String, String), f32> {
    let n = n_verses.max(1) as f64;
    let df_of = |s: &str| df.get(s).copied().unwrap_or(1).max(1) as f64;
    let mut out = HashMap::new();
    for ((a, b), c) in co {
        let v = (*c as f64 * n / (df_of(a) * df_of(b))).ln();
        if v > 0.0 {
            out.insert((a.clone(), b.clone()), v as f32);
        }
    }
    out
}

/// Keep only mutual top-`k` edges: `(a,b)` survives iff `b` is among `a`'s
/// `k` strongest neighbours *and* vice versa.
pub fn mutual_knn(
    k: usize,
    edges: &HashMap<(String, String), f32>,
) -> HashMap<(String, String), f32> {
    // Each node's top-k neighbour set.
    let mut nbrs: HashMap<&str, Vec<(&str, f32)>> = HashMap::new();
    for ((a, b), w) in edges {
        nbrs.entry(a).or_default().push((b, *w));
        nbrs.entry(b).or_default().push((a, *w));
    }
    let top: HashMap<&str, HashSet<&str>> = nbrs
        .into_iter()
        .map(|(node, mut list)| {
            list.sort_by(|x, y| y.1.total_cmp(&x.1));
            list.truncate(k);
            (node, list.into_iter().map(|(n, _)| n).collect())
        })
        .collect();
    edges
        .iter()
        .filter(|((a, b), _)| {
            top.get(a.as_str()).is_some_and(|s| s.contains(b.as_str()))
                && top.get(b.as_str()).is_some_and(|s| s.contains(a.as_str()))
        })
        .map(|((a, b), w)| ((a.clone(), b.clone()), *w))
        .collect()
}

/// Label-propagation communities over an edge graph: groups of ≥3 codes,
/// largest first. Deterministic (ties break to the smallest label among the
/// heaviest), bounded to `max_rounds`.
pub fn communities(max_rounds: usize, edges: &HashMap<(String, String), f32>) -> Vec<Vec<String>> {
    // Weighted adjacency.
    let mut adj: HashMap<&str, Vec<(&str, f32)>> = HashMap::new();
    for ((a, b), w) in edges {
        adj.entry(a).or_default().push((b, *w));
        adj.entry(b).or_default().push((a, *w));
    }
    // Label per node (start: itself).
    let mut labels: HashMap<&str, &str> = adj.keys().map(|&n| (n, n)).collect();

    for _ in 0..max_rounds {
        let mut next: HashMap<&str, &str> = HashMap::with_capacity(labels.len());
        for (&v, nbrs) in &adj {
            // Sum edge weight per neighbouring label.
            let mut pull: HashMap<&str, f32> = HashMap::new();
            for (u, w) in nbrs {
                *pull.entry(labels[u]).or_insert(0.0) += *w;
            }
            let chosen = match pull.iter().map(|(_, w)| *w).fold(f32::MIN, f32::max) {
                best if best > f32::MIN => {
                    // Smallest label among the heaviest — order-independent.
                    pull.iter()
                        .filter(|(_, w)| **w >= best)
                        .map(|(l, _)| *l)
                        .min()
                        .unwrap_or(labels[v])
                }
                _ => labels[v],
            };
            next.insert(v, chosen);
        }
        if next == labels {
            break;
        }
        labels = next;
    }

    let mut grouped: HashMap<&str, Vec<String>> = HashMap::new();
    for (v, lbl) in &labels {
        grouped.entry(lbl).or_default().push(v.to_string());
    }
    let mut out: Vec<Vec<String>> = grouped
        .into_values()
        .filter(|g| g.len() >= 3)
        .map(|mut g| {
            g.sort();
            g
        })
        .collect();
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.first().cmp(&b.first())));
    out
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
        let n_verses = corpus.verses().len();
        let df = verse_frequency(corpus);
        let co = co_occurrence(corpus);
        let edges = ppmi(n_verses, &df, &co);
        drop(co); // free the dense count matrix
        let knn = mutual_knn(KNN, &edges);
        drop(edges);
        let communities = communities(COMMUNITY_ROUNDS, &knn);

        let mut collocates: HashMap<String, Vec<(String, f32)>> = HashMap::new();
        for ((a, b), w) in &knn {
            collocates.entry(a.clone()).or_default().push((b.clone(), *w));
            collocates.entry(b.clone()).or_default().push((a.clone(), *w));
        }
        for list in collocates.values_mut() {
            list.sort_by(|x, y| y.1.total_cmp(&x.1));
        }
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

    /// The `n` books this concept occurs in most.
    pub fn top_books(&self, code: &str, n: usize) -> Vec<(String, u32)> {
        self.ix.get(code).map(|s| top_books(s, n)).unwrap_or_default()
    }

    /// (OT, NT) occurrence split.
    pub fn testament_split(&self, code: &str) -> (u32, u32) {
        self.ix.get(code).map(testament_split).unwrap_or((0, 0))
    }

    /// The strongest collocates of a code (mutual-kNN, by PPMI).
    pub fn collocates(&self, code: &str, k: usize) -> Vec<(String, f32)> {
        self.collocates.get(code).map(|v| v.iter().take(k).cloned().collect()).unwrap_or_default()
    }

    /// The collocation community `code` belongs to (its co-occurring field),
    /// excluding the code itself; empty if it's in none.
    pub fn community(&self, code: &str) -> Vec<String> {
        match self.community_of.get(code) {
            Some(&i) => self.communities[i].iter().filter(|c| c.as_str() != code).cloned().collect(),
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
/// pre-baked) through `pure_engine_concept_map_json`.
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
        let corpus = pure_core::corpus::from_str(jsonl).unwrap();
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
