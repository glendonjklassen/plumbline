//! The text-as-witness loader: a lexicon-free Hebrew↔Greek alignment that can
//! disbelieve links the bridge and its sources assert — seeded from the phonetic
//! correspondence of transliterated proper names, grown by usage geometry alone.
//!
//! Ported from overlay `Witness.hs` — the loader and the gate only; the offline
//! `ml/text_witness.py` produces `data/text-witness.json`. [`TextWitness::disbelief`]
//! returns a percentile only once the witness has passed its held-out grading
//! (`witnessQualified` and `testimonyActionable`), so an ungraded witness cannot
//! surface an accusation. Both flags are false in the shipped data, so it always
//! returns `None` today.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// The loaded witness: whether it is graded fit to speak, and the (Hebrew,
/// Greek) pairs it independently disbelieves twice → disbelief percentile
/// (0 believed … 1 maximally disbelieved). Keyed Hebrew-first, like every
/// cross-testament map in the codebase; the stronger percentile wins on
/// collision.
#[derive(Debug, Clone, Default)]
pub struct TextWitness {
    qualified: bool,
    actionable: bool,
    twice: HashMap<(String, String), f64>,
}

#[derive(Deserialize)]
struct TwicePair {
    h: String,
    g: String,
    #[serde(rename = "disbeliefPct")]
    pct: f64,
}

#[derive(Deserialize)]
struct Testimony {
    #[serde(rename = "independentlyDisbelievedTwice", default)]
    twice: Vec<TwicePair>,
}

#[derive(Deserialize)]
struct WitnessWire {
    #[serde(rename = "witnessQualified", default)]
    qualified: bool,
    #[serde(rename = "testimonyActionable", default)]
    actionable: bool,
    #[serde(default)]
    testimony: HashMap<String, Testimony>,
}

impl TextWitness {
    /// The silent witness: qualifies nothing, disbelieves nothing. What a
    /// missing/corrupt file (or first run) falls back to.
    pub fn empty() -> TextWitness {
        TextWitness::default()
    }

    /// Load `data/text-witness.json`, or the silent witness if absent/corrupt
    /// (a bad file degrades the feature, never crashes the reader).
    pub fn load(path: impl AsRef<Path>) -> TextWitness {
        match std::fs::read(path.as_ref()) {
            Ok(bytes) => match serde_json::from_slice::<WitnessWire>(&bytes) {
                Ok(w) => {
                    let mut twice: HashMap<(String, String), f64> = HashMap::new();
                    for t in w.testimony.values() {
                        for p in &t.twice {
                            let e = twice.entry((p.h.clone(), p.g.clone())).or_insert(f64::MIN);
                            *e = e.max(p.pct);
                        }
                    }
                    TextWitness { qualified: w.qualified, actionable: w.actionable, twice }
                }
                Err(_) => TextWitness::empty(),
            },
            Err(_) => TextWitness::empty(),
        }
    }

    /// The disbelief percentile for a (Hebrew, Greek) pair, but only once the
    /// witness is graded fit (`qualified` and `actionable`) — otherwise `None`
    /// regardless of the stored pairs. The UI contract is a one-line "a second
    /// witness disbelieves this link" badge, shown iff this is `Some`.
    pub fn disbelief(&self, heb: &str, grk: &str) -> Option<f64> {
        if self.qualified && self.actionable {
            self.twice.get(&(heb.to_string(), grk.to_string())).copied()
        } else {
            None
        }
    }

    /// How many pairs the witness disbelieves twice (diagnostic; ungated).
    pub fn pair_count(&self) -> usize {
        self.twice.len()
    }

    /// Whether the witness is graded fit to speak at all.
    pub fn is_qualified(&self) -> bool {
        self.qualified && self.actionable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNQUALIFIED: &str = r#"{
        "format":"overlay-text-witness-v1",
        "witnessQualified": false, "testimonyActionable": false,
        "testimony": { "text": { "independentlyDisbelievedTwice": [ {"h":"H1","g":"G43","disbeliefPct":0.9} ] } }
    }"#;
    const QUALIFIED: &str = r#"{
        "witnessQualified": true, "testimonyActionable": true,
        "testimony": {
          "text": { "independentlyDisbelievedTwice": [ {"h":"H1","g":"G43","disbeliefPct":0.7} ] },
          "audit": { "independentlyDisbelievedTwice": [ {"h":"H1","g":"G43","disbeliefPct":0.9} ] }
        }
    }"#;

    fn write(tmp: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("plumbline-witness-{}-{tmp}.json", std::process::id()));
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn unqualified_witness_is_silent() {
        let w = TextWitness::load(write("unq", UNQUALIFIED));
        assert_eq!(w.pair_count(), 1); // it recorded the pair…
        assert!(!w.is_qualified());
        assert_eq!(w.disbelief("H1", "G43"), None); // …but cannot accuse
    }

    #[test]
    fn qualified_witness_speaks_with_the_strongest_percentile() {
        let w = TextWitness::load(write("q", QUALIFIED));
        assert!(w.is_qualified());
        // Two sources flag the pair (0.7, 0.9) → the stronger wins.
        assert_eq!(w.disbelief("H1", "G43"), Some(0.9));
        assert_eq!(w.disbelief("H1", "G999"), None);
    }

    #[test]
    fn missing_file_is_silent() {
        let w = TextWitness::load("/no/such/text-witness.json");
        assert!(!w.is_qualified());
        assert_eq!(w.pair_count(), 0);
    }
}
