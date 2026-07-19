//! The Old↔New Testament concept bridge: links between Hebrew Strong's numbers
//! (`H…`, OT) and Greek ones (`G…`, NT), which otherwise share no numbering.
//! Without it, following a theme across the testaments is impossible.
//!
//! Ported from overlay `Bridge.hs` — the `etymologyLinks` layer only, which is
//! **Strong's own 1890 cross-references**: Greek entries whose derivation says
//! "of Hebrew origin (Hxxxx)". Authoritative but narrow (loanwords, proper
//! nouns, cultic terms), and — crucially — derived entirely from the Strong's
//! dictionary already in the repo: no Septuagint, no modern lexicon, no
//! embeddings. The noisier `renderingCandidates` layer and the fused
//! multi-source trust model are deferred to the data-pack tier.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use pure_core::strongs::StrongsDict;

/// One directed etymology link: a Greek Strong's code that Strong recorded as
/// being of the given Hebrew code's origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeLink {
    pub heb: String,
    pub grk: String,
}

/// Extract Hebrew Strong's references (`H####`) embedded in free text — e.g. the
/// `(H0031)` in a derivation — normalised to the zero-stripped dictionary style
/// (`H31`). Case-insensitive on the leading `H`.
pub fn heb_refs_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'H' || c == b'h' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 {
                // Zero-strip: parse then re-print, matching the dictionary keys.
                if let Ok(n) = text[i + 1..j].parse::<u32>() {
                    out.push(format!("H{n}"));
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Every etymology link Strong himself recorded: for each Greek entry whose
/// derivation cites a Hebrew origin, one link per cited Hebrew code.
pub fn etymology_links(dict: &StrongsDict) -> Vec<BridgeLink> {
    let mut out = Vec::new();
    for (code, entry) in dict {
        if !code.starts_with('G') {
            continue;
        }
        let Some(d) = entry.deriv.as_deref() else { continue };
        // "of Hebrew origin" — match the substring overlay keys on.
        if !d.contains("ebrew") {
            continue;
        }
        for h in heb_refs_in(d) {
            out.push(BridgeLink { heb: h, grk: code.clone() });
        }
    }
    out
}

/// The resolved bridge: an undirected adjacency from each Strong's code to the
/// codes on the far testament it links to (deduped, in first-seen order).
#[derive(Debug, Clone, Default)]
pub struct Bridge {
    partners: HashMap<String, Vec<String>>,
}

impl Bridge {
    /// Build the bridge from Strong's own etymology links.
    pub fn from_etymology(dict: &StrongsDict) -> Bridge {
        let mut b = Bridge::default();
        for l in etymology_links(dict) {
            b.insert_both(&l.heb, &l.grk);
        }
        b
    }

    fn insert_both(&mut self, a: &str, b: &str) {
        push_unique(self.partners.entry(a.to_string()).or_default(), b);
        push_unique(self.partners.entry(b.to_string()).or_default(), a);
    }

    /// The codes on the opposite testament linked to `code` (empty if none).
    pub fn partners(&self, code: &str) -> &[String] {
        self.partners.get(code).map(Vec::as_slice).unwrap_or(&[])
    }

    /// How many codes participate in at least one link.
    pub fn len(&self) -> usize {
        self.partners.len()
    }

    pub fn is_empty(&self) -> bool {
        self.partners.is_empty()
    }
}

fn push_unique(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

// ── external source links + trust (the fused bridge) ───────────────────────────

/// A Hebrew↔Greek link asserted by an external witness (LXX alignment,
/// Abbott-Smith, STEPBible TIPNR, harvested quotations). Corroborating
/// evidence, never ground truth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLink {
    pub heb: String,
    pub grk: String,
    pub source: String,
}

#[derive(Deserialize)]
struct SourceLinkWire {
    h: String,
    g: String,
    source: String,
}

#[derive(Deserialize)]
struct BridgeSourcesWire {
    #[serde(default)]
    links: Vec<SourceLinkWire>,
}

/// Load every external bridge-source link under a home: the committed
/// `bridge/*.json` (redistributable: LXX, Abbott-Smith, TIPNR), plus the
/// optional hydrated `data/bridge-sources.json` and `data/quotation-pairs.json`.
/// Missing/unreadable files are skipped, so the fused bridge degrades to the
/// in-repo etymology layer when nothing external is present.
pub fn load_sources(home: impl AsRef<Path>) -> Vec<SourceLink> {
    let home = home.as_ref();
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(home.join("bridge")) {
        let mut here: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        here.sort();
        files.extend(here);
    }
    files.push(home.join("data").join("bridge-sources.json"));
    files.push(home.join("data").join("quotation-pairs.json"));

    let mut out = Vec::new();
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if let Ok(w) = serde_json::from_slice::<BridgeSourcesWire>(&bytes) {
            out.extend(w.links.into_iter().map(|l| SourceLink { heb: l.h, grk: l.g, source: l.source }));
        }
    }
    out
}

/// Per-source trust priors (`data/source-priors.json`), fitted offline against
/// the Abbott-Smith gold. `prior(source)` falls back to `_default`, then 0.5.
#[derive(Debug, Clone)]
pub struct Priors {
    map: HashMap<String, f32>,
}

#[derive(Deserialize)]
struct PriorsWire {
    #[serde(default)]
    priors: HashMap<String, f32>,
}

impl Default for Priors {
    fn default() -> Priors {
        Priors { map: HashMap::new() }
    }
}

impl Priors {
    /// The prior for a source; `_default` if unlisted, else 0.5.
    pub fn prior(&self, source: &str) -> f32 {
        self.map
            .get(source)
            .copied()
            .or_else(|| self.map.get("_default").copied())
            .unwrap_or(0.5)
    }
}

/// Load `data/source-priors.json`; a missing/unreadable file yields empty priors
/// (every source then scores the 0.5 default).
pub fn load_priors(home: impl AsRef<Path>) -> Priors {
    let path = home.as_ref().join("data").join("source-priors.json");
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<PriorsWire>(&bytes) {
            Ok(w) => Priors { map: w.priors },
            Err(_) => Priors::default(),
        },
        Err(_) => Priors::default(),
    }
}

/// The authority tier of a piece of evidence — where it comes from, so the
/// reader always knows the provenance of what they are looking at. Ported from
/// overlay `Bridge.hs` `Tier`.
///
/// - [`Tier::God`] — the text itself: TR/Masoretic words, and
///   scripture-quotes-scripture, which is "the words read twice" and so
///   inherits the text's own authority.
/// - [`Tier::Human`] — curated scholarship: lexicons, the 1769 translators'
///   renderings, TSK.
/// - [`Tier::Machine`] — learned/aligned artifacts (the LXX alignment, concept
///   embeddings, the R&D layer). Also the default for an unrecognized source,
///   so nothing over-claims authority it has not earned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    God,
    Human,
    Machine,
}

impl Tier {
    /// The stable lowercase wire name (`"god"` / `"human"` / `"machine"`).
    pub fn wire_name(self) -> &'static str {
        match self {
            Tier::God => "god",
            Tier::Human => "human",
            Tier::Machine => "machine",
        }
    }
}

/// The tier(s) a witness source attests — a *set*, because one source can carry
/// both a content tier and a method tier (`quotation` is God-tier content found
/// by a machine method). An unrecognized source defaults to machine-only, so it
/// never over-claims. Ported from overlay `sourceTiers`.
pub fn source_tiers(source: &str) -> &'static [Tier] {
    match source {
        "quotation" => &[Tier::God, Tier::Machine],
        "etymology" | "rendering" | "abbott-smith" | "stepbible-tbesg" | "stepbible-tipnr"
        | "tsk" => &[Tier::Human],
        "lxx" | "embedding" | "text-witness" => &[Tier::Machine],
        _ => &[Tier::Machine],
    }
}

/// Whether a source's *method* has not yet passed its held-out grading — a
/// lead, not a result. Ported from overlay `researchGrade`.
pub fn research_grade(source: &str) -> bool {
    matches!(source, "quotation" | "text-witness")
}

/// A short human label for a witness source, tuned for lay readers (no
/// Greek/Hebrew assumed). Ported from overlay `sourceLabel`, keeping the app's
/// own plain wording for the sources it surfaces.
pub fn source_label(source: &str) -> &str {
    match source {
        "etymology" => "etymology",
        "lxx" => "Septuagint",
        "quotation" => "NT quotation",
        "abbott-smith" => "Abbott-Smith (1922)",
        "stepbible-tbesg" => "STEPBible",
        "stepbible-tipnr" => "STEPBible names",
        "rendering" => "1769 renderings",
        "tsk" => "Treasury of Scripture Knowledge",
        other => other,
    }
}

/// The deduped union of tiers across a set of witness sources, ordered
/// God → Human → Machine. The additive provenance model: a multi-source item
/// shows every tier its witnesses attest, never a single "winning" one.
pub fn tiers_of<S: AsRef<str>>(sources: &[S]) -> Vec<Tier> {
    [Tier::God, Tier::Human, Tier::Machine]
        .into_iter()
        .filter(|t| sources.iter().any(|s| source_tiers(s.as_ref()).contains(t)))
        .collect()
}

/// A cross-testament partner with the witnesses that assert it and their best
/// trust prior. `etymology` (Strong's own derivations) is treated as an
/// authoritative in-repo witness.
#[derive(Debug, Clone, PartialEq)]
pub struct Partner {
    pub code: String,
    pub sources: Vec<String>,
    pub prior: f32,
}

/// The etymology bridge fused with the external witnesses and weighted by their
/// trust priors. Built once; queried per lemma.
#[derive(Debug, Clone)]
pub struct FusedBridge {
    etymology: Bridge,
    source_ix: HashMap<String, Vec<SourceLink>>,
    priors: Priors,
}

/// The implicit prior for Strong's own etymology derivations — authoritative,
/// so it ranks with (just above) the strongest external witness.
const ETYMOLOGY_PRIOR: f32 = 0.95;

impl FusedBridge {
    /// Build from the Strong's dictionary (etymology) plus a home's external
    /// source files and trust priors.
    pub fn build(dict: &StrongsDict, home: impl AsRef<Path>) -> FusedBridge {
        let home = home.as_ref();
        let etymology = Bridge::from_etymology(dict);
        let links = load_sources(home);
        let mut source_ix: HashMap<String, Vec<SourceLink>> = HashMap::new();
        for l in links {
            source_ix.entry(l.heb.clone()).or_default().push(l.clone());
            source_ix.entry(l.grk.clone()).or_default().push(l);
        }
        FusedBridge { etymology, source_ix, priors: load_priors(home) }
    }

    /// The dictionary-only bridge: the etymology layer with no external
    /// witnesses and default priors. For engines opened without a home dir —
    /// no filesystem is probed (a CWD-relative probe would be
    /// nondeterministic and a mild injection surface).
    pub fn etymology_only(dict: &StrongsDict) -> FusedBridge {
        FusedBridge {
            etymology: Bridge::from_etymology(dict),
            source_ix: HashMap::new(),
            priors: Priors::default(),
        }
    }

    /// How many external source links were loaded (for reporting).
    pub fn source_link_count(&self) -> usize {
        // Each link is indexed under both endpoints, so halve.
        self.source_ix.values().map(Vec::len).sum::<usize>() / 2
    }

    /// The other-testament partners of `code`, each with its witnessing sources
    /// and best prior, ranked strongest-first (ties broken by code). Merges the
    /// etymology partners with every external source that ties the same lemma.
    pub fn partners(&self, code: &str) -> Vec<Partner> {
        let mut acc: HashMap<String, (Vec<String>, f32)> = HashMap::new();

        for p in self.etymology.partners(code) {
            let e = acc.entry(p.clone()).or_insert_with(|| (Vec::new(), 0.0));
            push_unique(&mut e.0, "etymology");
            e.1 = e.1.max(ETYMOLOGY_PRIOR);
        }
        if let Some(links) = self.source_ix.get(code) {
            let is_greek = code.starts_with('G');
            for l in links {
                // The partner is the endpoint on the *other* testament.
                let partner = if is_greek { &l.heb } else { &l.grk };
                // Guard against a same-language link (shouldn't occur, but be safe).
                if partner.starts_with(if is_greek { 'G' } else { 'H' }) {
                    continue;
                }
                let e = acc.entry(partner.clone()).or_insert_with(|| (Vec::new(), 0.0));
                push_unique(&mut e.0, &l.source);
                e.1 = e.1.max(self.priors.prior(&l.source));
            }
        }

        let mut out: Vec<Partner> =
            acc.into_iter().map(|(code, (sources, prior))| Partner { code, sources, prior }).collect();
        out.sort_by(|a, b| b.prior.total_cmp(&a.prior).then_with(|| a.code.cmp(&b.code)));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pure_core::strongs::StrongsEntry;

    fn entry(deriv: Option<&str>) -> StrongsEntry {
        StrongsEntry {
            lemma: None,
            xlit: None,
            pron: None,
            deriv: deriv.map(String::from),
            def: None,
            kjv: None,
        }
    }

    #[test]
    fn extracts_and_normalises_hebrew_refs() {
        assert_eq!(heb_refs_in("of Hebrew origin (H0031)"), vec!["H31"]);
        assert_eq!(heb_refs_in("from (H1254) and (h430)"), vec!["H1254", "H430"]);
        assert_eq!(heb_refs_in("no refs here"), Vec::<String>::new());
    }

    #[test]
    fn source_tiers_classify_provenance() {
        // God-tier content found by a machine method — carries both.
        assert_eq!(source_tiers("quotation"), &[Tier::God, Tier::Machine]);
        // Curated scholarship.
        assert_eq!(source_tiers("etymology"), &[Tier::Human]);
        assert_eq!(source_tiers("abbott-smith"), &[Tier::Human]);
        assert_eq!(source_tiers("rendering"), &[Tier::Human]);
        assert_eq!(source_tiers("tsk"), &[Tier::Human]);
        // Learned/aligned artifacts.
        assert_eq!(source_tiers("lxx"), &[Tier::Machine]);
        assert_eq!(source_tiers("embedding"), &[Tier::Machine]);
        // Unknown → machine-only, so it never over-claims.
        assert_eq!(source_tiers("who-knows"), &[Tier::Machine]);
    }

    #[test]
    fn research_grade_flags_ungraded_methods() {
        assert!(research_grade("quotation"));
        assert!(research_grade("text-witness"));
        assert!(!research_grade("etymology"));
        assert!(!research_grade("lxx"));
    }

    #[test]
    fn tiers_of_unions_and_orders_god_human_machine() {
        // A quotation alone already spans God + Machine.
        assert_eq!(
            tiers_of(&["quotation".to_string()]),
            vec![Tier::God, Tier::Machine]
        );
        // Etymology (Human) + LXX (Machine) fused on one partner: both marks,
        // ordered God→Human→Machine (no God here).
        assert_eq!(
            tiers_of(&["etymology".to_string(), "lxx".to_string()]),
            vec![Tier::Human, Tier::Machine]
        );
        // All three tiers present, deduped and ordered.
        assert_eq!(
            tiers_of(&["quotation".to_string(), "etymology".to_string()]),
            vec![Tier::God, Tier::Human, Tier::Machine]
        );
        assert!(tiers_of::<String>(&[]).is_empty());
    }

    #[test]
    fn source_label_is_lay_friendly() {
        assert_eq!(source_label("lxx"), "Septuagint");
        assert_eq!(source_label("quotation"), "NT quotation");
        assert_eq!(source_label("abbott-smith"), "Abbott-Smith (1922)");
        // Unknown falls back to the raw key.
        assert_eq!(source_label("mystery"), "mystery");
    }

    #[test]
    fn fused_bridge_merges_etymology_and_sources_by_prior() {
        let home = std::env::temp_dir().join(format!("pure-fused-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("bridge")).unwrap();
        std::fs::create_dir_all(home.join("data")).unwrap();

        // Etymology: G43 of Hebrew origin H1.
        let mut dict = StrongsDict::new();
        dict.insert("G43".into(), entry(Some("of Hebrew origin (H0001)")));
        dict.insert("H1".into(), entry(None));

        // External LXX source ties H1 ↔ G43 (corroborates etymology) and H1 ↔ G99.
        std::fs::write(
            home.join("bridge").join("lxx-alignment.json"),
            r#"{"format":"overlay-bridge-sources-v1","links":[{"h":"H1","g":"G43","source":"lxx"},{"h":"H1","g":"G99","source":"lxx"}]}"#,
        )
        .unwrap();
        std::fs::write(
            home.join("data").join("source-priors.json"),
            r#"{"priors":{"lxx":0.85,"_default":0.5}}"#,
        )
        .unwrap();

        let fb = FusedBridge::build(&dict, &home);
        assert_eq!(fb.source_link_count(), 2);
        let ps = fb.partners("H1");
        // G43: witnessed by both etymology (0.95) and lxx → prior 0.95, both sources.
        let g43 = ps.iter().find(|p| p.code == "G43").unwrap();
        assert!((g43.prior - 0.95).abs() < 1e-6);
        assert!(g43.sources.contains(&"etymology".to_string()) && g43.sources.contains(&"lxx".to_string()));
        // G99: lxx only → prior 0.85.
        let g99 = ps.iter().find(|p| p.code == "G99").unwrap();
        assert!((g99.prior - 0.85).abs() < 1e-6);
        // Ranked strongest-first.
        assert_eq!(ps[0].code, "G43");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn links_greek_of_hebrew_origin_only() {
        let mut dict = StrongsDict::new();
        dict.insert("G43".into(), entry(Some("of Hebrew origin (H0001)")));
        dict.insert("G999".into(), entry(Some("a native Greek word")));
        dict.insert("H1".into(), entry(None));

        let links = etymology_links(&dict);
        assert_eq!(links, vec![BridgeLink { heb: "H1".into(), grk: "G43".into() }]);

        let bridge = Bridge::from_etymology(&dict);
        assert_eq!(bridge.partners("H1"), ["G43"]);
        assert_eq!(bridge.partners("G43"), ["H1"]);
        assert!(bridge.partners("G999").is_empty());
    }
}
