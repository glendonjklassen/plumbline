//! Weaves: parallel passages as a graph of verse-to-verse links.
//!
//! Ported from overlay `Weave.hs`. A weave is a set of undirected links between
//! single verses. The graph is the whole model: a drawn connector per edge is
//! its faithful rendering, combining two weaves is the union of their edges
//! (transitive join), and an unlinked verse still reads in place.
//!
//! Weaves are personal study data: plain unsigned JSON, one file per weave.
//!
//! **Not yet ported:** the `overlay-weave-v1` (grid) → v2 migration. New
//! Plumbline data is v2; a v1 file currently surfaces as a parse error rather
//! than being silently migrated. Port `migrateV1` when older data must load.

use crate::corpus::Corpus;
use crate::reference::VRef;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

/// A word span within a verse: an inclusive token-index range.
pub type Span = (u16, u16);

/// The identity of an edge: endpoints, label, and spans — approval excluded.
type LinkKey = (VRef, VRef, String, Option<Span>, Option<Span>);

/// An undirected edge between two verses, stored canonically (`a <= b` in
/// reading order) so equal links compare equal. `label` is the exact shared
/// text the edge points at; `span_a`/`span_b` optionally narrow each endpoint
/// to a word span. `approved` marks reviewer approval and is deliberately
/// **excluded from identity** (`Eq`/`Ord`), so toggling it never duplicates an
/// edge. Ported from `Link`.
#[derive(Debug, Clone)]
pub struct Link {
    pub a: VRef,
    pub b: VRef,
    pub label: String,
    pub approved: bool,
    pub span_a: Option<Span>,
    pub span_b: Option<Span>,
}

impl Link {
    /// Build a labelled link with optional per-endpoint spans, endpoints in
    /// reading order. The label rides the edge; each span rides its endpoint,
    /// so swapping endpoints swaps the spans in lockstep. This is the sole
    /// structural constructor — every span-aware caller routes through it so
    /// the endpoint/span pairing can never desync. Ported from `canonLinkSpan`.
    pub fn canon_span(
        a: VRef,
        b: VRef,
        label: impl Into<String>,
        span_a: Option<Span>,
        span_b: Option<Span>,
    ) -> Link {
        let label = label.into();
        if a.reading_key() <= b.reading_key() {
            Link { a, b, label, approved: false, span_a, span_b }
        } else {
            Link { a: b, b: a, label, approved: false, span_a: span_b, span_b: span_a }
        }
    }

    /// An unlabelled, whole-verse link with endpoints in reading order.
    pub fn canon(a: VRef, b: VRef) -> Link {
        Link::canon_span(a, b, "", None, None)
    }

    /// A labelled, whole-verse link with endpoints in reading order.
    pub fn canon_labelled(a: VRef, b: VRef, label: impl Into<String>) -> Link {
        Link::canon_span(a, b, label, None, None)
    }

    /// Edge identity: endpoints, label, spans (ignoring approval).
    pub fn key(&self) -> LinkKey {
        (
            self.a.clone(),
            self.b.clone(),
            self.label.clone(),
            self.span_a,
            self.span_b,
        )
    }
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        // Field-wise (not via `key()`): identity comparisons run in sort/dedup
        // loops and must not allocate five Strings per call.
        self.a == other.a
            && self.b == other.b
            && self.label == other.label
            && self.span_a == other.span_a
            && self.span_b == other.span_b
    }
}
impl Eq for Link {}
impl PartialOrd for Link {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Link {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.a
            .cmp(&other.a)
            .then_with(|| self.b.cmp(&other.b))
            .then_with(|| self.label.cmp(&other.label))
            .then_with(|| self.span_a.cmp(&other.span_a))
            .then_with(|| self.span_b.cmp(&other.span_b))
    }
}

// ── weave kind ───────────────────────────────────────────────────────────────

/// What sort of parallel a weave records. Stored as a frozen token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaveKind {
    Retelling,
    Typological,
    Prophecy,
    Quotation,
}

impl WeaveKind {
    pub const ALL: [WeaveKind; 4] = [
        WeaveKind::Retelling,
        WeaveKind::Typological,
        WeaveKind::Prophecy,
        WeaveKind::Quotation,
    ];

    /// Frozen on-disk token.
    pub fn token(self) -> &'static str {
        match self {
            WeaveKind::Retelling => "retelling",
            WeaveKind::Typological => "type",
            WeaveKind::Prophecy => "prophecy",
            WeaveKind::Quotation => "quotation",
        }
    }

    /// Human label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            WeaveKind::Retelling => "retelling",
            WeaveKind::Typological => "type",
            WeaveKind::Prophecy => "prophecy & fulfillment",
            WeaveKind::Quotation => "quotation",
        }
    }

    pub fn parse(t: &str) -> Option<WeaveKind> {
        match t {
            "retelling" => Some(WeaveKind::Retelling),
            "type" => Some(WeaveKind::Typological),
            "prophecy" => Some(WeaveKind::Prophecy),
            "quotation" => Some(WeaveKind::Quotation),
            _ => None,
        }
    }
}

/// Who wrote a weave's notes. Reverent prose reads the same whether a person or
/// a machine produced it, so the reader is told which. Ported from
/// `NotesSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesSource {
    Hand,
    Generated,
}

impl NotesSource {
    pub fn token(self) -> &'static str {
        match self {
            NotesSource::Hand => "hand",
            NotesSource::Generated => "generated",
        }
    }
    pub fn parse(t: &str) -> Option<NotesSource> {
        match t {
            "hand" => Some(NotesSource::Hand),
            "generated" => Some(NotesSource::Generated),
            _ => None,
        }
    }
}

// ── weave ──────────────────────────────────────────────────────────────────

/// A weave: named graph of verse↔verse links, plus metadata. Ported from
/// `Weave`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weave {
    pub name: String,
    pub kind: WeaveKind,
    pub tok_version: String,
    pub notes: String,
    pub notes_source: NotesSource,
    pub created: String,
    pub links: Vec<Link>,
    pub approved: bool,
}

impl Weave {
    /// A fresh, empty weave (no links yet). Ported from `emptyWeave`.
    pub fn empty(name: impl Into<String>, kind: WeaveKind, tok_version: impl Into<String>, created: impl Into<String>) -> Weave {
        Weave {
            name: name.into(),
            kind,
            tok_version: tok_version.into(),
            notes: String::new(),
            notes_source: NotesSource::Generated,
            created: created.into(),
            links: Vec::new(),
            approved: false,
        }
    }

    /// Add links, keeping the set deduplicated and sorted; later links win on
    /// identity collision (their approval replaces the earlier edge's), then
    /// the weave-level flag is recomputed. Ported from `addLinks`.
    pub fn add_links(&mut self, new: impl IntoIterator<Item = Link>) {
        let mut m: BTreeMap<LinkKey, Link> = BTreeMap::new();
        for l in self.links.drain(..).chain(new) {
            m.insert(l.key(), l);
        }
        self.links = m.into_values().collect();
        self.reapprove();
    }

    /// Remove an edge (matched by identity, regardless of approval state).
    pub fn remove_link(&mut self, target: &Link) {
        self.links.retain(|l| l != target);
        self.reapprove();
    }

    /// Recompute the weave-level approval flag: approved exactly when it has
    /// links and every one is approved. Ported from `reapprove`.
    pub fn reapprove(&mut self) {
        self.approved = !self.links.is_empty() && self.links.iter().all(|l| l.approved);
    }

    /// Set the approval of a single edge (matched by identity), then recompute
    /// the weave flag. Ported from `setLinkApproval`.
    pub fn set_link_approval(&mut self, target: &Link, val: bool) {
        for l in &mut self.links {
            if *l == *target {
                l.approved = val;
            }
        }
        self.reapprove();
    }

    /// Approve or unapprove every edge at once. Ported from `setAllApproval`.
    pub fn set_all_approval(&mut self, val: bool) {
        for l in &mut self.links {
            l.approved = val;
        }
        self.reapprove();
    }

    /// How many edges are approved.
    pub fn approved_count(&self) -> usize {
        self.links.iter().filter(|l| l.approved).count()
    }

    /// Union another weave's edges into this one (the transitive merge; shared
    /// verses join their components). This weave's metadata is kept. Ported
    /// from `combine`.
    pub fn combine(&mut self, other: &Weave) {
        self.add_links(other.links.iter().cloned());
    }

    /// The links with at least one endpoint among the given verses (for ambient
    /// rendering). Ported from `linksTouching`.
    pub fn links_touching<'a>(&'a self, verses: &'a HashSet<VRef>) -> impl Iterator<Item = &'a Link> {
        self.links
            .iter()
            .filter(move |l| verses.contains(&l.a) || verses.contains(&l.b))
    }
}

/// Connected components of a link graph, each a set of verses (deterministic:
/// components and their members come out in `VRef` order). Ported from
/// `components`.
pub fn components(links: &[Link]) -> Vec<Vec<VRef>> {
    let mut adj: HashMap<&VRef, Vec<&VRef>> = HashMap::new();
    let mut verts: BTreeSet<&VRef> = BTreeSet::new();
    for l in links {
        adj.entry(&l.a).or_default().push(&l.b);
        adj.entry(&l.b).or_default().push(&l.a);
        verts.insert(&l.a);
        verts.insert(&l.b);
    }

    let mut remaining: BTreeSet<&VRef> = verts;
    let mut out = Vec::new();
    while let Some(&start) = remaining.iter().next() {
        // BFS from the smallest remaining vertex.
        let mut seen: BTreeSet<&VRef> = BTreeSet::new();
        let mut queue = vec![start];
        while let Some(v) = queue.pop() {
            if seen.insert(v) {
                if let Some(ns) = adj.get(v) {
                    queue.extend(ns.iter().copied());
                }
            }
        }
        for v in &seen {
            remaining.remove(*v);
        }
        out.push(seen.into_iter().cloned().collect());
    }
    out
}

/// All verses linked (transitively) to a verse, the verse included. Ported
/// from `componentOf`.
pub fn component_of(links: &[Link], v: &VRef) -> Vec<VRef> {
    components(links)
        .into_iter()
        .find(|c| c.contains(v))
        .unwrap_or_else(|| vec![v.clone()])
}

/// Build links from a per-pane selection. Two equal-length panes zip 1:1;
/// anything else connects every selected verse to every selected verse in
/// another pane (the many-to-many / convergent case). Ported from `smartLinks`.
pub fn smart_links(panes: &[Vec<VRef>]) -> Vec<Link> {
    let non_empty: Vec<&Vec<VRef>> = panes.iter().filter(|p| !p.is_empty()).collect();
    if let [a, b] = non_empty.as_slice() {
        if a.len() == b.len() {
            return a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| Link::canon(x.clone(), y.clone()))
                .collect();
        }
    }
    let mut out = Vec::new();
    for i in 0..non_empty.len() {
        for j in (i + 1)..non_empty.len() {
            for x in non_empty[i] {
                for y in non_empty[j] {
                    out.push(Link::canon(x.clone(), y.clone()));
                }
            }
        }
    }
    out
}

// ── JSON codec ───────────────────────────────────────────────────────────────

impl Serialize for Link {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("a", &self.a.ref_key())?;
        m.serialize_entry("b", &self.b.ref_key())?;
        if !self.label.is_empty() {
            m.serialize_entry("label", &self.label)?;
        }
        if self.approved {
            m.serialize_entry("approved", &true)?;
        }
        if let Some(s) = self.span_a {
            m.serialize_entry("spanA", &s)?;
        }
        if let Some(s) = self.span_b {
            m.serialize_entry("spanB", &s)?;
        }
        m.end()
    }
}

#[derive(Deserialize)]
struct LinkWire {
    a: String,
    b: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    approved: bool,
    #[serde(rename = "spanA", default)]
    span_a: Option<Span>,
    #[serde(rename = "spanB", default)]
    span_b: Option<Span>,
}

impl<'de> Deserialize<'de> for Link {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let w = LinkWire::deserialize(d)?;
        let a = VRef::parse_ref_key(&w.a)
            .ok_or_else(|| D::Error::custom(format!("bad link ref: {}", w.a)))?;
        let b = VRef::parse_ref_key(&w.b)
            .ok_or_else(|| D::Error::custom(format!("bad link ref: {}", w.b)))?;
        // Route through the canonical constructor so the endpoint/span swap
        // stays paired even for hand-edited, out-of-order on-disk links.
        let mut link = Link::canon_span(a, b, w.label, w.span_a, w.span_b);
        link.approved = w.approved;
        Ok(link)
    }
}

impl Serialize for Weave {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("format", "overlay-weave-v2")?;
        m.serialize_entry("name", &self.name)?;
        m.serialize_entry("kind", self.kind.token())?;
        m.serialize_entry("tokenization", &self.tok_version)?;
        m.serialize_entry("notes", &self.notes)?;
        if !self.notes.is_empty() {
            m.serialize_entry("notesSource", self.notes_source.token())?;
        }
        m.serialize_entry("created", &self.created)?;
        m.serialize_entry("approved", &self.approved)?;
        m.serialize_entry("links", &self.links)?;
        m.end()
    }
}

#[derive(Deserialize)]
struct WeaveWire {
    format: String,
    name: String,
    #[serde(default = "default_kind_token")]
    kind: String,
    tokenization: String,
    #[serde(default)]
    notes: String,
    #[serde(rename = "notesSource", default = "default_notes_source_token")]
    notes_source: String,
    created: String,
    #[serde(default)]
    links: Vec<Link>,
    #[serde(default)]
    approved: bool,
}

fn default_kind_token() -> String {
    "retelling".into()
}
fn default_notes_source_token() -> String {
    "generated".into()
}

impl<'de> Deserialize<'de> for Weave {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;
        let w = WeaveWire::deserialize(d)?;
        if w.format != "overlay-weave-v2" {
            return Err(D::Error::custom(format!("not overlay-weave-v2: {}", w.format)));
        }
        let kind = WeaveKind::parse(&w.kind)
            .ok_or_else(|| D::Error::custom(format!("unknown weave kind: {}", w.kind)))?;
        let notes_source = NotesSource::parse(&w.notes_source)
            .ok_or_else(|| D::Error::custom(format!("unknown notesSource: {}", w.notes_source)))?;
        Ok(Weave {
            name: w.name,
            kind,
            tok_version: w.tokenization,
            notes: w.notes,
            notes_source,
            created: w.created,
            links: w.links,
            approved: w.approved,
        })
    }
}

/// A weave together with the file it loaded from. Ported from `LoadedWeave`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedWeave {
    pub file: std::path::PathBuf,
    pub weave: Weave,
}

/// All weave links across the library as canonical, deduped verse pairs. Each
/// stored link is already ordered `a <= b` in reading order (see
/// [`Link::canon_span`]), so the pair is taken as-is and a `HashSet` drops the
/// duplicates a verse pair linked by several weaves would produce.
///
/// This is the one derivation behind the ambient connector lines and the chord
/// map, shared by every shell: GTK calls it directly; the non-Rust shells
/// receive the same pairs (with each endpoint resolved and located) through
/// `plumbline_engine_link_pairs_json`. Resolvability against the corpus is a
/// separate, drawing-time concern and is not filtered here.
pub fn link_pairs(weaves: &[LoadedWeave]) -> Vec<(VRef, VRef)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for lw in weaves {
        for l in &lw.weave.links {
            if seen.insert((l.a.clone(), l.b.clone())) {
                out.push((l.a.clone(), l.b.clone()));
            }
        }
    }
    out
}

/// Book-to-book weave density: how strongly each canon-ordered book pair is
/// woven together, over the deduped [`link_pairs`]. Each entry is
/// `(book_a_index, book_b_index, count)` with `a <= b` (canon order, so the pair
/// is orientation-free), plus the maximum count for normalising ribbon
/// weight/alpha. A pair whose endpoint book isn't in the canon (unknown id) is
/// skipped, matching the drawing code that has nowhere to place it.
///
/// The one derivation behind the chord/arc "Weave map": GTK calls it directly;
/// the non-Rust shells receive the same folded counts through
/// `plumbline_engine_chord_map_json`, so no shell re-folds the pairs or re-derives
/// the max.
pub fn chord_pairs(weaves: &[LoadedWeave]) -> (Vec<(usize, usize, u32)>, u32) {
    let mut counts: HashMap<(usize, usize), u32> = HashMap::new();
    for (a, b) in link_pairs(weaves) {
        let (Some(ia), Some(ib)) = (crate::canon::book_order(&a.book), crate::canon::book_order(&b.book))
        else {
            continue;
        };
        let key = if ia <= ib { (ia, ib) } else { (ib, ia) };
        *counts.entry(key).or_insert(0) += 1;
    }
    let max = counts.values().copied().max().unwrap_or(1);
    let mut pairs: Vec<(usize, usize, u32)> =
        counts.into_iter().map(|((a, b), c)| (a, b, c)).collect();
    // Deterministic order (HashMap iteration is not): by book pair. The shells
    // re-sort by weight for painting, so this only fixes the wire/test output.
    pairs.sort_unstable();
    (pairs, max)
}

// ── constellation (the weave-library overview) ────────────────────────────────
//
// Ported from overlay `Constellation.hs`: a scoped overview of the weave
// library, one weave per labelled lane (largest first), nodes on the canon book
// backbone, links as gentle curves. Pinned lanes stay put while paging cycles
// the free lanes past them. This is the one derivation behind the popup (review
// item 3): GTK calls it directly; the non-Rust shells get the same laid-out
// page (as camelCase JSON) via `plumbline_engine_constellation_json`.
//
// Everything here is **fractions / logical units** — the shell maps them to
// pixels, picks colours, and paints. `x` is a canon fraction 0..1 across the
// book backbone; `lane_frac` is a fraction 0..1 within a lane's band (jitter
// already applied, always strictly inside so nothing clips); `size` is the
// node's witness degree normalised 0..1.

/// How many lanes the constellation shows at once. Both the paging arithmetic
/// (here) and each shell's lane-height mapping key on it — served in the wire
/// model so a shell never hardcodes a divisor that could drift from the paging.
pub const CONSTELLATION_LANES: usize = 18;

/// A laid-out page of the constellation: the lanes shown (pinned first, then
/// this page's slice of the free lanes) plus the paging arithmetic already
/// resolved into a caption.
pub struct Constellation {
    pub lanes: Vec<ConstellationLane>,
    /// How many lanes are pinned, the total free (unpinned) weaves, the page
    /// actually shown (the requested page clamped into range), and the last
    /// page index — the paging state a shell echoes into its controls.
    pub n_pins: usize,
    pub free_total: usize,
    pub page: usize,
    pub max_page: usize,
    /// The fully-composed paging caption ("N pinned · weaves lo–hi of total · …").
    pub caption: String,
    /// The fixed lane capacity ([`CONSTELLATION_LANES`]) — the shell's
    /// lane-height denominator, carried so it can't drift from the paging.
    pub lane_capacity: usize,
}

/// One lane of the current page: a weave drawn as its resolvable links.
pub struct ConstellationLane {
    /// The weave's index in the loaded library — the handle the compare card
    /// takes and the pin set keys on. Stable until the next authoring write.
    pub weave_index: usize,
    pub name: String,
    pub pinned: bool,
    /// Deduped nodes on this lane (first-appearance order).
    pub nodes: Vec<ConstellationNode>,
    /// One curved edge per resolvable link; endpoint coords carried so the shell
    /// draws and hit-tests the curve without re-placing the nodes.
    pub edges: Vec<ConstellationEdge>,
}

/// A node on a lane: a verse on the canon backbone, sized by witness degree.
pub struct ConstellationNode {
    /// Canon fraction 0..1 across the book backbone (shell maps to plot x).
    pub x: f32,
    /// Fraction 0..1 within the lane's band (jitter applied), strictly inside.
    pub lane_frac: f32,
    /// Witness degree ÷ the library's max degree (0..1); the shell picks the
    /// radius (both shells: `1.4 + 2.4 * size`).
    pub size: f32,
    /// The verse — located + displayed, so a click navigates and the tooltip
    /// names it without the shell parsing a ref key.
    pub ref_key: String,
    pub book: String,
    pub chapter: u16,
    pub verse: u16,
    pub display: String,
}

/// A link on a lane: both endpoints' `(x, lane_frac)` (same lane), for the curve.
pub struct ConstellationEdge {
    pub a_x: f32,
    pub a_lane_frac: f32,
    pub b_x: f32,
    pub b_lane_frac: f32,
}

/// A verse's canon fraction 0..1: book position plus chapter progress within
/// the book, over the 66 — the same backbone the canon strip uses.
fn constellation_x(corpus: &Corpus, r: &VRef) -> f32 {
    let bi = crate::canon::book_order(&r.book).unwrap_or(0) as f32;
    let nc = corpus.chapter_count(&r.book).max(1) as f32;
    (bi + (r.chapter.saturating_sub(1)) as f32 / nc) / crate::canon::BOOKS.len() as f32
}

/// A node's fractional position within its lane band: centre plus a small
/// deterministic jitter from the verse identity, so co-lane nodes don't fuse
/// into one flat line. In `(0.14, 0.86)` for every verse, so it never clips
/// regardless of lane height.
fn constellation_lane_frac(r: &VRef) -> f32 {
    let j = ((r.chapter as i64 * 3 + r.verse as i64) % 7 - 3) as f32;
    0.5 + j * 0.12
}

/// The paging caption: pins, the honest free-weave range, and the pin hint.
fn constellation_caption(n_pins: usize, free_total: usize, page: usize) -> String {
    let free_lanes = CONSTELLATION_LANES.saturating_sub(n_pins);
    let pins = if n_pins > 0 { format!("{n_pins} pinned · ") } else { String::new() };
    let body = if free_lanes == 0 {
        "all lanes pinned — unpin one to page".to_string()
    } else if free_total == 0 {
        "no free weaves".to_string()
    } else {
        format!(
            "weaves {}–{} of {} · largest first · click the ▪ to pin a lane",
            page * free_lanes + 1,
            free_total.min((page + 1) * free_lanes),
            free_total
        )
    };
    format!("{pins}{body}")
}

/// Lay out one page of the constellation for the given `page` and `pins` (weave
/// indices into `weaves`, the same handles the lanes carry). Pinned lanes come
/// first and stay put; the free lanes page past them. Recomputed per event —
/// the weave library is small.
pub fn constellation(
    weaves: &[LoadedWeave],
    corpus: &Corpus,
    page: usize,
    pins: &[usize],
) -> Constellation {
    // Witness degree over the whole library: how many weave links touch each
    // verse (both endpoints, every weave). The busiest verse sets the scale, so
    // node size is stable across pages.
    let mut deg: HashMap<VRef, usize> = HashMap::new();
    for lw in weaves {
        for l in &lw.weave.links {
            *deg.entry(l.a.clone()).or_default() += 1;
            *deg.entry(l.b.clone()).or_default() += 1;
        }
    }
    let max_deg = deg.values().copied().max().unwrap_or(1) as f32;

    // Usable weaves: those with at least one link whose both ends resolve in the
    // corpus (drawable). Keep the original library index for pins + compare.
    let mut usable: Vec<(usize, Vec<(VRef, VRef)>)> = weaves
        .iter()
        .enumerate()
        .map(|(i, lw)| {
            let links: Vec<(VRef, VRef)> = lw
                .weave
                .links
                .iter()
                .filter(|l| corpus.verse(&l.a).is_some() && corpus.verse(&l.b).is_some())
                .map(|l| (l.a.clone(), l.b.clone()))
                .collect();
            (i, links)
        })
        .filter(|(_, ls)| !ls.is_empty())
        .collect();
    // Largest first; a stable sort keeps ties in library (name) order.
    usable.sort_by_key(|(_, ls)| std::cmp::Reverse(ls.len()));

    let pinned_flag: Vec<bool> = usable.iter().map(|(i, _)| pins.contains(i)).collect();
    let n_pins = pinned_flag.iter().filter(|p| **p).count();
    let free_total = usable.len() - n_pins;
    let free_lanes = CONSTELLATION_LANES.saturating_sub(n_pins);
    let max_page =
        if free_lanes == 0 || free_total == 0 { 0 } else { (free_total - 1) / free_lanes };
    let page = page.min(max_page);

    let make_lane = |i: usize, links: &[(VRef, VRef)], pinned: bool| -> ConstellationLane {
        let mut seen: HashSet<VRef> = HashSet::new();
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for (a, b) in links {
            let (ax, alf) = (constellation_x(corpus, a), constellation_lane_frac(a));
            let (bx, blf) = (constellation_x(corpus, b), constellation_lane_frac(b));
            edges.push(ConstellationEdge { a_x: ax, a_lane_frac: alf, b_x: bx, b_lane_frac: blf });
            for r in [a, b] {
                if seen.insert(r.clone()) {
                    nodes.push(ConstellationNode {
                        x: constellation_x(corpus, r),
                        lane_frac: constellation_lane_frac(r),
                        size: deg.get(r).copied().unwrap_or(0) as f32 / max_deg,
                        ref_key: r.ref_key(),
                        book: r.book.clone(),
                        chapter: r.chapter,
                        verse: r.verse,
                        display: r.display(),
                    });
                }
            }
        }
        ConstellationLane { weave_index: i, name: weaves[i].weave.name.clone(), pinned, nodes, edges }
    };

    // Pinned lanes first (in usable order), then this page's slice of the free.
    let mut lanes = Vec::new();
    for ((i, links), pinned) in usable.iter().zip(&pinned_flag) {
        if *pinned {
            lanes.push(make_lane(*i, links, true));
        }
    }
    if free_lanes > 0 {
        for ((i, links), _) in usable
            .iter()
            .zip(&pinned_flag)
            .filter(|(_, p)| !**p)
            .skip(page * free_lanes)
            .take(free_lanes)
        {
            lanes.push(make_lane(*i, links, false));
        }
    }

    Constellation {
        lanes,
        n_pins,
        free_total,
        page,
        max_page,
        caption: constellation_caption(n_pins, free_total, page),
        lane_capacity: CONSTELLATION_LANES,
    }
}

/// Slug a weave name into a JSON filename under `dir`. Ported from
/// `weaveFileIn`.
pub fn weave_file_in(dir: impl AsRef<Path>, name: &str) -> std::path::PathBuf {
    dir.as_ref().join(format!("{}.json", crate::store::slug(name, "weave")))
}

/// Load every `*.json` weave under the two standard directories `home/weaves`
/// and `home/weaves/suggested`. Returns the loaded weaves (sorted by
/// lowercased name) and any per-file errors. Ported from `loadWeaves` (v1
/// migration excluded — see the module note).
pub fn load_weaves(home: impl AsRef<Path>) -> (Vec<LoadedWeave>, Vec<String>) {
    let home = home.as_ref();
    let dirs = [home.join("weaves"), home.join("weaves").join("suggested")];

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in &dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut here: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect();
            here.sort();
            files.extend(here);
        }
    }

    let mut loaded = Vec::new();
    let mut errors = Vec::new();
    for path in files {
        match std::fs::read(&path) {
            Err(e) => errors.push(format!("{}: {e}", path.display())),
            Ok(bytes) => match serde_json::from_slice::<Weave>(&bytes) {
                Ok(weave) => loaded.push(LoadedWeave { file: path, weave }),
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            },
        }
    }
    loaded.sort_by(|x, y| x.weave.name.to_lowercase().cmp(&y.weave.name.to_lowercase()));
    (loaded, errors)
}

/// Write a weave to a file as pretty-safe JSON (matching overlay's trailing
/// newline). Ported from `writeWeave` (atomic write handled by the caller /
/// `crate::store` later).
pub fn to_json(weave: &Weave) -> Result<String, Error> {
    serde_json::to_string(weave).map(|s| s + "\n").map_err(|e| Error::Parse(e.to_string()))
}

/// Atomically write a weave to `path`.
pub fn write_weave(path: impl AsRef<Path>, weave: &Weave) -> Result<(), Error> {
    crate::store::write_atomic(path, &to_json(weave)?)
}

/// Add `link` to the weave named `name` (case-insensitive match among
/// `loaded`), creating its file on first use with `kind`. Links are deduped and
/// canonicalized by [`Weave::add_links`]. Returns the file written; a file that
/// exists but is absent from `loaded` (failed to parse) is refused rather than
/// clobbered. The caller supplies the creation timestamp.
pub fn add_link(
    home: impl AsRef<Path>,
    loaded: &[LoadedWeave],
    name: &str,
    kind: WeaveKind,
    tok_version: &str,
    created: &str,
    link: Link,
) -> Result<std::path::PathBuf, Error> {
    let wanted = name.trim().to_lowercase();
    // Match canonical weaves only: appending a user's link to a *suggestion*
    // would silently delete it if the suggestion is later rejected.
    if let Some(lw) = loaded
        .iter()
        .find(|lw| !is_suggested(lw) && lw.weave.name.to_lowercase() == wanted)
    {
        let mut weave = lw.weave.clone();
        weave.add_links([link]);
        write_weave(&lw.file, &weave)?;
        Ok(lw.file.clone())
    } else {
        let path = weave_file_in(home.as_ref().join("weaves"), name);
        if path.exists() {
            return Err(Error::Corpus(format!(
                "{} exists but could not be read — refusing to overwrite",
                path.display()
            )));
        }
        let mut weave = Weave::empty(name.trim(), kind, tok_version, created);
        weave.add_links([link]);
        write_weave(&path, &weave)?;
        Ok(path)
    }
}

/// Weave a set of passages together as a **canon-ordered chain** — the
/// tag→weave conversion: a reader accumulates a topic tag over time (Rapture,
/// New Birth, …) and later turns it (or a chosen subset) into a weave they can
/// read as one thread through the canon. Refs are reading-order sorted and
/// deduped; consecutive pairs become links. Find-or-creates `name` like
/// [`add_link`], so re-running after the tag grows just adds the new edges.
/// Fewer than two distinct refs is an error — a weave is made of links.
pub fn add_chain(
    home: impl AsRef<Path>,
    loaded: &[LoadedWeave],
    name: &str,
    kind: WeaveKind,
    tok_version: &str,
    created: &str,
    refs: &[VRef],
) -> Result<std::path::PathBuf, Error> {
    let mut ordered: Vec<VRef> = refs.to_vec();
    ordered.sort_by_key(|r| r.reading_key());
    ordered.dedup();
    if ordered.len() < 2 {
        return Err(Error::Corpus(
            "a weave needs at least two distinct passages".into(),
        ));
    }
    let links: Vec<Link> = ordered
        .windows(2)
        .map(|w| Link::canon(w[0].clone(), w[1].clone()))
        .collect();

    let wanted = name.trim().to_lowercase();
    if let Some(lw) = loaded
        .iter()
        .find(|lw| !is_suggested(lw) && lw.weave.name.to_lowercase() == wanted)
    {
        let mut weave = lw.weave.clone();
        weave.add_links(links);
        write_weave(&lw.file, &weave)?;
        Ok(lw.file.clone())
    } else {
        let path = weave_file_in(home.as_ref().join("weaves"), name);
        if path.exists() {
            return Err(Error::Corpus(format!(
                "{} exists but could not be read — refusing to overwrite",
                path.display()
            )));
        }
        let mut weave = Weave::empty(name.trim(), kind, tok_version, created);
        weave.add_links(links);
        write_weave(&path, &weave)?;
        Ok(path)
    }
}

/// Is this loaded weave a *suggestion* — i.e. it lives under
/// `home/weaves/suggested` rather than `home/weaves`? Suggestions are proposed
/// (often machine-generated) weaves awaiting the reader's review. Checked by
/// the immediate parent directory name, so it is OS-path-separator agnostic.
pub fn is_suggested(lw: &LoadedWeave) -> bool {
    lw.file
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "suggested")
}

/// **Approve** a weave: mark every link approved and land it in the canonical
/// `home/weaves` directory. A suggestion is *promoted* — written into
/// `home/weaves` and its `suggested` file removed; if a weave of the same name
/// already lives there its edges are merged in (union) rather than clobbered.
/// An already-canonical weave is simply rewritten in place with all links
/// approved. Returns the canonical file written. Cross-platform: the write goes
/// through [`crate::store`]'s atomic write and the old file (if any) is removed
/// with `std::fs::remove_file`.
pub fn approve_weave(home: impl AsRef<Path>, lw: &LoadedWeave) -> Result<std::path::PathBuf, Error> {
    let dest = weave_file_in(home.as_ref().join("weaves"), &lw.weave.name);

    // Start from any existing canonical weave of this name so approval merges
    // into it instead of overwriting; otherwise from the weave being approved.
    let mut weave = if dest != lw.file && dest.exists() {
        match std::fs::read(&dest) {
            Ok(bytes) => match serde_json::from_slice::<Weave>(&bytes) {
                Ok(mut existing) => {
                    existing.combine(&lw.weave);
                    existing
                }
                Err(e) => return Err(Error::Parse(format!("{}: {e}", dest.display()))),
            },
            Err(e) => return Err(Error::Corpus(format!("{}: {e}", dest.display()))),
        }
    } else {
        lw.weave.clone()
    };
    weave.set_all_approval(true);

    write_weave(&dest, &weave)?;
    if lw.file != dest && lw.file.exists() {
        std::fs::remove_file(&lw.file).map_err(|e| Error::Corpus(format!("{}: {e}", lw.file.display())))?;
    }
    Ok(dest)
}

/// **Reject** a weave: delete its file. Intended for suggestions (removing a
/// proposal the reader declined); it will delete a canonical weave too, so the
/// caller decides what may be rejected. A missing file is treated as success.
pub fn reject_weave(lw: &LoadedWeave) -> Result<(), Error> {
    match std::fs::remove_file(&lw.file) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Corpus(format!("{}: {e}", lw.file.display()))),
    }
}

/// Replace the notes document of the weave named `name` (case-insensitive among
/// `loaded`), marking `notesSource` as hand-written since a person edited it.
/// The weave must already exist.
pub fn set_weave_notes(loaded: &[LoadedWeave], name: &str, notes: &str) -> Result<std::path::PathBuf, Error> {
    let wanted = name.trim().to_lowercase();
    let lw = loaded
        .iter()
        .find(|lw| lw.weave.name.to_lowercase() == wanted)
        .ok_or_else(|| Error::Corpus(format!("no weave named {name}")))?;
    let mut weave = lw.weave.clone();
    weave.notes = notes.to_string();
    weave.notes_source = NotesSource::Hand;
    write_weave(&lw.file, &weave)?;
    Ok(lw.file.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(book: &str, c: u16, v: u16) -> VRef {
        VRef::new(book, c, v)
    }

    #[test]
    fn sets_weave_notes_as_hand_written() {
        let home = std::env::temp_dir().join(format!("plumbline-weave-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let (loaded, _) = load_weaves(&home);
        add_link(&home, &loaded, "Lamb", WeaveKind::Typological, "kjv1769-tok2", "c", Link::canon(r("Gen", 22, 8), r("John", 1, 29))).unwrap();

        let (loaded, _) = load_weaves(&home);
        set_weave_notes(&loaded, "lamb", "God will provide himself a lamb").unwrap();

        let (loaded, _) = load_weaves(&home);
        assert_eq!(loaded[0].weave.notes, "God will provide himself a lamb");
        assert_eq!(loaded[0].weave.notes_source, NotesSource::Hand);
        assert!(set_weave_notes(&loaded, "nope", "x").is_err());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn approve_promotes_suggestion_and_reject_deletes() {
        let home = std::env::temp_dir().join(format!("plumbline-weave-approve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);

        // Seed a suggestion under weaves/suggested.
        let sug_dir = home.join("weaves").join("suggested");
        let mut w = Weave::empty("Ransom", WeaveKind::Prophecy, "kjv1769-tok2", "2026-01-01T00:00:00Z");
        w.add_links([Link::canon(r("Isa", 53, 5), r("1Pet", 2, 24))]);
        assert!(!w.approved);
        write_weave(weave_file_in(&sug_dir, "Ransom"), &w).unwrap();

        let (loaded, errs) = load_weaves(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        assert!(is_suggested(&loaded[0]));

        // Approve → promoted into weaves/, all links approved, suggestion gone.
        let dest = approve_weave(&home, &loaded[0]).unwrap();
        assert!(!loaded[0].file.exists(), "suggestion should be removed");
        assert!(dest.exists());
        let (loaded, _) = load_weaves(&home);
        assert_eq!(loaded.len(), 1);
        assert!(!is_suggested(&loaded[0]));
        assert!(loaded[0].weave.approved);
        assert_eq!(loaded[0].weave.approved_count(), 1);

        // Reject the now-canonical weave → its file is deleted.
        reject_weave(&loaded[0]).unwrap();
        assert!(!loaded[0].file.exists());
        let (loaded, _) = load_weaves(&home);
        assert!(loaded.is_empty());
        // Rejecting again (missing file) is a no-op success.
        reject_weave(&LoadedWeave { file: dest, weave: w }).unwrap();

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn approve_merges_into_existing_canonical_weave() {
        let home = std::env::temp_dir().join(format!("plumbline-weave-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);

        // Canonical weave with one (approved) link.
        let mut canon = Weave::empty("Lamb", WeaveKind::Typological, "kjv1769-tok2", "c");
        canon.add_links([Link::canon(r("Gen", 22, 8), r("John", 1, 29))]);
        canon.set_all_approval(true);
        write_weave(weave_file_in(home.join("weaves"), "Lamb"), &canon).unwrap();

        // Same-named suggestion with a different link.
        let mut sug = Weave::empty("Lamb", WeaveKind::Typological, "kjv1769-tok2", "s");
        sug.add_links([Link::canon(r("Exod", 12, 3), r("Rev", 5, 6))]);
        write_weave(weave_file_in(home.join("weaves").join("suggested"), "Lamb"), &sug).unwrap();

        let (loaded, _) = load_weaves(&home);
        let suggestion = loaded.iter().find(|lw| is_suggested(lw)).unwrap();
        approve_weave(&home, suggestion).unwrap();

        let (loaded, _) = load_weaves(&home);
        assert_eq!(loaded.len(), 1, "the two same-named weaves merged into one canonical file");
        assert_eq!(loaded[0].weave.links.len(), 2);
        assert!(loaded[0].weave.approved);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn add_link_creates_appends_and_dedupes() {
        let home = std::env::temp_dir().join(format!("plumbline-weave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);

        let (loaded, _) = load_weaves(&home);
        add_link(&home, &loaded, "My Links", WeaveKind::Quotation, "kjv1769-tok2", "2026-01-01T00:00:00Z", Link::canon(r("Gen", 15, 6), r("Rom", 4, 3))).unwrap();

        let (loaded, _) = load_weaves(&home);
        assert_eq!(loaded.len(), 1);
        add_link(&home, &loaded, "my links", WeaveKind::Quotation, "kjv1769-tok2", "x", Link::canon(r("Gen", 15, 6), r("Gal", 3, 6))).unwrap();

        // A duplicate of the first link must not be added twice.
        let (loaded, _) = load_weaves(&home);
        add_link(&home, &loaded, "My Links", WeaveKind::Quotation, "kjv1769-tok2", "x", Link::canon(r("Gen", 15, 6), r("Rom", 4, 3))).unwrap();

        let (loaded, errs) = load_weaves(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].weave.links.len(), 2);

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn add_chain_weaves_refs_in_reading_order() {
        let home = std::env::temp_dir().join(format!("plumbline-chain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);

        // Unordered, with a duplicate — the chain must come out canon-ordered
        // and deduped: Gen 15:6 → Rom 4:3 → Gal 3:6 as two links.
        let refs = [r("Rom", 4, 3), r("Gen", 15, 6), r("Gal", 3, 6), r("Gen", 15, 6)];
        let (loaded, _) = load_weaves(&home);
        add_chain(&home, &loaded, "Faith Counted", WeaveKind::Typological, "kjv1769-tok2", "2026-07-25T00:00:00Z", &refs)
            .unwrap();

        let (loaded, errs) = load_weaves(&home);
        assert!(errs.is_empty());
        assert_eq!(loaded.len(), 1);
        let links = &loaded[0].weave.links;
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].a, r("Gen", 15, 6));
        assert_eq!(links[0].b, r("Rom", 4, 3));
        assert_eq!(links[1].a, r("Rom", 4, 3));
        assert_eq!(links[1].b, r("Gal", 3, 6));

        // Re-running after the tag grew adds only the new edge (find-or-create
        // + link dedup), so accumulate-then-weave is idempotent.
        let refs2 = [r("Gen", 15, 6), r("Rom", 4, 3), r("Gal", 3, 6), r("Jas", 2, 23)];
        add_chain(&home, &loaded, "faith counted", WeaveKind::Typological, "kjv1769-tok2", "x", &refs2).unwrap();
        let (loaded, _) = load_weaves(&home);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].weave.links.len(), 3);

        // One ref is not a weave.
        let (loaded, _) = load_weaves(&home);
        assert!(add_chain(&home, &loaded, "too small", WeaveKind::Typological, "kjv1769-tok2", "x", &[r("Gen", 1, 1)]).is_err());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn canonicalizes_endpoints_in_reading_order() {
        // John after Gen in reading order, so endpoints swap.
        let l = Link::canon(r("John", 3, 16), r("Gen", 1, 1));
        assert_eq!(l.a, r("Gen", 1, 1));
        assert_eq!(l.b, r("John", 3, 16));
    }

    #[test]
    fn span_swaps_with_endpoints() {
        let l = Link::canon_span(r("John", 3, 16), r("Gen", 1, 1), "", Some((1, 2)), Some((3, 4)));
        assert_eq!(l.a, r("Gen", 1, 1));
        assert_eq!(l.span_a, Some((3, 4))); // the span that rode Gen
        assert_eq!(l.span_b, Some((1, 2)));
    }

    #[test]
    fn identity_ignores_approval() {
        let mut x = Link::canon(r("Gen", 1, 1), r("John", 1, 1));
        let y = Link::canon(r("Gen", 1, 1), r("John", 1, 1));
        x.approved = true;
        assert_eq!(x, y);
    }

    #[test]
    fn add_links_dedups_and_reapproves() {
        let mut w = Weave::empty("t", WeaveKind::Retelling, "kjv1769-tok2", "now");
        w.add_links([Link::canon(r("Gen", 1, 1), r("John", 1, 1))]);
        w.add_links([Link::canon(r("Gen", 1, 1), r("John", 1, 1))]); // duplicate
        assert_eq!(w.links.len(), 1);
        assert!(!w.approved);
        w.set_all_approval(true);
        assert!(w.approved);
        // extending a fully-approved weave drops it back to unapproved
        w.add_links([Link::canon(r("Gen", 1, 2), r("Matt", 1, 1))]);
        assert!(!w.approved);
        assert_eq!(w.approved_count(), 1);
    }

    #[test]
    fn link_pairs_dedupe_canonically_across_weaves() {
        // Two weaves that share one link (Gen 1:1–John 1:1, endpoints given in
        // opposite order) plus one unique link each.
        let mut w1 = Weave::empty("a", WeaveKind::Retelling, "kjv1769-tok2", "now");
        w1.add_links([
            Link::canon(r("Gen", 1, 1), r("John", 1, 1)),
            Link::canon(r("Gen", 2, 4), r("Matt", 19, 4)),
        ]);
        let mut w2 = Weave::empty("b", WeaveKind::Quotation, "kjv1769-tok2", "now");
        w2.add_links([
            Link::canon(r("John", 1, 1), r("Gen", 1, 1)), // same pair, reversed
            Link::canon(r("Exod", 12, 3), r("Rev", 5, 6)),
        ]);
        let loaded = vec![
            LoadedWeave { file: "a.json".into(), weave: w1 },
            LoadedWeave { file: "b.json".into(), weave: w2 },
        ];

        let pairs = link_pairs(&loaded);
        // The shared pair appears once → three pairs total.
        assert_eq!(pairs.len(), 3);
        assert!(pairs.contains(&(r("Gen", 1, 1), r("John", 1, 1))));
        assert!(pairs.contains(&(r("Gen", 2, 4), r("Matt", 19, 4))));
        assert!(pairs.contains(&(r("Exod", 12, 3), r("Rev", 5, 6))));
        // Every pair is stored canonically (a <= b in reading order).
        for (a, b) in &pairs {
            assert!(a.reading_key() <= b.reading_key());
        }
    }

    #[test]
    fn chord_pairs_fold_book_density_over_deduped_links() {
        // Two Gen↔John verse links (distinct verses) fold to one book pair with
        // count 2; a Gen↔Gen link is a self-pair; a shared link counts once.
        let mut w1 = Weave::empty("a", WeaveKind::Retelling, "kjv1769-tok2", "now");
        w1.add_links([
            Link::canon(r("Gen", 1, 1), r("John", 1, 1)),
            Link::canon(r("Gen", 2, 4), r("John", 3, 16)),
            Link::canon(r("Gen", 1, 1), r("Gen", 5, 1)), // OT self-pair
        ]);
        let mut w2 = Weave::empty("b", WeaveKind::Quotation, "kjv1769-tok2", "now");
        w2.add_links([Link::canon(r("John", 1, 1), r("Gen", 1, 1))]); // dup of w1's first
        let loaded = vec![
            LoadedWeave { file: "a.json".into(), weave: w1 },
            LoadedWeave { file: "b.json".into(), weave: w2 },
        ];

        let (pairs, max) = chord_pairs(&loaded);
        let gen = crate::canon::book_order("Gen").unwrap();
        let john = crate::canon::book_order("John").unwrap();
        // Gen↔John woven by two distinct (deduped) verse links.
        assert_eq!(pairs.iter().find(|(a, b, _)| *a == gen && *b == john).unwrap().2, 2);
        // Gen↔Gen self-pair, count 1.
        assert_eq!(pairs.iter().find(|(a, b, _)| *a == gen && *b == gen).unwrap().2, 1);
        assert_eq!(max, 2);
        // Deterministic (sorted) output.
        let mut sorted = pairs.clone();
        sorted.sort_unstable();
        assert_eq!(pairs, sorted);
    }

    #[test]
    fn constellation_lays_out_lanes_paging_and_pins() {
        let jsonl = concat!(
            r#"{"tokenization":"kjv1769-tok2","verses":3}"#,
            "\n",
            r#"{"b":"Gen","c":1,"v":1,"t":[["","In","",[],0]]}"#,
            "\n",
            r#"{"b":"Gen","c":1,"v":2,"t":[["","And","",[],0]]}"#,
            "\n",
            r#"{"b":"John","c":3,"v":16,"t":[["","For","",[],0]]}"#,
        );
        let corpus = crate::corpus::from_str(jsonl).unwrap();

        let mut w1 = Weave::empty("alpha", WeaveKind::Retelling, "kjv1769-tok2", "now");
        w1.add_links([
            Link::canon(r("Gen", 1, 1), r("John", 3, 16)),
            Link::canon(r("Gen", 1, 2), r("John", 3, 16)),
        ]);
        let mut w2 = Weave::empty("beta", WeaveKind::Quotation, "kjv1769-tok2", "now");
        w2.add_links([Link::canon(r("Gen", 1, 1), r("Gen", 1, 2))]);
        let loaded = vec![
            LoadedWeave { file: "a.json".into(), weave: w1 },
            LoadedWeave { file: "b.json".into(), weave: w2 },
        ];

        // Unpinned: the larger lane (w1, 2 links) comes first, then w2.
        let c = constellation(&loaded, &corpus, 0, &[]);
        assert_eq!(c.lanes.len(), 2);
        assert_eq!(c.lanes[0].weave_index, 0);
        assert_eq!(c.lanes[0].name, "alpha");
        assert!(!c.lanes[0].pinned);
        assert_eq!(c.lanes[0].edges.len(), 2);
        // Gen 1:1, John 3:16 (shared, deduped), Gen 1:2 → three nodes.
        assert_eq!(c.lanes[0].nodes.len(), 3);
        assert_eq!(c.n_pins, 0);
        assert_eq!(c.free_total, 2);
        assert_eq!(c.lane_capacity, CONSTELLATION_LANES);
        // Everything is an in-range fraction; jitter never clips the band.
        for lane in &c.lanes {
            for n in &lane.nodes {
                assert!((0.0..=1.0).contains(&n.x));
                assert!(n.lane_frac > 0.13 && n.lane_frac < 0.87);
                assert!((0.0..=1.0).contains(&n.size));
            }
        }
        // Genesis 1:1 anchors the very start of the backbone.
        let g11 = c.lanes[0].nodes.iter().find(|n| n.ref_key == "Gen 1:1").unwrap();
        assert!(g11.x < 0.02);
        assert_eq!(g11.book, "Gen");
        assert_eq!(g11.chapter, 1);

        // Pin w2 (library index 1): it jumps to lane 0 (pinned first).
        let c = constellation(&loaded, &corpus, 0, &[1]);
        assert_eq!(c.n_pins, 1);
        assert_eq!(c.free_total, 1);
        assert_eq!(c.lanes[0].weave_index, 1);
        assert!(c.lanes[0].pinned);
        assert_eq!(c.lanes[1].weave_index, 0);
        assert!(c.caption.starts_with("1 pinned · "));
    }

    #[test]
    fn components_are_transitive() {
        // A-B and B-C should form one component {A,B,C}; D-E a second.
        let links = vec![
            Link::canon(r("Gen", 1, 1), r("Gen", 1, 2)),
            Link::canon(r("Gen", 1, 2), r("Gen", 1, 3)),
            Link::canon(r("Exod", 1, 1), r("Exod", 1, 2)),
        ];
        let comps = components(&links);
        assert_eq!(comps.len(), 2);
        let big = comps.iter().find(|c| c.len() == 3).unwrap();
        assert!(big.contains(&r("Gen", 1, 1)) && big.contains(&r("Gen", 1, 3)));
    }

    #[test]
    fn smart_links_zip_vs_cross() {
        // equal-length panes zip 1:1
        let zip = smart_links(&[vec![r("Gen", 1, 1), r("Gen", 1, 2)], vec![r("Matt", 1, 1), r("Matt", 1, 2)]]);
        assert_eq!(zip.len(), 2);
        // unequal → cross product
        let cross = smart_links(&[vec![r("Gen", 1, 1)], vec![r("Matt", 1, 1), r("Matt", 1, 2)]]);
        assert_eq!(cross.len(), 2);
    }

    #[test]
    fn weave_json_roundtrip() {
        let mut w = Weave::empty("The first and last Adam", WeaveKind::Typological, "kjv1769-tok2", "2026-01-01");
        w.notes = "hand note".into();
        w.notes_source = NotesSource::Hand;
        w.add_links([Link::canon_labelled(r("Gen", 2, 7), r("1Cor", 15, 45), "living soul")]);
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains(r#""format":"overlay-weave-v2""#));
        assert!(json.contains(r#""kind":"type""#));
        assert!(json.contains(r#""a":"Gen 2:7""#));
        let back: Weave = serde_json::from_str(&json).unwrap();
        assert_eq!(back, w);
    }

    #[test]
    fn file_slug() {
        assert_eq!(
            weave_file_in("weaves", "The First & Last Adam!"),
            std::path::Path::new("weaves/the-first-last-adam.json")
        );
        assert_eq!(weave_file_in("weaves", "  "), std::path::Path::new("weaves/weave.json"));
    }
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use crate::VRef;

    /// REVIEW 2026-07-14 correctness #1: a name-matching *suggestion* must
    /// never receive a user's link — rejecting the suggestion later would
    /// delete it. add_link creates/extends a canonical weave instead.
    #[test]
    fn add_link_never_appends_to_a_suggestion() {
        let home = std::env::temp_dir().join(format!("plumbline-weave-sugg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let sug_dir = home.join("weaves").join("suggested");
        std::fs::create_dir_all(&sug_dir).unwrap();

        let mut sug = Weave::empty("Echoes", WeaveKind::Quotation, "kjv1769-tok2", "c");
        sug.add_links([Link::canon(VRef::new("Gen", 1, 1), VRef::new("John", 1, 1))]);
        let sug_file = sug_dir.join("echoes.json");
        write_weave(&sug_file, &sug).unwrap();
        let loaded = vec![LoadedWeave { file: sug_file.clone(), weave: sug }];

        let out = add_link(
            &home,
            &loaded,
            "Echoes",
            WeaveKind::Quotation,
            "kjv1769-tok2",
            "2026-01-01T00:00:00Z",
            Link::canon(VRef::new("Exod", 3, 14), VRef::new("John", 8, 58)),
        )
        .unwrap();

        // A fresh canonical file — not the suggestion.
        assert_ne!(out, sug_file);
        assert!(out.starts_with(home.join("weaves")));
        assert!(!out.starts_with(&sug_dir));
        // The suggestion is untouched (still exactly one link).
        let (reloaded, _) = load_weaves(&home);
        let s = reloaded.iter().find(|lw| is_suggested(lw)).unwrap();
        assert_eq!(s.weave.links.len(), 1);
        let _ = std::fs::remove_dir_all(&home);
    }
}
