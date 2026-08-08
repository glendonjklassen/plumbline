//! `plumbline-rnd` — the optional, feature-gated "R&D" layer for Plumbline.
//!
//! Everything a casual reader should never be forced to see lives here, behind
//! cargo features: the OT↔NT etymology bridge (`bridge`), the morphology layer
//! (`morphology`), and the symbolic concept engine (`concept`). A simple-reader
//! build depends on `plumbline-rnd` with no features and compiles none of it.
//!
//! The learned concept embeddings (`embeddings`) were retired — the
//! nearest-neighbour "concepts near this one" surface was unreliable and is
//! gone from both shells; the symbolic `concept` engine below is what remains.
//!
//! Ported from overlay `Concept*`, `Embed`, `Morph`, `Burst`, `Witness`,
//! `Bridge`. The **etymology bridge** (`bridge` feature) is ported and pure
//! (Strong's-derived, no ML data). The embedding / morphology tiers depend on
//! hydrated ML data packs and land with the pack pipeline. (Keyness and the
//! source-trust model are not ported yet; their features return when the code
//! they gate exists, so `capabilities()` never advertises an empty panel.)

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

/// Which R&D capabilities this build was **compiled** with.
///
/// NOT the same axis as what the reader sees, and it is worth being exact about
/// that because the comment here used to claim it was. Which panels and toggles
/// appear is decided by `Config::scholars_analysis` / `machine_analysis` — a
/// reader PREFERENCE, set at first run (decision #4) and changeable in Settings.
/// This is a BUILD FACT: whether the code behind a tier was compiled in at all.
/// A build can perfectly well have `concept` compiled in and the reader's machine
/// tier switched off, and today every shipped build has all three (see
/// `crates/ffi/Cargo.toml`; `plumbline-hydrate` takes two).
///
/// So nothing crosses the C ABI yet, and this is deliberately not wired to a
/// shell: `plumbline-rnd` cannot see a shell, and the endpoint that should carry
/// it is the negotiated-capabilities handshake in TODO §H (which also has to
/// carry `defer_builds`, `warm_step`, and a live `PLUMBLINE_WIRE_VERSION` —
/// today emitted and read by nothing). Kept rather than deleted for two
/// reasons: it is a `const fn` over `cfg!`, so it costs no runtime and no bytes;
/// and the tests below are the only thing watching the feature gating itself.
/// CI runs a dedicated `rnd-featureless` job (ci.yml) precisely so feature
/// unification from a sibling `-p` flag cannot switch the tiers on behind our
/// back — and `default_build_has_no_rnd` is the only assertion in that job that
/// says anything about the gating (its three companions are `stopwords`, pure
/// data with no gate). Delete this and the job proves the crate compiles with no
/// features, not that it compiled no features in.
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
    // step. It had no assertion about `capabilities()` at all, so the report
    // could have gone permanently all-false in the build that ships (every
    // shipped build has all four — see `crates/ffi/Cargo.toml`) and only the
    // featureless job, where all-false is CORRECT, would have been watching.
    #[cfg(all(feature = "bridge", feature = "morphology", feature = "concept"))]
    #[test]
    fn full_build_reports_every_tier() {
        assert!(any_enabled());
        let c = capabilities();
        assert!(c.bridge, "bridge compiled in but not reported");
        assert!(c.morphology, "morphology compiled in but not reported");
        assert!(c.concept, "concept compiled in but not reported");
    }

    // Every field must track ITS OWN feature. All-on and all-off cannot tell a
    // copy-pasted `cfg!` from a correct one — both builds agree on every field
    // either way — so this runs in the PARTIAL configuration that can: bridge
    // without concept, which is `plumbline-hydrate`'s own feature set and what
    // `cargo test -p plumbline-rnd --features bridge` builds.
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
