//! `pure-rnd` — the optional, feature-gated "R&D" layer for pure-study.
//!
//! Everything a casual reader should never be forced to see lives here, behind
//! cargo features: concept embeddings + neighbourhoods (`embeddings`), the
//! morphology layer (`morphology`), keyness / leitwort statistics (`keyness`),
//! and the adversarial source-trust / witness model (`trust`). A simple-reader
//! build depends on `pure-rnd` with no features and compiles none of it.
//!
//! Ported from overlay `Concept*`, `Embed`, `Morph`, `Burst`, `Witness`,
//! `Bridge`. **Stub for now** — the port lands after the reading core and the
//! GTK shell are proven end-to-end.

/// Which R&D capabilities this build was compiled with. The UI queries this to
/// decide which panels/toggles to even show (decision #4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub embeddings: bool,
    pub morphology: bool,
    pub keyness: bool,
    pub trust: bool,
}

/// The capabilities compiled into this build.
pub const fn capabilities() -> Capabilities {
    Capabilities {
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
    c.embeddings || c.morphology || c.keyness || c.trust
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_build_has_no_rnd() {
        // The workspace default features are empty, so a plain `cargo test`
        // sees a pure-reader capability set.
        assert!(!any_enabled());
        assert_eq!(capabilities(), Capabilities::default());
    }
}
