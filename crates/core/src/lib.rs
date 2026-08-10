//! `plumbline-core` — Plumbline's pure, headless domain core.
//!
//! A ground-up Rust port of the study logic in the Haskell `overlay` app: the
//! canon, verse references, the tokenized KJV corpus, Strong's dictionary +
//! concordance, multi-tier search, and the weave graph. No UI, no rendering,
//! no I/O beyond loading data files — everything here is deterministic and
//! unit-testable, so the native shells (GTK/WinUI/Compose) and the FFI layer
//! can sit on top without duplicating any study logic.
//!
//! Optional "R&D" features (morphology, concept, bridge) live in
//! the separate, feature-gated `plumbline-rnd` crate so a simple-reader build can
//! omit them entirely.

pub mod akjv;
pub mod canon;
pub mod church;
pub mod civil;
pub mod config;
pub mod corpus;
pub mod crossref;
pub mod export;
pub mod font;
pub mod home;
pub mod hymnal;
pub mod i18n;
pub mod memory;
pub mod notes;
pub mod panel;
pub mod plan;
pub mod reading;
pub mod reference;
pub mod renderings;
pub mod search;
pub mod store;
pub mod strongs;
pub mod tag;
pub mod theme;
pub mod thread;
pub mod usernote;
pub mod versification;
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
