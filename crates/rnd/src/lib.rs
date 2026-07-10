//! `pure-rnd` — the optional, feature-gated "R&D" layer for pure-study.
//!
//! Everything a casual reader should never be forced to see lives here, behind
//! cargo features: concept embeddings + neighbourhoods (`embeddings`), the
//! morphology layer (`morphology`), keyness / leitwort statistics (`keyness`),
//! and the adversarial source-trust / witness model (`trust`). A simple-reader
//! build depends on `pure-rnd` with no features and compiles none of it.
//!
//! Ported from overlay `Concept*`, `Embed`, `Morph`, `Burst`, `Witness`,
//! `Bridge`. The **etymology bridge** (`bridge` feature) is ported and pure
//! (Strong's-derived, no ML data). The embedding / morphology / keyness / trust
//! tiers depend on hydrated ML data packs and land with the pack pipeline.

/// The OT↔NT etymology bridge (Strong's-derived, no ML data). Compiled in with
/// the `bridge` feature.
#[cfg(feature = "bridge")]
pub mod bridge;

/// Concept embeddings: loader + neighbour search over the offline-trained
/// `concept-vectors.vec` artifact. Compiled in with the `embeddings` feature.
#[cfg(feature = "embeddings")]
pub mod embed;

/// Morphology: OSHM/Robinson parsing-code parsers + renderer + sidecar loader
/// over the offline-projected `morphology.jsonl`. With the `morphology` feature.
#[cfg(feature = "morphology")]
pub mod morph;

/// Symbolic concept engine: co-occurrence statistics + community graph over the
/// corpus (no ML data). With the `concept` feature.
#[cfg(feature = "concept")]
pub mod concept;

/// Which R&D capabilities this build was compiled with. The UI queries this to
/// decide which panels/toggles to even show (decision #4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub bridge: bool,
    pub embeddings: bool,
    pub morphology: bool,
    pub keyness: bool,
    pub trust: bool,
    pub concept: bool,
}

/// The capabilities compiled into this build.
pub const fn capabilities() -> Capabilities {
    Capabilities {
        bridge: cfg!(feature = "bridge"),
        embeddings: cfg!(feature = "embeddings"),
        morphology: cfg!(feature = "morphology"),
        keyness: cfg!(feature = "keyness"),
        trust: cfg!(feature = "trust"),
        concept: cfg!(feature = "concept"),
    }
}

/// Whether any R&D capability is compiled in at all. When false, the UI stays
/// in pure-reader mode with no "Full study" affordances.
pub const fn any_enabled() -> bool {
    let c = capabilities();
    c.bridge || c.embeddings || c.morphology || c.keyness || c.trust || c.concept
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // Only meaningful in a featureless build (a plain `cargo test`); skip it
    // when any R&D feature is enabled on the command line.
    #[cfg(not(any(
        feature = "bridge",
        feature = "embeddings",
        feature = "morphology",
        feature = "keyness",
        feature = "trust",
        feature = "concept"
    )))]
    #[test]
    fn default_build_has_no_rnd() {
        assert!(!any_enabled());
        assert_eq!(capabilities(), Capabilities::default());
    }
}
