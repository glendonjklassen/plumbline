//! `plumbline-rnd` — the optional, feature-gated "R&D" layer for Plumbline.
//!
//! Everything a casual reader should never be forced to see lives here, behind
//! cargo features: the OT↔NT etymology bridge (`bridge`), the morphology layer
//! (`morphology`), and the symbolic concept engine (`concept`). A simple-reader
//! build depends on `plumbline-rnd` with no features and compiles none of it.
//!
//! Ported from overlay `Concept*`, `Morph`, `Burst`, `Witness`, `Bridge`.

/// The OT↔NT etymology bridge (Strong's-derived, no ML data). Compiled in with
/// the `bridge` feature.
#[cfg(feature = "bridge")]
pub mod bridge;

/// The text-as-witness loader: a graded, lexicon-free alignment that can
/// disbelieve bridge links. Bundled with the `bridge` feature.
#[cfg(feature = "bridge")]
pub mod witness;

/// Morphology: OSHM/Robinson parsing-code parsers + renderer + sidecar loader
/// over the offline-projected `morphology.jsonl`. With the `morphology` feature.
#[cfg(feature = "morphology")]
pub mod morph;

/// Symbolic concept engine: co-occurrence statistics + community graph over the
/// corpus (no ML data). With the `concept` feature.
#[cfg(feature = "concept")]
pub mod concept;

/// Grammatical function-word Strong's codes, excluded from concept-neighbour
/// surfaces (pure data, no feature gate — used by the `concept` engine).
pub mod stopwords;

/// Leitwort / burst discovery (Poisson scan statistic over concept positions).
/// Bundled with the `concept` feature.
#[cfg(feature = "concept")]
pub mod burst;

/// Which R&D capabilities this build was *compiled* with — a build fact, not what
/// the reader sees. Which panels appear is decided by `Config::scholars_analysis`
/// / `machine_analysis`, a reader preference; a build can have `concept` compiled
/// in with the reader's machine tier switched off. Every shipped build has all
/// three (`crates/ffi/Cargo.toml`); `plumbline-hydrate` takes two.
///
/// Nothing carries this across the C ABI yet. Kept because it is a `const fn`
/// over `cfg!` (no runtime cost) and because the tests below are the only thing
/// watching the feature gating: CI's `rnd-featureless` job would otherwise prove
/// only that the crate compiles with no features, not that feature unification
/// from a sibling `-p` flag left the tiers off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub bridge: bool,
    pub morphology: bool,
    pub concept: bool,
}

/// The capabilities compiled into this build.
pub const fn capabilities() -> Capabilities {
    Capabilities {
        bridge: cfg!(feature = "bridge"),
        morphology: cfg!(feature = "morphology"),
        concept: cfg!(feature = "concept"),
    }
}

/// Whether any R&D capability is compiled in at all. When false, the UI stays
/// in reader-only mode with no "Full study" affordances.
pub const fn any_enabled() -> bool {
    let c = capabilities();
    c.bridge || c.morphology || c.concept
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // Only meaningful in a featureless build (a plain `cargo test`); skip it
    // when any R&D feature is enabled on the command line.
    #[cfg(not(any(feature = "bridge", feature = "morphology", feature = "concept")))]
    #[test]
    fn default_build_has_no_rnd() {
        assert!(!any_enabled());
        assert_eq!(capabilities(), Capabilities::default());
    }

    // The mirror of the above, for the all-features build CI runs as its own
    // step: without it the report could go permanently all-false in the build
    // that ships, and only the featureless job — where all-false is correct —
    // would be watching.
    #[cfg(all(feature = "bridge", feature = "morphology", feature = "concept"))]
    #[test]
    fn full_build_reports_every_tier() {
        assert!(any_enabled());
        let c = capabilities();
        assert!(c.bridge, "bridge compiled in but not reported");
        assert!(c.morphology, "morphology compiled in but not reported");
        assert!(c.concept, "concept compiled in but not reported");
    }

    // Every field must track its own feature. All-on and all-off cannot tell a
    // copy-pasted `cfg!` from a correct one, so this runs in the partial
    // configuration that can: bridge without concept.
    #[cfg(all(feature = "bridge", not(feature = "concept")))]
    #[test]
    fn each_tier_tracks_its_own_feature() {
        let c = capabilities();
        assert!(c.bridge, "bridge is on, so Capabilities::bridge must be true");
        assert!(!c.concept, "concept is off, so Capabilities::concept must be false");
        assert!(any_enabled(), "one tier on is enough for any_enabled()");
        assert_ne!(c, Capabilities::default(), "a partial build is not the empty report");
    }
}
