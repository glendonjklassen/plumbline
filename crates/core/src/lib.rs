//! `pure-core` — the pure, headless domain core for pure-study.
//!
//! A ground-up Rust port of the study logic in the Haskell `overlay` app: the
//! canon, verse references, the tokenized KJV corpus, Strong's dictionary +
//! concordance, multi-tier search, and the weave graph. No UI, no rendering,
//! no I/O beyond loading data files — everything here is deterministic and
//! unit-testable, so the native shells (GTK/WinUI/Compose) and the FFI layer
//! can sit on top without duplicating any study logic.
//!
//! Optional "R&D" features (embeddings, morphology, keyness, trust) live in
//! the separate, feature-gated `pure-rnd` crate so a simple-reader build can
//! omit them entirely.

pub mod canon;
pub mod config;
pub mod corpus;
pub mod notes;
pub mod reference;
pub mod search;
pub mod store;
pub mod strongs;
pub mod tag;
pub mod thread;
pub mod weave;

pub use corpus::{Corpus, Token, Verse};
pub use reference::VRef;

/// The crate's error type. Loaders return this; pure functions don't fail.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("parse error: {0}")]
    Parse(String),
    #[error("corpus error: {0}")]
    Corpus(String),
}
