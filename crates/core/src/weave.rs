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
//! pure-study data is v2; a v1 file currently surfaces as a parse error rather
//! than being silently migrated. Port `migrateV1` when older data must load.

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
        self.key() == other.key()
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
        self.key().cmp(&other.key())
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

/// Slug a weave name into a JSON filename under `dir`. Ported from
/// `weaveFileIn`.
pub fn weave_file_in(dir: impl AsRef<Path>, name: &str) -> std::path::PathBuf {
    let cleaned: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let slug = cleaned.split_whitespace().collect::<Vec<_>>().join("-");
    let slug = if slug.is_empty() { "weave".to_string() } else { slug };
    dir.as_ref().join(format!("{slug}.json"))
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
    if let Some(lw) = loaded.iter().find(|lw| lw.weave.name.to_lowercase() == wanted) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn r(book: &str, c: u16, v: u16) -> VRef {
        VRef::new(book, c, v)
    }

    #[test]
    fn add_link_creates_appends_and_dedupes() {
        let home = std::env::temp_dir().join(format!("pure-weave-{}", std::process::id()));
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
