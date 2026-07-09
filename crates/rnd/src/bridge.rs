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
