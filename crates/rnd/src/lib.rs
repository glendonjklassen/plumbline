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

/// Which R&D capabilities this build was compiled with. The UI queries this to
/// decide which panels/toggles to even show (decision #4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub bridge: bool,
    pub embeddings: bool,
    pub morphology: bool,
    pub keyness: bool,
    pub trust: bool,
}

/// The capabilities compiled into this build.
pub const fn capabilities() -> Capabilities {
    Capabilities {
        bridge: cfg!(feature = "bridge"),
        embeddings: cfg!(feature = "embeddings"),
        morphology: cfg!(feature = "morphology"),
        keyness: cfg!(feature = "keyness"),
        trust: cfg!(feature = "trust"),
    }
}

/// Whether any R&D capability is compiled in at all. When false, the UI stays
/// in pure-reader mode with no "Full study" affordances.
pub const fn any_enabled() -> bool {
    let c = capabilities();
    c.bridge || c.embeddings || c.morphology || c.keyness || c.trust
}

#[cfg(test)]
mod tests {
    use super::*;

    // Only meaningful in a featureless build (a plain `cargo test`); skip it
    // when any R&D feature is enabled on the command line.
    #[cfg(not(any(
        feature = "bridge",
        feature = "embeddings",
        feature = "morphology",
        feature = "keyness",
        feature = "trust"
    )))]
    #[test]
    fn default_build_has_no_rnd() {
        assert!(!any_enabled());
        assert_eq!(capabilities(), Capabilities::default());
    }
}
