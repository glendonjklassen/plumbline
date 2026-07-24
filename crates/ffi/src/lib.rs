//! `pure-ffi` — the single, flat C ABI over `pure-core` + `pure-layout`.
//!
//! Decision #1 (native-per-platform) says: define the app's data surface
//! **once** here as a C ABI, then let each native UI bind to it — csbindgen /
//! P-Invoke for C# (WinUI) and JNA/UniFFI for Kotlin (Android). Every shell
//! paints the display list the core produces and forwards input coordinates
//! back across this boundary; no study logic is reimplemented in Kotlin or C#.
//!
//! ## Shape of the ABI
//!
//! * Two **opaque handles**: [`PureEngine`] (the loaded corpus + Strong's +
//!   search/occurrence indices) and [`PureDisplayList`] (one laid-out chapter).
//!   C sees them as forward-declared structs; only these functions touch them.
//! * **Primitives** for scalar params (chapter numbers, coordinates).
//! * **JSON** (NUL-terminated UTF-8) for every structured return value. JSON is
//!   the lowest common denominator across C#, Kotlin, Swift and JS, it keeps the
//!   ABI tiny and stable (a future field is additive, not a struct-layout
//!   break), and it is exactly what a cross-device sync SaaS will speak later.
//!   The wire schemas live in the [`wire`] module and are the frozen contract.
//! * Layout keeps living in Rust: the caller passes a [`PureMeasureFn`] callback
//!   so `pure_layout::layout_chapter` measures text with the platform's own
//!   engine (Pango/DirectWrite/Android) while the hard line-breaking + per-word
//!   hit-region bookkeeping stays written once, here.
//!
//! ## Memory & safety contract (read before binding)
//!
//! * Every `*mut c_char` returned by a `pure_*` function is owned by the caller
//!   and must be released with [`pure_study_string_free`]. A null return means
//!   "no value" (blank query, unknown code) or an error (see per-fn docs).
//! * Every handle (`*mut PureEngine`, `*mut PureDisplayList`) must be released
//!   with its matching `*_free`. Freeing null is a no-op; double-free is UB.
//! * Input `*const c_char` / byte pointers are **borrowed for the call only**;
//!   the caller keeps ownership. Strings must be valid UTF-8.
//! * Every entry point is wrapped in `catch_unwind`: a Rust panic can never
//!   unwind across the C boundary (that would be UB). A panic surfaces as a
//!   null / `0` / `0.0` return instead.
//! * A `*const PureEngine` may be shared across threads for these read-only
//!   calls; a `*mut PureDisplayList` is single-owner (do not hit-test one from
//!   two threads at once — though all calls here are `&`-only, so it is in
//!   practice also safe to read concurrently).

use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;
use std::sync::OnceLock;

use pure_core::config;
use pure_core::corpus::{self, Corpus};
use pure_core::crossref::{self, XRefIx};
use pure_core::renderings::Renderings;
use pure_core::search::{self, Notes, SearchIx};
use pure_core::strongs::{self, OccurrenceIx, StrongsDict};
use pure_core::memory;
use pure_core::tag::{self, LoadedTag, TagTarget};
use pure_core::thread::{self, LoadedThread, ThreadEntry};
use pure_core::weave::{self, Link, LoadedWeave, WeaveKind};
use pure_core::panel::{self, PanelSource};
use pure_core::{canon, export, notes, theme, usernote, VRef};
use pure_layout::{layout_chapter, DisplayList, LayoutConfig, Measure};
use pure_rnd::{bridge, burst, concept, embed, morph};

mod wire;

// ── token flag bits (mirror `pure_core::corpus::FLAG_*`; exported to bindings)──
//
// Written as bare literals (not `= corpus::FLAG_*`) so cbindgen can const-fold
// them into `#define`s in the C header. The `const _` assertions below fail the
// build if they ever drift from the core's canonical values, so the mirror
// stays honest without costing the bindings.

/// Word supplied by the KJV translators (rendered in italics).
pub const PURE_FLAG_ADDED: u32 = 1;
/// The divine name.
pub const PURE_FLAG_DIVINE: u32 = 2;
/// Psalm superscription / title text.
pub const PURE_FLAG_TITLE: u32 = 4;
/// A paragraph mark (¶) precedes this word.
pub const PURE_FLAG_PARA: u32 = 8;

const _: () = assert!(PURE_FLAG_ADDED == corpus::FLAG_ADDED);
const _: () = assert!(PURE_FLAG_DIVINE == corpus::FLAG_DIVINE);
const _: () = assert!(PURE_FLAG_TITLE == corpus::FLAG_TITLE);
const _: () = assert!(PURE_FLAG_PARA == corpus::FLAG_PARA);

/// How many verse references an occurrence list returns before it is capped
/// (`total` in the JSON stays honest above this).
pub const OCCURRENCE_CAP: usize = 500;

/// The wire-JSON contract version. Bump on any **non-additive** change to the
/// payload shapes in `wire.rs` (renames/removals/retypes) so typed decoders
/// can fail loudly instead of silently reading nulls; purely additive fields
/// do not bump it. Exported to the C header; golden samples are pinned in
/// `tests.rs`.
pub const PURE_WIRE_VERSION: u32 = 1;

// ── opaque handles ────────────────────────────────────────────────────────────

/// The loaded, immutable study core: corpus + Strong's dictionary + the search
/// and occurrence indices every lookup rides on. Opaque to C; construct with
/// [`pure_engine_open`] / [`pure_engine_open_from_bytes`], release with
/// [`pure_engine_free`].
pub struct PureEngine {
    corpus: Corpus,
    strongs: StrongsDict,
    search_ix: SearchIx,
    occ_ix: OccurrenceIx,
    /// The rendering lens: code → English renderings and surface word → codes,
    /// both corpus-derived and immutable after open (like `occ_ix`).
    renderings: Renderings,
    /// The data home, if opened from one — required to author (write) study
    /// data. `None` when opened from bytes (study data is then read-only/empty).
    home: Option<PathBuf>,
    /// Personal study data (margin notes, threads, tags, the weave graph),
    /// loaded from `home` and **reloaded after any authoring write** — so it
    /// sits behind an RwLock: the README promises `*const PureEngine` is safe
    /// to share across threads for reads, and a C# shell may author off its UI
    /// thread while another thread reads.
    study: std::sync::RwLock<StudyData>,
    /// R&D tier (loaded once at open; artifacts don't change on authoring):
    /// the fused OT↔NT bridge, concept embeddings, and morphology. Cheap loads;
    /// absent artifacts leave them empty/None.
    bridge: bridge::FusedBridge,
    embedding: Option<embed::Embedding>,
    morph: Option<morph::MorphData>,
    /// SIF "verses like this" — heavy to build (~a second over the whole
    /// corpus), so it is built lazily on the first similar-verses query and
    /// cached. `None` inside once the embedding is absent.
    verse_sim: OnceLock<Option<embed::VerseSim>>,
    /// TSK topical cross-references (loaded when opened from a home dir).
    xref_ix: XRefIx,
    /// The symbolic concept engine (collocations, distribution, communities)
    /// and the leitwort scan — corpus-wide sweeps, built lazily like the SIF
    /// model and cached for the engine's lifetime.
    concept: OnceLock<concept::Concept>,
    leitwort: OnceLock<std::collections::HashMap<String, burst::Burst>>,
}

impl PureEngine {
    fn new(corpus: Corpus, strongs: StrongsDict, home: Option<PathBuf>) -> PureEngine {
        let search_ix = SearchIx::build(&corpus);
        let occ_ix = OccurrenceIx::build(&corpus);
        let renderings = Renderings::build(&corpus);
        // R&D artifacts. The bridge's etymology layer works from the in-memory
        // dict even without a home; external witnesses + the embedding/morph
        // sidecars need a home's files. Without a home, no filesystem is
        // probed at all (a CWD-relative probe would be nondeterministic and a
        // mild data-injection surface).
        let bridge = match &home {
            Some(h) => bridge::FusedBridge::build(&strongs, h),
            None => bridge::FusedBridge::etymology_only(&strongs),
        };
        let (embedding, morph) = match &home {
            Some(h) => {
                let data = h.join("data");
                (
                    embed::load_embedding(canon::TOKENIZATION_VERSION, data.join("concept-vectors.vec")),
                    morph::load_morph(canon::TOKENIZATION_VERSION, data.join("morphology.jsonl")),
                )
            }
            None => (None, None),
        };
        let xref_ix = match &home {
            Some(h) => crossref::load_cross_refs(crossref::cross_refs_path(h)),
            None => XRefIx::new(),
        };
        let study = load_study(&home);
        // Margin notes never change after open (authoring writes touch
        // threads/tags/weaves only), so attach them to the search index once.
        let mut search_ix = search_ix;
        search_ix.attach_notes(&corpus, &study.notes);
        PureEngine {
            corpus,
            strongs,
            search_ix,
            occ_ix,
            renderings,
            home,
            study: std::sync::RwLock::new(study),
            bridge,
            embedding,
            morph,
            verse_sim: OnceLock::new(),
            xref_ix,
            concept: OnceLock::new(),
            leitwort: OnceLock::new(),
        }
    }

    /// Read the study state (poison-tolerant: every entry point is panic-
    /// firewalled, so recover the data rather than propagate the poison).
    fn study_read(&self) -> std::sync::RwLockReadGuard<'_, StudyData> {
        self.study.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Exclusive study state, for the authoring entry points.
    fn study_write(&self) -> std::sync::RwLockWriteGuard<'_, StudyData> {
        self.study.write().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The SIF model, built lazily on first use from the embedding + corpus.
    fn verse_sim(&self) -> Option<&embed::VerseSim> {
        self.verse_sim
            .get_or_init(|| self.embedding.as_ref().map(|e| embed::VerseSim::build(e, &self.corpus)))
            .as_ref()
    }

    /// The concept engine, built lazily on the first concept query.
    fn concept(&self) -> &concept::Concept {
        self.concept.get_or_init(|| concept::Concept::build(&self.corpus))
    }

    /// Leitwort discoveries keyed by Strong's code, built lazily alongside.
    fn leitwort(&self) -> &std::collections::HashMap<String, burst::Burst> {
        self.leitwort.get_or_init(|| {
            burst::discover_leitworter(&burst::BurstParams::default(), &self.corpus)
                .into_iter()
                .map(|b| (b.strongs.clone(), b))
                .collect()
        })
    }

}

/// The reloadable personal study state (see [`PureEngine::study`]).
#[derive(Default)]
struct StudyData {
    notes: Notes,
    threads: Vec<LoadedThread>,
    tags: Vec<LoadedTag>,
    weaves: Vec<LoadedWeave>,
    /// The reader's personal per-verse notes (Tier 0 #3), keyed by verse.
    user_notes: std::collections::HashMap<VRef, usernote::LoadedNote>,
}

/// Load notes + threads + tags + weaves + personal notes from `home` (empty
/// without one).
fn load_study(home: &Option<PathBuf>) -> StudyData {
    match home {
        Some(home) => StudyData {
            notes: notes::load_notes(home.join("data").join("kjv-notes.jsonl"))
                .unwrap_or_default(),
            threads: thread::load_threads(home).0,
            tags: tag::load_tags(home).0,
            weaves: weave::load_weaves(home).0,
            user_notes: usernote::load_notes(home).0,
        },
        None => StudyData::default(),
    }
}

// The ABI promises `*const PureEngine` is safe to share across threads for
// reads while authoring may happen on another thread — which is exactly
// `Send + Sync`. Fails to compile if a field ever loses that property.
fn _assert_engine_is_send_sync() {
    fn assert<T: Send + Sync>() {}
    assert::<PureEngine>();
}

/// One laid-out chapter: the positioned display list a shell paints and
/// hit-tests. Opaque to C; produced by [`pure_engine_layout_chapter`], released
/// with [`pure_layout_free`].
pub struct PureDisplayList {
    inner: DisplayList,
}

// ── layout config + measurement callback ──────────────────────────────────────

/// Layout parameters, all in device pixels — the C-ABI mirror of
/// `pure_layout::LayoutConfig` (passed by value, so it is `#[repr(C)]`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PureLayoutConfig {
    pub width: f32,
    pub line_height: f32,
    pub space_width: f32,
    pub verse_num_gap: f32,
    pub para_indent: f32,
    pub para_spacing: f32,
    /// Nonzero: start every verse on a fresh line (verse-per-line mode).
    pub verse_break: u32,
}

impl From<PureLayoutConfig> for LayoutConfig {
    fn from(c: PureLayoutConfig) -> LayoutConfig {
        LayoutConfig {
            width: c.width,
            line_height: c.line_height,
            space_width: c.space_width,
            verse_num_gap: c.verse_num_gap,
            para_indent: c.para_indent,
            para_spacing: c.para_spacing,
            verse_break: c.verse_break != 0,
        }
    }
}

/// Advance-width callback: given the caller's context pointer and a
/// NUL-terminated UTF-8 run of text, return its width in device pixels in the
/// reader's scripture font. The shell backs this with its native text stack so
/// the hit regions this crate computes line up exactly with painted glyphs.
///
/// Nullable (`Option<fn>`): a null callback makes layout return null.
///
/// # Contract — the callback MUST be total
/// It must **not** throw or panic across the boundary and should return a
/// **finite, non-negative** width. This crate's [`catch_unwind`] firewall only
/// catches *Rust* panics; a foreign exception unwinding out of this callback is
/// undefined behaviour — on .NET it fast-fails the process, on JNA it is
/// swallowed and reported as `0.0`. A returned `NaN`/negative is clamped to
/// `0.0` here (a degraded but safe layout) rather than corrupting line-breaking.
pub type PureMeasureFn = Option<extern "C" fn(ctx: *mut c_void, text: *const c_char) -> f32>;

/// Adapts a C measurement callback to the [`Measure`] trait the layout wants.
struct FfiMeasure {
    f: extern "C" fn(*mut c_void, *const c_char) -> f32,
    ctx: *mut c_void,
}

impl Measure for FfiMeasure {
    fn text_width(&self, text: &str) -> f32 {
        let w = match CString::new(text) {
            Ok(c) => (self.f)(self.ctx, c.as_ptr()),
            // Scripture text carries no interior NUL; if it ever did, a zero
            // width is a harmless degrade rather than a crash.
            Err(_) => 0.0,
        };
        // Defend line-breaking against a misbehaving callback: a NaN or negative
        // width would poison the pen arithmetic, so clamp to a safe 0.0.
        if w.is_finite() && w >= 0.0 {
            w
        } else {
            0.0
        }
    }
}

// ── small helpers ─────────────────────────────────────────────────────────────

/// Run `f`, turning any panic into `default` so nothing unwinds across the ABI.
fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(default)
}

/// Guard for the authoring calls, whose result string is null on success and an
/// owned error message otherwise. A panic surfaces as an error (not a false
/// success), and the error string is only allocated on the panic path.
fn guard_err(f: impl FnOnce() -> *mut c_char) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| out_string("internal error".to_string()))
}

/// Move an owned `String` out as a caller-freed C string. Returns null if the
/// string contains an interior NUL (never true for our JSON / ref keys).
fn out_string(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Serialize a wire DTO to a caller-freed JSON C string (null on the — for
/// these plain data types, impossible — serialization error).
fn out_json<T: serde::Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(s) => out_string(s),
        Err(_) => ptr::null_mut(),
    }
}

/// Borrow a C string param as `&str` for the call. Null / non-UTF-8 → `None`.
///
/// # Safety
/// `p` must be null or point to a valid NUL-terminated string that stays valid
/// for the duration of the call.
unsafe fn opt_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        CStr::from_ptr(p).to_str().ok()
    }
}

/// Write an owned error message through `out_err` if the caller supplied a slot.
///
/// # Safety
/// `out_err` must be null or a valid, writable `*mut *mut c_char`.
unsafe fn set_err(out_err: *mut *mut c_char, msg: String) {
    if !out_err.is_null() {
        *out_err = out_string(msg);
    }
}

// ── version probe (kept from the stub) ─────────────────────────────────────────

/// The pure-study core version as a caller-freed NUL-terminated UTF-8 string.
/// Never null.
#[no_mangle]
pub extern "C" fn pure_study_version() -> *mut c_char {
    out_string(env!("CARGO_PKG_VERSION").to_string())
}

/// Free a string previously returned by any `pure_*` function. Null is a no-op.
///
/// # Safety
/// `ptr` must be a pointer returned by this library and not already freed.
#[no_mangle]
pub unsafe extern "C" fn pure_study_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

// ── engine lifecycle ────────────────────────────────────────────────────────────

/// Open an engine from an overlay-style home directory containing
/// `data/kjv.jsonl` and `data/strongs.json`.
///
/// Returns null on failure; if `out_err` is non-null it receives a caller-freed
/// error message (and is set to null on success).
///
/// # Safety
/// `home` is a valid NUL-terminated UTF-8 path; `out_err` is null or a writable
/// slot for one `*mut c_char`.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_open(
    home: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut PureEngine {
    guard(ptr::null_mut(), || {
        if !out_err.is_null() {
            *out_err = ptr::null_mut();
        }
        let Some(home) = opt_str(home) else {
            set_err(out_err, "home path is null or not valid UTF-8".into());
            return ptr::null_mut();
        };
        let corpus = match corpus::load_corpus(format!("{home}/data/kjv.jsonl")) {
            Ok(c) => c,
            Err(e) => {
                set_err(out_err, e.to_string());
                return ptr::null_mut();
            }
        };
        let strongs = match strongs::load_strongs(format!("{home}/data/strongs.json")) {
            Ok(s) => s,
            Err(e) => {
                set_err(out_err, e.to_string());
                return ptr::null_mut();
            }
        };
        Box::into_raw(Box::new(PureEngine::new(corpus, strongs, Some(PathBuf::from(home)))))
    })
}

/// Open an engine from in-memory bytes — for shells that bundle the data as
/// assets/resources (decision #3): the `kjv.jsonl` text and the `strongs.json`
/// object, each as a length-delimited byte buffer (need not be NUL-terminated).
///
/// Returns null on failure; `out_err` behaves as in [`pure_engine_open`].
///
/// # Safety
/// Each `*_ptr`/`*_len` pair describes a readable buffer of that length (a null
/// pointer with length 0 is treated as empty and will error); `out_err` is null
/// or a writable slot.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_open_from_bytes(
    kjv_ptr: *const u8,
    kjv_len: usize,
    strongs_ptr: *const u8,
    strongs_len: usize,
    out_err: *mut *mut c_char,
) -> *mut PureEngine {
    guard(ptr::null_mut(), || {
        if !out_err.is_null() {
            *out_err = ptr::null_mut();
        }

        let read = |p: *const u8, n: usize| -> &[u8] {
            if p.is_null() || n == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(p, n)
            }
        };
        let kjv_bytes = read(kjv_ptr, kjv_len);
        let strongs_bytes = read(strongs_ptr, strongs_len);

        let kjv_str = match std::str::from_utf8(kjv_bytes) {
            Ok(s) => s,
            Err(e) => {
                set_err(out_err, format!("kjv bytes are not valid UTF-8: {e}"));
                return ptr::null_mut();
            }
        };
        let corpus = match corpus::from_str(kjv_str) {
            Ok(c) => c,
            Err(e) => {
                set_err(out_err, e.to_string());
                return ptr::null_mut();
            }
        };
        let strongs: StrongsDict = match serde_json::from_slice(strongs_bytes) {
            Ok(s) => s,
            Err(e) => {
                set_err(out_err, format!("could not parse strongs.json: {e}"));
                return ptr::null_mut();
            }
        };
        // No home when opened from bytes: study data is empty and read-only.
        Box::into_raw(Box::new(PureEngine::new(corpus, strongs, None)))
    })
}

/// Release an engine. Null is a no-op.
///
/// # Safety
/// `engine` must be a pointer from `pure_engine_open*` and not already freed.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_free(engine: *mut PureEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

// ── canon / corpus queries ──────────────────────────────────────────────────────

/// Table of contents as JSON: `{"books":[{"id","name","chapters"},...]}` in
/// canonical order. Chapter counts reflect the loaded corpus and are floored at
/// 1 for any book the corpus lacks (the full KJV corpus has all 66 books, so in
/// production the counts are exact). Caller-freed; null only on a null engine.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_toc_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(engine) = engine.as_ref() else {
            return ptr::null_mut();
        };
        let books: Vec<wire::TocBook> = canon::BOOKS
            .iter()
            .map(|b| wire::TocBook {
                id: b.id,
                name: b.name,
                chapters: engine.corpus.chapter_count(b.id),
            })
            .collect();
        out_json(&wire::Toc { books })
    })
}

/// Number of chapters the loaded corpus has for `book` (an OSIS id like
/// `"John"`). Floored at 1 for a book the corpus lacks (a safe UI range floor);
/// 0 only on a null engine.
///
/// # Safety
/// `engine` is valid; `book` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_chapter_count(
    engine: *const PureEngine,
    book: *const c_char,
) -> u32 {
    guard(0, || {
        match (engine.as_ref(), opt_str(book)) {
            (Some(engine), Some(book)) => engine.corpus.chapter_count(book) as u32,
            _ => 0,
        }
    })
}

/// A single verse as JSON:
/// `{"reference","display","body","title","tokens":[...]}`. `reference` is a
/// compact key like `"John 3:16"`. Null if the reference is unknown or
/// unparseable. Caller-freed.
///
/// # Safety
/// `engine` is valid; `ref_key` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_verse_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let Some(vref) = VRef::parse_ref_key(rk) else {
            return ptr::null_mut();
        };
        match engine.corpus.verse(&vref) {
            Some(v) => out_json(&wire::verse_to_wire(v)),
            None => ptr::null_mut(),
        }
    })
}

/// A single token as JSON: `{"pre","word","post","render","flags","strongs"}`.
/// Null if the reference or token index is out of range. Caller-freed.
///
/// # Safety
/// `engine` is valid; `ref_key` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_token_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
    token_index: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let Some(vref) = VRef::parse_ref_key(rk) else {
            return ptr::null_mut();
        };
        match engine
            .corpus
            .verse(&vref)
            .and_then(|v| v.tokens.get(token_index as usize))
        {
            Some(t) => out_json(&wire::token_to_wire(t)),
            None => ptr::null_mut(),
        }
    })
}

// ── layout + hit-testing ─────────────────────────────────────────────────────────

/// Lay out a chapter into a display list, measuring text through `measure`
/// (called with `measure_ctx`). Returns an opaque handle to release with
/// [`pure_layout_free`], or null on a null engine, a null callback, or an
/// unknown/out-of-range book+chapter (no such verses). Because the KJV has no
/// empty chapters, a null return reliably means "past the end" — a shell can
/// page by advancing until it gets null.
///
/// # Safety
/// `engine` is valid; `book` is a valid NUL-terminated UTF-8 string; `measure`
/// is a valid function pointer for the call and `measure_ctx` is whatever it
/// expects (it is passed back verbatim).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_layout_chapter(
    engine: *const PureEngine,
    book: *const c_char,
    chapter: u32,
    cfg: PureLayoutConfig,
    measure: PureMeasureFn,
    measure_ctx: *mut c_void,
) -> *mut PureDisplayList {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(book), Some(measure)) =
            (engine.as_ref(), opt_str(book), measure)
        else {
            return ptr::null_mut();
        };
        // The ABI takes u32 to match the bindings' `uint`; a value outside the
        // corpus's u16 chapter domain simply cannot exist (return null, don't
        // wrap it into a different real chapter).
        let Ok(chapter) = u16::try_from(chapter) else {
            return ptr::null_mut();
        };
        let verses = engine.corpus.chapter_verses(book, chapter);
        if verses.is_empty() {
            return ptr::null_mut();
        }
        let m = FfiMeasure { f: measure, ctx: measure_ctx };
        let dl = layout_chapter(verses, &m, &cfg.into());
        Box::into_raw(Box::new(PureDisplayList { inner: dl }))
    })
}

/// The full display list as JSON (see [`wire`] for the schema): positioned
/// items plus the total painted `width`/`height`. Caller-freed; null on a null
/// handle.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_layout_to_json(dl: *const PureDisplayList) -> *mut c_char {
    guard(ptr::null_mut(), || match dl.as_ref() {
        Some(dl) => out_json(&wire::display_list_to_wire(&dl.inner)),
        None => ptr::null_mut(),
    })
}

/// Total painted height in device pixels (scrollbar extent). 0 on a null handle.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_layout_height(dl: *const PureDisplayList) -> f32 {
    guard(0.0, || dl.as_ref().map(|d| d.inner.height).unwrap_or(0.0))
}

/// The column width the layout targeted. 0 on a null handle.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_layout_width(dl: *const PureDisplayList) -> f32 {
    guard(0.0, || dl.as_ref().map(|d| d.inner.width).unwrap_or(0.0))
}

/// Number of placed items in the display list. 0 on a null handle.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_layout_item_count(dl: *const PureDisplayList) -> u32 {
    guard(0, || dl.as_ref().map(|d| d.inner.items.len() as u32).unwrap_or(0))
}

/// Resolve a point (in the display list's own coordinate space) to the word
/// under it. Returns a `Hit` JSON (`{"verse","display","tokenIndex","strongs"}`)
/// or null when the point hits a verse number, a gap, or nothing. Caller-freed.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_layout_hit_test_json(
    dl: *const PureDisplayList,
    x: f32,
    y: f32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(dl) = dl.as_ref() else {
            return ptr::null_mut();
        };
        match dl.inner.hit_test(x, y) {
            Some(hit) => out_json(&wire::hit_to_wire(&hit)),
            None => ptr::null_mut(),
        }
    })
}

/// Release a display list. Null is a no-op.
///
/// # Safety
/// `dl` must be a pointer from [`pure_engine_layout_chapter`] and not already
/// freed.
#[no_mangle]
pub unsafe extern "C" fn pure_layout_free(dl: *mut PureDisplayList) {
    if !dl.is_null() {
        drop(Box::from_raw(dl));
    }
}

// ── Strong's + search ─────────────────────────────────────────────────────────

/// A Strong's entry as JSON: `{"code","lemma","xlit","pron","deriv","def",
/// "kjv"}` (unknown fields null). Null when the code is absent. Caller-freed.
///
/// # Safety
/// `engine` is valid; `code` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_strongs_json(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        match engine.strongs.get(code) {
            Some(e) => out_json(&wire::strongs_to_wire(code, e)),
            None => ptr::null_mut(),
        }
    })
}

/// The concordance for a Strong's code as JSON: `{"code","total","capped",
/// "verses":["Gen 1:1",...]}`. `verses` is capped at [`OCCURRENCE_CAP`] in
/// canonical order; `total` is the honest count and `capped` says whether the
/// list was truncated. Caller-freed; null on a null engine.
///
/// # Safety
/// `engine` is valid; `code` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_strongs_occurrences_json(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let all = engine.occ_ix.verses(code);
        let total = all.len();
        let verses: Vec<String> = all.iter().take(OCCURRENCE_CAP).map(|v| v.ref_key()).collect();
        out_json(&wire::Occurrences {
            code: code.to_string(),
            total,
            capped: total > verses.len(),
            verses,
        })
    })
}

/// The rendering lens for a Strong's code: every distinct English rendering of
/// it, most frequent first, each with an occurrence count and its (capped)
/// verse refs + token spans. `{"code","renderings":[{"rendering","total",
/// "capped","refs":[{"verse","display","span":[start,end]}]}]}`. Each `refs`
/// list is capped at [`OCCURRENCE_CAP`]; `renderings` is empty for an untagged
/// or unknown code. Null only on a null engine. Caller-freed.
///
/// # Safety
/// `engine` is valid; `code` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_renderings_json(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let renderings = engine
            .renderings
            .renderings(code)
            .into_iter()
            .map(|r| {
                let total = r.count;
                let refs: Vec<wire::WireRenderingRef> = r
                    .occs
                    .iter()
                    .take(OCCURRENCE_CAP)
                    .map(|o| wire::WireRenderingRef {
                        verse: o.vref.ref_key(),
                        display: o.vref.display(),
                        span: [o.span.0, o.span.1],
                    })
                    .collect();
                wire::WireRendering {
                    rendering: r.label.to_string(),
                    total,
                    capped: total > refs.len(),
                    refs,
                }
            })
            .collect();
        out_json(&wire::WireRenderings { code: code.to_string(), renderings })
    })
}

/// The reverse lens: the Strong's codes a surface English word translates,
/// most frequent first. `{"word","codes":[{"code","count"}]}`. Reveals where
/// one English word hides a Greek/Hebrew distinction ("love" ← agape and
/// phileo); `codes` is empty for an untagged word. Null only on a null engine.
/// Caller-freed.
///
/// # Safety
/// `engine` is valid; `word` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_word_codes_json(
    engine: *const PureEngine,
    word: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(word)) = (engine.as_ref(), opt_str(word)) else {
            return ptr::null_mut();
        };
        let codes = engine
            .renderings
            .word_codes(word)
            .into_iter()
            .map(|(code, count)| wire::WireWordCode { code: code.to_string(), count })
            .collect();
        out_json(&wire::WireWordCodes { word: word.to_string(), codes })
    })
}

/// Run a query through the multi-tier search and return a `SearchAnswer` JSON:
/// either `{"kind":"goto",...}` (the query was a reference) or
/// `{"kind":"hits","how","total","capped","hits":[...]}`. Null when the query is
/// blank or the engine is null. Caller-freed.
///
/// # Safety
/// `engine` is valid; `query` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_search_json(
    engine: *const PureEngine,
    query: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(query)) = (engine.as_ref(), opt_str(query)) else {
            return ptr::null_mut();
        };
        let study = engine.study_read();
        match search::run_search(&engine.corpus, &study.notes, &engine.search_ix, query) {
            Some(answer) => out_json(&wire::search_to_wire(&answer)),
            None => ptr::null_mut(),
        }
    })
}

// ── study data: read ─────────────────────────────────────────────────────────

/// The loaded threads as JSON: `{"threads":[{name,notes,created,entries:[…]}]}`.
/// Caller-freed; null on a null engine.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_threads_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::threads_to_wire(&e.study_read().threads)),
        None => ptr::null_mut(),
    })
}

/// The loaded tags as JSON: `{"tags":[{name,color,created,members:[…]}]}`.
/// Caller-freed; null on a null engine.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_tags_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::tags_to_wire(&e.study_read().tags)),
        None => ptr::null_mut(),
    })
}

/// A verse's weave cross-reference partners as JSON:
/// `{"verse","partners":[{"verse","display","weave"}]}`. Caller-freed; null on a
/// null engine or an unparseable reference.
///
/// # Safety
/// `engine` is valid; `ref_key` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_verse_xrefs_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        match VRef::parse_ref_key(rk) {
            Some(vref) => out_json(&wire::verse_xrefs_to_wire(&e.study_read().weaves, &vref)),
            None => ptr::null_mut(),
        }
    })
}

/// The suggested weaves (proposals under `home/weaves/suggested`) awaiting
/// review, as JSON: `{"suggested":[{index,name,kind,notes,links:[…]}]}`. Each
/// item's `index` is the ordinal within the suggested subset — the handle the
/// approve/reject calls take. Caller-freed; null on a null engine.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_suggested_weaves_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::suggested_weaves_to_wire(&e.study_read().weaves)),
        None => ptr::null_mut(),
    })
}

// ── R&D tier: read (concept embeddings, morphology, fused bridge) ────────────────
//
// These consume the offline artifacts loaded at open (see `data-prep`). Each
// returns null when its artifact is absent (or the engine/ref is invalid), so a
// shell shows the section exactly when it exists — no training happens here.

/// Concept neighbours of a Strong's code as JSON:
/// `{"code","near":[{code,score}],"cross":[{code,score}]}` (same-testament, then
/// cross-testament — the latter empty unless the embedding is aligned). Null
/// when no embedding is loaded or the args are invalid.
///
/// # Safety
/// `engine` is valid; `code` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_concept_neighbours_json(
    engine: *const PureEngine,
    code: *const c_char,
    k: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let Some(emb) = &e.embedding else { return ptr::null_mut() };
        let k = k as usize;
        out_json(&wire::WireConceptNeighbours {
            code: code.to_string(),
            near: wire::scored_to_wire(emb.nearest_concepts(code, k)),
            cross: wire::scored_to_wire(emb.cross_concepts(code, k)),
        })
    })
}

/// The fused OT↔NT bridge partners of a Strong's code as JSON:
/// `{"code","partners":[{code,sources,prior}]}`, ranked by trust prior. The
/// etymology layer works from the dictionary alone, so this is available even
/// for a bytes-opened engine (external witnesses need a home). Null on a null
/// engine / invalid code.
///
/// # Safety
/// `engine` is valid; `code` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_bridge_partners_json(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let partners = e
            .bridge
            .partners(code)
            .into_iter()
            .map(|p| {
                // Authority provenance, classified once here (overlay `Tier`):
                // the additive tier set + research-grade flag travel with each
                // partner so non-Rust shells need not reimplement the mapping.
                let tiers = bridge::tiers_of(&p.sources)
                    .into_iter()
                    .map(|t| t.wire_name().to_string())
                    .collect();
                let research_grade = p.sources.iter().any(|s| bridge::research_grade(s));
                wire::WireBridgePartner {
                    code: p.code,
                    sources: p.sources,
                    prior: p.prior,
                    tiers,
                    research_grade,
                }
            })
            .collect();
        out_json(&wire::WireBridgePartners { code: code.to_string(), partners })
    })
}

/// The morphology of one token as JSON:
/// `{"verse","tokenIndex","code","gloss"}`. Null when no morphology is loaded,
/// the reference is unparseable, or that token carries no annotation.
///
/// # Safety
/// `engine` is valid; `ref_key` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_morph_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
    token_index: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let (Some(md), Some(vref)) = (&e.morph, VRef::parse_ref_key(rk)) else {
            return ptr::null_mut();
        };
        let Some(entry) = md.entries(&vref).iter().find(|en| en.tok == token_index) else {
            return ptr::null_mut();
        };
        let Some(gloss) = md.gloss(&vref, token_index) else { return ptr::null_mut() };
        out_json(&wire::WireMorph {
            verse: vref.ref_key(),
            token_index,
            code: entry.code.clone(),
            gloss,
        })
    })
}

/// "Verses like this one" (SIF) as JSON:
/// `{"verse","in":[{verse,display,score}],"cross":[…]}`. The SIF model is built
/// lazily on the first call (heavy) and cached. Null when no embedding is
/// loaded or the reference is unparseable.
///
/// # Safety
/// `engine` is valid; `ref_key` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_similar_verses_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
    k: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let (Some(vs), Some(vref)) = (e.verse_sim(), VRef::parse_ref_key(rk)) else {
            return ptr::null_mut();
        };
        let k = k as usize;
        out_json(&wire::WireSimilarVerses {
            verse: vref.ref_key(),
            within: wire::similar_to_wire(vs.similar_verses_in(&vref, k)),
            cross: wire::similar_to_wire(vs.similar_verses_cross(&vref, k)),
        })
    })
}

// ── study data: authoring (write) ──────────────────────────────────────────────
//
// These mutate on-disk study data through the cross-platform `core::store`
// atomic writer, then reload the engine's in-memory copies. Each returns **null
// on success** and an owned error string on failure (free it with
// `pure_study_string_free`). All require an engine opened from a home directory
// (`pure_engine_open`); an engine opened from bytes returns an error.

/// Add the whole verse `ref_key` to the thread named `name` (created on first
/// use). `note` may be null; `added` is a caller-supplied UTC timestamp.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_thread_add(
    engine: *mut PureEngine,
    name: *const c_char,
    ref_key: *const c_char,
    note: *const c_char,
    added: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(name), Some(rk), Some(added)) = (opt_str(name), opt_str(ref_key), opt_str(added))
        else {
            return out_string("null or invalid argument".to_string());
        };
        let Some(vref) = VRef::parse_ref_key(rk) else {
            return out_string(format!("bad ref: {rk}"));
        };
        let (span, text) = match engine.corpus.verse(&vref) {
            Some(v) => {
                let words: Vec<String> = v.tokens.iter().map(|t| t.word.clone()).collect();
                ((0u16, words.len().saturating_sub(1) as u16), words)
            }
            None => ((0, 0), Vec::new()),
        };
        let entry = ThreadEntry {
            vref,
            span,
            text,
            note: opt_str(note).map(str::to_string),
            added: added.to_string(),
        };
        let mut study = engine.study_write();
        match thread::add_to_thread(&home, &study.threads, name, canon::TOKENIZATION_VERSION, entry) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Add a target to the tag named `name` (created on first use). `kind` is
/// `"verse"` (with `value` a ref key) or `"concept"` (with `value` a Strong's
/// code). `note` may be null; `added` is a caller-supplied UTC timestamp.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_tag_add(
    engine: *mut PureEngine,
    name: *const c_char,
    kind: *const c_char,
    value: *const c_char,
    note: *const c_char,
    added: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(name), Some(added)) = (opt_str(name), opt_str(added)) else {
            return out_string("null or invalid argument".to_string());
        };
        let target = match parse_target(kind, value) {
            Ok(t) => t,
            Err(e) => return out_string(e),
        };
        let mut study = engine.study_write();
        match tag::add_member(
            &home,
            &study.tags,
            name,
            canon::TOKENIZATION_VERSION,
            target,
            opt_str(note).map(str::to_string),
            added,
        ) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Remove a target (see [`pure_engine_tag_add`] for `kind`/`value`) from the tag
/// named `name`. A missing target is a no-op; a missing tag is an error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_tag_remove(
    engine: *mut PureEngine,
    name: *const c_char,
    kind: *const c_char,
    value: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(name) = opt_str(name) else {
            return out_string("null name".to_string());
        };
        let target = match parse_target(kind, value) {
            Ok(t) => t,
            Err(e) => return out_string(e),
        };
        let wanted = name.trim().to_lowercase();
        let mut study = engine.study_write();
        let found = study.tags.iter().find(|lt| lt.tag.name.to_lowercase() == wanted).cloned();
        match found {
            Some(lt) => match tag::remove_member(&lt, &target) {
                Ok(()) => {
                    *study = load_study(&engine.home);
                    ptr::null_mut()
                }
                Err(e) => out_string(e.to_string()),
            },
            None => out_string(format!("no tag named {name}")),
        }
    })
}

/// Weave the two whole verses `a_ref` / `b_ref` into the weave named `name`
/// (created on first use). `added` is a caller-supplied UTC timestamp.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_weave_add_link(
    engine: *mut PureEngine,
    name: *const c_char,
    a_ref: *const c_char,
    b_ref: *const c_char,
    added: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(name), Some(a), Some(b), Some(added)) =
            (opt_str(name), opt_str(a_ref), opt_str(b_ref), opt_str(added))
        else {
            return out_string("null or invalid argument".to_string());
        };
        let (Some(av), Some(bv)) = (VRef::parse_ref_key(a), VRef::parse_ref_key(b)) else {
            return out_string("bad ref".to_string());
        };
        let mut study = engine.study_write();
        match weave::add_link(
            &home,
            &study.weaves,
            name,
            WeaveKind::Quotation,
            canon::TOKENIZATION_VERSION,
            added,
            Link::canon(av, bv),
        ) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// The `index`-th suggested weave (as ordered by
/// `pure_engine_suggested_weaves_json`) into the engine's flat weave list.
fn nth_suggested(weaves: &[LoadedWeave], index: usize) -> Option<usize> {
    weaves
        .iter()
        .enumerate()
        .filter(|(_, lw)| weave::is_suggested(lw))
        .nth(index)
        .map(|(i, _)| i)
}

/// **Approve** the `index`-th suggested weave: promote it into `home/weaves`
/// with all links approved (merging into a same-named weave there if present)
/// and remove the suggestion. `index` is the ordinal from
/// `pure_engine_suggested_weaves_json`. Null on success, else an owned error.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_weave_approve(engine: *mut PureEngine, index: u32) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let mut study = engine.study_write();
        let Some(i) = nth_suggested(&study.weaves, index as usize) else {
            return out_string(format!("no suggested weave at index {index}"));
        };
        match weave::approve_weave(&home, &study.weaves[i]) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// **Reject** the `index`-th suggested weave: delete its file. `index` is the
/// ordinal from `pure_engine_suggested_weaves_json`. Null on success, else an
/// owned error.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_weave_reject(engine: *mut PureEngine, index: u32) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let mut study = engine.study_write();
        let Some(i) = nth_suggested(&study.weaves, index as usize) else {
            return out_string(format!("no suggested weave at index {index}"));
        };
        match weave::reject_weave(&study.weaves[i]) {
            Ok(()) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Replace the running notes document of the thread named `name`. Null on
/// success, else an owned error. The thread must already exist.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_thread_set_notes(
    engine: *mut PureEngine,
    name: *const c_char,
    notes: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let (Some(name), Some(notes)) = (opt_str(name), opt_str(notes)) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match thread::set_thread_notes(&study.threads, name, notes) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Set (or clear, with a null `note`) the note on entry `index` of the thread
/// named `name`. Null on success, else an owned error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_thread_entry_set_note(
    engine: *mut PureEngine,
    name: *const c_char,
    index: u32,
    note: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let Some(name) = opt_str(name) else {
            return out_string("null or invalid name".to_string());
        };
        let mut study = engine.study_write();
        match thread::set_entry_note(&study.threads, name, index as usize, opt_str(note).map(str::to_string)) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Replace the notes document of the weave named `name` (marks it hand-written).
/// Null on success, else an owned error. The weave must already exist.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_weave_set_notes(
    engine: *mut PureEngine,
    name: *const c_char,
    notes: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let (Some(name), Some(notes)) = (opt_str(name), opt_str(notes)) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match weave::set_weave_notes(&study.weaves, name, notes) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

// ── shell-parity endpoints (margin notes, TSK, weave library, concept, gloss,
//    span links, config) — see docs/FEATURE-MANIFEST.md ─────────────────────────

/// A verse's 1769 translators' margin notes as JSON, or null when the verse
/// has none (or the engine was opened from bytes).
///
/// # Safety
/// `engine` is a live engine; `ref_key` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_verse_notes_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let Some(vref) = VRef::parse_ref_key(rk) else {
            return ptr::null_mut();
        };
        let study = e.study_read();
        match study.notes.get(&vref) {
            Some(ns) if !ns.is_empty() => out_json(&wire::WireVerseNotes {
                verse: vref.ref_key(),
                notes: ns.clone(),
            }),
            _ => ptr::null_mut(),
        }
    })
}

/// The verse's TSK study cross-references (best-voted first) as JSON, or null
/// when the verse has none or the TSK artifact is absent.
///
/// # Safety
/// `engine` is a live engine; `ref_key` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_study_xrefs_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let Some(vref) = VRef::parse_ref_key(rk) else {
            return ptr::null_mut();
        };
        match e.xref_ix.get(&vref) {
            Some(rs) if !rs.is_empty() => out_json(&wire::study_xrefs_to_wire(&vref, rs)),
            _ => ptr::null_mut(),
        }
    })
}

/// The full weave library (canonical + suggested) with every link field a
/// shell needs for connectors, compare cards, the chord map and the
/// constellation. Never null on a live engine (empty library → empty list).
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_weaves_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::weaves_to_wire(&e.study_read().weaves, &e.corpus)),
        None => ptr::null_mut(),
    })
}

/// Every weave link as a deduped canonical verse pair, each endpoint located
/// (ref key + book/chapter/verse) and flagged `resolved` when both ends are in
/// the corpus. The one derivation behind the ambient connector lines and the
/// chord map, so a shell neither dedupes nor parses ref keys itself. Never null
/// on a live engine (empty library → empty list).
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_link_pairs_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::link_pairs_to_wire(&e.study_read().weaves, &e.corpus)),
        None => ptr::null_mut(),
    })
}

/// The canon overview segmentation: the 8 sections as `(label, first, last)`
/// book indices over the 66 books, plus the OT/NT divide (39). Static data
/// frozen in `core::reference` — served here so a non-Rust shell consumes the
/// one source instead of re-hardcoding the bands. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null); the payload does not depend on
/// engine state, but the arg keeps the call shape uniform.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_canon_segments_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(_) => out_json(&wire::canon_segments_to_wire()),
        None => ptr::null_mut(),
    })
}

/// The book-to-book weave chord map: canon-ordered book-pair counts over the
/// deduped link pairs (`{pairs:[{a,b,count}], max, otNtDivide, bookCount}`),
/// where `a`/`b` are book indices (`a <= b`). The one fold behind the "Weave
/// map" popup, so a shell lays out ribbons without folding pairs or deriving the
/// max. Never null on a live engine (empty library → empty pairs, max 1).
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_chord_map_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::chord_map_to_wire(&e.study_read().weaves)),
        None => ptr::null_mut(),
    })
}

/// One laid-out page of the constellation (the weave-library overview popup):
/// lanes with nodes + edges as **fractions**, plus the pin/paging state already
/// resolved into a caption. The shell holds the transient `page` and `pins`
/// (weave indices, the same handles the lanes carry) and passes them in;
/// everything derived — usable filter, largest-first order, per-verse degree,
/// jitter, lane assignment, paging — lives here. `pins_json` is a JSON array of
/// weave indices (e.g. `"[3,7]"`); null / empty / malformed means no pins.
/// Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null); `pins_json` is null or valid
/// NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_constellation_json(
    engine: *const PureEngine,
    page: u32,
    pins_json: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let pins: Vec<usize> =
            opt_str(pins_json).and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        out_json(&wire::constellation_to_wire(
            &e.study_read().weaves,
            &e.corpus,
            page as usize,
            &pins,
        ))
    })
}

/// The symbolic concept engine's view of a Strong's code: occurrence total,
/// testament split, concentrating books, per-book dispersion counts,
/// collocates, co-occurrence community, and the leitwort discovery when one
/// exists. Null when the code never occurs. Built lazily on first call
/// (corpus-wide sweep, ~seconds) and cached.
///
/// # Safety
/// `engine` is a live engine; `code` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_concept_json(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let ce = e.concept();
        let Some(stat) = ce.stat(code) else {
            return ptr::null_mut();
        };
        let (ot, nt) = ce.testament_split(code);
        let leitwort = e.leitwort().get(code).map(|b| wire::WireLeitwort {
            n: b.n,
            win_count: b.win_count,
            score: b.score,
            label: burst::span_label(
                |id| canon::display_name(id).to_string(),
                &b.win_start,
                &b.win_end,
            ),
        });
        out_json(&wire::WireConcept {
            code: code.to_string(),
            total: stat.total,
            ot,
            nt,
            top_books: ce
                .top_books(code, 5)
                .into_iter()
                .map(|(book, count)| wire::WireBookCount {
                    display: canon::display_name(&book).to_string(),
                    book,
                    count,
                })
                .collect(),
            by_book: stat.by_book.clone(),
            collocates: wire::scored_to_wire(ce.collocates(code, 12)),
            community: ce.community(code).into_iter().take(12).collect(),
            leitwort,
        })
    })
}

/// How many spokes each side (semantic / community) of the concept map shows.
const CONCEPT_MAP_SPOKES: usize = 6;

/// A concept-map node label: the English gloss over the lemma (`\n`-separated),
/// falling back to whichever exists, then the bare code. Mirrors the GTK
/// `label_of` closure so every shell labels the radial nodes identically.
fn concept_label(e: &PureEngine, code: &str) -> String {
    let gloss = english_gloss(e, code);
    let lemma = e.strongs.get(code).and_then(|s| s.lemma.clone());
    match (gloss, lemma) {
        (Some(g), Some(l)) => format!("{g}\n{l}"),
        (Some(g), None) => g,
        (None, Some(l)) => l,
        (None, None) => code.to_string(),
    }
}

/// The concept map for a code: the radial neighbourhood (embedding neighbours ∪
/// collocation community, deduped, labels pre-baked) plus the per-book
/// dispersion counts in canon order. One call replaces the shell's spoke
/// assembly and its four separate lookups (neighbours / concept / gloss /
/// lemma). Never null on a live engine + valid code — a code with no stats
/// still yields its centre label and any embedding spokes (empty dispersion).
///
/// # Safety
/// `engine` is a live engine; `code` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_concept_map_json(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        // Semantic neighbours (gold) — empty without an embedding artifact.
        let near: Vec<String> = e
            .embedding
            .as_ref()
            .map(|emb| {
                emb.nearest_concepts(code, CONCEPT_MAP_SPOKES)
                    .into_iter()
                    .map(|(c, _)| c)
                    .collect()
            })
            .unwrap_or_default();
        let ce = e.concept();
        let community = ce.community(code);
        let spokes = concept::radial_spokes(&near, &community, CONCEPT_MAP_SPOKES)
            .into_iter()
            .map(|(c, semantic)| wire::WireConceptSpoke {
                label: concept_label(e, &c),
                code: c,
                semantic,
            })
            .collect();
        // Dispersion in canon order (0 where the concept never occurs).
        let by_book = ce
            .stat(code)
            .map(|s| canon::BOOKS.iter().map(|b| s.by_book.get(b.id).copied().unwrap_or(0)).collect())
            .unwrap_or_else(|| vec![0; canon::BOOKS.len()]);
        // Cross-testament bridge row: the strongest other-testament partners and
        // their unioned dispersion (so Christ reveals where Messiah occurs).
        let partners = e.bridge.partners(code);
        let bridge = (!partners.is_empty()).then(|| {
            let top: Vec<&bridge::Partner> =
                partners.iter().take(concept::BRIDGE_ROW_PARTNERS).collect();
            let union = ce.union_by_book(top.iter().map(|p| p.code.as_str()));
            wire::WireConceptBridge {
                partners: top
                    .iter()
                    .map(|p| wire::WireBridgeNode {
                        label: concept_label(e, &p.code),
                        code: p.code.clone(),
                        prior: p.prior,
                    })
                    .collect(),
                by_book: canon::BOOKS
                    .iter()
                    .map(|b| union.get(b.id).copied().unwrap_or(0))
                    .collect(),
            }
        });
        out_json(&wire::WireConceptMap {
            center_label: concept_label(e, code),
            code: code.to_string(),
            spokes,
            by_book,
            ot_nt_divide: pure_core::reference::OT_NT_DIVIDE,
            book_count: canon::BOOKS.len(),
            bridge,
        })
    })
}

/// A short English gloss for a Strong's code — the modal KJV rendering across
/// its occurrences (≤80 sampled), falling back to a distilled dictionary
/// clause. Plain text (not JSON); null when nothing sensible exists.
///
/// # Safety
/// `engine` is a live engine; `code` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_gloss(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        match english_gloss(e, code) {
            Some(g) => out_string(g),
            None => ptr::null_mut(),
        }
    })
}

// ── the study-panel content model ─────────────────────────────────────────────
//
// One Rust producer (`pure_core::panel`) builds the typed block list for every
// panel view; this projects the engine's data into it and serves the blocks as
// JSON. `full` (Full study vs simple reader) is a shell setting, so the endpoints
// that gate on it take a `full` flag — the FFI itself is mode-agnostic.

/// How many concordance verses a panel card lists before an "… N more" tail
/// (matches the shells' prior cap).
const PANEL_OCC_CAP: usize = 300;

impl PanelSource for PureEngine {
    fn token_word(&self, verse: &str, token: u32) -> Option<String> {
        let v = VRef::parse_ref_key(verse)?;
        self.corpus.verse(&v)?.tokens.get(token as usize).map(|t| t.word.clone())
    }
    fn verse_display(&self, refkey: &str) -> Option<String> {
        VRef::parse_ref_key(refkey).map(|v| v.display())
    }
    fn morph_gloss(&self, verse: &str, token: u32) -> Option<String> {
        let (md, v) = (self.morph.as_ref()?, VRef::parse_ref_key(verse)?);
        md.gloss(&v, token)
    }
    fn occurrence_count(&self, code: &str) -> usize {
        self.occ_ix.verses(code).len()
    }
    fn strongs(&self, code: &str) -> Option<panel::StrongsView> {
        let e = self.strongs.get(code)?;
        Some(panel::StrongsView {
            lemma: e.lemma.clone(),
            xlit: e.xlit.clone(),
            pron: e.pron.clone(),
            deriv: e.deriv.clone(),
            def: e.def.clone(),
            kjv: e.kjv.clone(),
        })
    }
    fn gloss(&self, code: &str) -> Option<String> {
        english_gloss(self, code)
    }
    fn chip(&self, code: &str) -> panel::ChipView {
        panel::ChipView {
            code: code.to_string(),
            gloss: english_gloss(self, code),
            lemma: self.strongs.get(code).and_then(|e| e.lemma.clone()),
        }
    }
    fn renderings(&self, code: &str) -> Vec<panel::RenderingView> {
        self.renderings
            .renderings(code)
            .into_iter()
            .map(|r| panel::RenderingView { rendering: r.label.to_string(), total: r.count as u32 })
            .collect()
    }
    fn rendering_refs(&self, code: &str, rendering: &str) -> Option<panel::RenderingRefsView> {
        let key = pure_core::renderings::normalize(rendering);
        let r = self
            .renderings
            .renderings(code)
            .into_iter()
            .find(|r| pure_core::renderings::normalize(r.label) == key)?;
        Some(panel::RenderingRefsView {
            rendering: r.label.to_string(),
            total: r.count as u32,
            refs: r.occs.iter().take(PANEL_OCC_CAP).map(|o| (o.vref.ref_key(), o.vref.display())).collect(),
        })
    }
    fn word_codes(&self, word: &str) -> Vec<String> {
        self.renderings.word_codes(word).into_iter().map(|(c, _)| c.to_string()).collect()
    }
    fn occurrences(&self, code: &str) -> panel::OccurrencesView {
        let all = self.occ_ix.verses(code);
        panel::OccurrencesView {
            total: all.len() as u32,
            verses: all.iter().take(PANEL_OCC_CAP).map(|v| (v.ref_key(), v.display())).collect(),
        }
    }
    fn bridge_partners(&self, code: &str) -> Vec<panel::BridgePartnerView> {
        self.bridge
            .partners(code)
            .into_iter()
            .map(|p| panel::BridgePartnerView {
                sources: p.sources.iter().map(|s| bridge::source_label(s).to_string()).collect(),
                tiers: bridge::tiers_of(&p.sources).into_iter().map(|t| t.wire_name().to_string()).collect(),
                research_grade: p.sources.iter().any(|s| bridge::research_grade(s)),
                code: p.code,
            })
            .collect()
    }
    fn concept_near(&self, code: &str, k: usize) -> (Vec<String>, Vec<String>) {
        match &self.embedding {
            Some(emb) => (
                emb.nearest_concepts(code, k).into_iter().map(|(c, _)| c).collect(),
                emb.cross_concepts(code, k).into_iter().map(|(c, _)| c).collect(),
            ),
            None => (Vec::new(), Vec::new()),
        }
    }
    fn concept(&self, code: &str) -> Option<panel::ConceptView> {
        let ce = self.concept();
        ce.stat(code)?;
        let (ot, nt) = ce.testament_split(code);
        let leitwort = self.leitwort().get(code).map(|b| panel::LeitwortView {
            n: b.n,
            win_count: b.win_count,
            score: b.score,
            label: burst::span_label(|id| canon::display_name(id).to_string(), &b.win_start, &b.win_end),
        });
        Some(panel::ConceptView {
            community: ce.community(code),
            top_books: ce.top_books(code, 5).into_iter().map(|(b, n)| (canon::display_name(&b).to_string(), n)).collect(),
            ot,
            nt,
            leitwort,
        })
    }
    fn verse_xrefs(&self, verse: &str) -> Vec<panel::XrefView> {
        let Some(v) = VRef::parse_ref_key(verse) else { return Vec::new() };
        let study = self.study_read();
        wire::verse_xrefs_to_wire(&study.weaves, &v)
            .partners
            .into_iter()
            .map(|p| {
                let weave_index = study.weaves.iter().position(|lw| lw.weave.name == p.weave);
                panel::XrefView { verse: p.verse, display: p.display, weave: p.weave, weave_index }
            })
            .collect()
    }
    fn study_xrefs(&self, verse: &str) -> Vec<panel::StudyXrefView> {
        let Some(v) = VRef::parse_ref_key(verse) else { return Vec::new() };
        match self.xref_ix.get(&v) {
            Some(rs) => rs
                .iter()
                .map(|r| panel::StudyXrefView {
                    to: r.to.ref_key(),
                    to_display: r.to.display(),
                    end: r.end.as_ref().map(|e| e.ref_key()),
                    end_display: r.end.as_ref().map(|e| e.display()),
                })
                .collect(),
            None => Vec::new(),
        }
    }
    fn similar_verses(&self, verse: &str, k: usize) -> (Vec<panel::SimilarView>, Vec<panel::SimilarView>) {
        let (Some(vs), Some(v)) = (self.verse_sim(), VRef::parse_ref_key(verse)) else {
            return (Vec::new(), Vec::new());
        };
        let map = |items: Vec<(VRef, f32)>| {
            items.into_iter().map(|(r, _)| panel::SimilarView { verse: r.ref_key(), display: r.display() }).collect()
        };
        (map(vs.similar_verses_in(&v, k)), map(vs.similar_verses_cross(&v, k)))
    }
    fn verse_tags(&self, verse: &str) -> Vec<(usize, String)> {
        self.study_read()
            .tags
            .iter()
            .enumerate()
            .filter_map(|(i, lt)| {
                let holds = lt
                    .tag
                    .members
                    .iter()
                    .any(|m| matches!(&m.target, pure_core::tag::TagTarget::Verse(v) if v.ref_key() == verse));
                holds.then(|| (i, lt.tag.name.clone()))
            })
            .collect()
    }
    fn verse_notes(&self, verse: &str) -> Vec<String> {
        let Some(v) = VRef::parse_ref_key(verse) else { return Vec::new() };
        self.study_read().notes.get(&v).cloned().unwrap_or_default()
    }
    fn user_note(&self, verse: &str) -> Option<String> {
        let v = VRef::parse_ref_key(verse)?;
        self.study_read().user_notes.get(&v).map(|ln| ln.note.text.clone())
    }
    fn threads(&self) -> Vec<panel::ThreadView> {
        self.study_read()
            .threads
            .iter()
            .map(|lt| panel::ThreadView {
                name: lt.thread.name.clone(),
                notes: lt.thread.notes.clone(),
                entries: lt
                    .thread
                    .entries
                    .iter()
                    .map(|e| panel::ThreadEntryView {
                        verse: e.vref.ref_key(),
                        display: e.vref.display(),
                        text: e.text.clone(),
                        note: e.note.clone(),
                    })
                    .collect(),
            })
            .collect()
    }
    fn tags(&self) -> Vec<panel::TagView> {
        self.study_read()
            .tags
            .iter()
            .map(|lt| panel::TagView {
                name: lt.tag.name.clone(),
                members: lt
                    .tag
                    .members
                    .iter()
                    .map(|m| match &m.target {
                        pure_core::tag::TagTarget::Verse(v) => panel::TagMemberView {
                            kind: "verse".into(),
                            verse: Some(v.ref_key()),
                            display: Some(v.display()),
                            strongs: None,
                            note: m.note.clone(),
                        },
                        pure_core::tag::TagTarget::Concept(c) => panel::TagMemberView {
                            kind: "concept".into(),
                            verse: None,
                            display: None,
                            strongs: Some(c.clone()),
                            note: m.note.clone(),
                        },
                    })
                    .collect(),
            })
            .collect()
    }
    fn weaves(&self) -> Vec<panel::WeaveView> {
        self.study_read()
            .weaves
            .iter()
            .enumerate()
            .map(|(index, lw)| panel::WeaveView {
                index,
                name: lw.weave.name.clone(),
                kind_label: lw.weave.kind.label().to_string(),
                notes: lw.weave.notes.clone(),
                suggested: pure_core::weave::is_suggested(lw),
                links: lw
                    .weave
                    .links
                    .iter()
                    .map(|l| panel::WeaveLinkView {
                        a: l.a.ref_key(),
                        a_display: l.a.display(),
                        b: l.b.ref_key(),
                        b_display: l.b.display(),
                        label: l.label.clone(),
                        span_a: l.span_a.map(|(lo, hi)| [lo, hi]),
                        span_b: l.span_b.map(|(lo, hi)| [lo, hi]),
                    })
                    .collect(),
            })
            .collect()
    }
    fn suggested(&self) -> Vec<panel::SuggestedView> {
        let study = self.study_read();
        study
            .weaves
            .iter()
            .filter(|lw| pure_core::weave::is_suggested(lw))
            .enumerate()
            .map(|(index, lw)| {
                let lib_index = study
                    .weaves
                    .iter()
                    .position(|x| pure_core::weave::is_suggested(x) && x.weave.name == lw.weave.name);
                panel::SuggestedView {
                    index,
                    name: lw.weave.name.clone(),
                    kind: lw.weave.kind.token().to_string(),
                    notes: lw.weave.notes.clone(),
                    lib_index,
                    links: lw
                        .weave
                        .links
                        .iter()
                        .map(|l| panel::SuggestedLinkView {
                            a: l.a.ref_key(),
                            a_display: l.a.display(),
                            b: l.b.ref_key(),
                            b_display: l.b.display(),
                            label: l.label.clone(),
                        })
                        .collect(),
                }
            })
            .collect()
    }
    fn verse_tokens(&self, refkey: &str) -> Option<panel::VerseTokensView> {
        let v = VRef::parse_ref_key(refkey)?;
        let verse = self.corpus.verse(&v)?;
        Some(panel::VerseTokensView {
            tokens: verse
                .tokens
                .iter()
                .map(|t| panel::TokenView { render: t.render(), added: t.has_flag(corpus::FLAG_ADDED) })
                .collect(),
        })
    }
    fn verse_body(&self, refkey: &str) -> Option<String> {
        let v = VRef::parse_ref_key(refkey)?;
        self.corpus.verse(&v).map(|verse| verse.body())
    }
    fn search(&self, query: &str) -> panel::SearchView {
        let study = self.study_read();
        match search::run_search(&self.corpus, &study.notes, &self.search_ix, query) {
            Some(search::SearchAnswer::GoTo { book, chapter, verse }) => {
                let display = match verse {
                    Some(v) => VRef::new(book.clone(), chapter, v).display(),
                    None => format!("{} {}", canon::display_name(&book), chapter),
                };
                panel::SearchView::Goto { book, chapter: chapter as u32, verse: verse.map(u32::from), display }
            }
            Some(search::SearchAnswer::Hits { how, total, hits }) => {
                let capped = total > hits.len();
                panel::SearchView::Hits {
                    how,
                    total,
                    capped,
                    hits: hits
                        .into_iter()
                        .map(|h| panel::SearchHitView { verse: h.vref.ref_key(), display: h.vref.display(), note: h.note, why: h.why })
                        .collect(),
                }
            }
            None => panel::SearchView::Hits { how: String::new(), total: 0, capped: false, hits: Vec::new() },
        }
    }
}

/// A panel view as the typed block list (`{blocks:[…]}`). Word study: the clicked
/// word's dictionary + Full-study tiers + this verse's cross-references/notes.
/// `full` gates the R&D tiers + author actions. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; `ref_key` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_word_study_blocks_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
    token_index: u32,
    full: bool,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let codes: Vec<String> = VRef::parse_ref_key(rk)
            .and_then(|v| e.corpus.verse(&v).and_then(|verse| verse.tokens.get(token_index as usize).cloned()))
            .map(|t| t.strongs)
            .unwrap_or_default();
        out_json(&wire::blocks_to_wire(panel::word_study(e, full, rk, token_index, &codes)))
    })
}

/// The standalone `code:CODE[:word]` study card (the reverse rendering-lens
/// target). `word` may be null. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_code_study_blocks_json(
    engine: *const PureEngine,
    code: *const c_char,
    word: *const c_char,
    full: bool,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        out_json(&wire::blocks_to_wire(panel::code_study_card(e, full, code, opt_str(word).unwrap_or(""))))
    })
}

/// The full concordance for a code as blocks. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; `code` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_concordance_blocks_json(
    engine: *const PureEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        out_json(&wire::blocks_to_wire(panel::concordance(e, code)))
    })
}

/// The concordance filtered to one rendering of a code, as blocks. Never null
/// on a live engine.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_rendering_concordance_blocks_json(
    engine: *const PureEngine,
    code: *const c_char,
    rendering: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code), Some(rendering)) = (engine.as_ref(), opt_str(code), opt_str(rendering)) else {
            return ptr::null_mut();
        };
        out_json(&wire::blocks_to_wire(panel::rendering_concordance(e, code, rendering)))
    })
}

/// The threads list as blocks. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_threads_blocks_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::blocks_to_wire(panel::threads_list(e))),
        None => ptr::null_mut(),
    })
}

/// One thread's detail as blocks (out-of-range index → the threads list). Never
/// null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_thread_blocks_json(engine: *const PureEngine, index: u32) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::blocks_to_wire(panel::thread_detail(e, index as usize))),
        None => ptr::null_mut(),
    })
}

/// The tags list as blocks. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_tags_blocks_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::blocks_to_wire(panel::tags_list(e))),
        None => ptr::null_mut(),
    })
}

/// One tag's detail as blocks (out-of-range index → the tags list). Never null
/// on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_tag_blocks_json(engine: *const PureEngine, index: u32) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::blocks_to_wire(panel::tag_detail(e, index as usize))),
        None => ptr::null_mut(),
    })
}

/// The weaves list as blocks. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_weaves_blocks_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::blocks_to_wire(panel::weaves_list(e))),
        None => ptr::null_mut(),
    })
}

/// The suggested-weave review queue as blocks. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_suggested_blocks_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::blocks_to_wire(panel::suggested(e))),
        None => ptr::null_mut(),
    })
}

/// A weave compare card as blocks (out-of-range index → empty). `full` adds the
/// edit-notes action. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_compare_blocks_json(
    engine: *const PureEngine,
    index: u32,
    full: bool,
) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::blocks_to_wire(panel::compare_card(e, full, index as usize))),
        None => ptr::null_mut(),
    })
}

/// Search results as blocks (goto link or ranked hits with snippets). Null when
/// the query is blank or the engine is null.
///
/// # Safety
/// `engine` is a live engine; `query` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_search_blocks_json(
    engine: *const PureEngine,
    query: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(query)) = (engine.as_ref(), opt_str(query)) else {
            return ptr::null_mut();
        };
        if query.trim().is_empty() {
            return ptr::null_mut();
        }
        out_json(&wire::blocks_to_wire(panel::search(e, query)))
    })
}

/// How many occurrence verses the english-gloss tally samples.
const GLOSS_SAMPLE: usize = 80;

/// The modal KJV rendering of a code — what an English reader recognises
/// ("world" for κόσμος) rather than Strong's etymological headword. Ported
/// verbatim from the GTK shell so every shell shows the same chips.
fn english_gloss(e: &PureEngine, code: &str) -> Option<String> {
    let mut tally: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for r in e.occ_ix.verses(code).iter().take(GLOSS_SAMPLE) {
        if let Some(v) = e.corpus.verse(r) {
            for t in &v.tokens {
                // Skip translator-supplied words: they render nothing original.
                if t.flags & corpus::FLAG_ADDED == 0 && t.strongs.iter().any(|c| c == code) {
                    let w = normalise_word(&t.word);
                    if !w.is_empty() {
                        *tally.entry(w).or_default() += 1;
                    }
                }
            }
        }
    }
    if !tally.is_empty() {
        let mut ranked: Vec<(String, u32)> = tally.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        return Some(ranked.swap_remove(0).0);
    }
    // No tagged occurrence — distil the dictionary as a last resort.
    let entry = e.strongs.get(code)?;
    entry
        .def
        .as_deref()
        .and_then(distil_gloss)
        .or_else(|| entry.kjv.as_deref().and_then(distil_gloss))
}

fn normalise_word(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric()).to_string()
}

/// Distil the first clean English fragment from a Strong's definition/KJV
/// field: drop parenthetical asides, take the leading comma/semicolon clause,
/// cap its length.
fn distil_gloss(raw: &str) -> Option<String> {
    let mut cleaned = String::with_capacity(raw.len());
    let mut depth: i32 = 0;
    for ch in raw.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = (depth - 1).max(0),
            _ if depth == 0 => cleaned.push(ch),
            _ => {}
        }
    }
    let first = cleaned
        .split(|c| c == ',' || c == ';')
        .map(str::trim)
        .find(|p| p.chars().any(|c| c.is_alphabetic()))?;
    let capped: String = first.chars().take(30).collect();
    let g = capped.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
    if g.is_empty() {
        None
    } else {
        Some(g)
    }
}

/// Author a weave link carrying word spans — the Full-study "pin a word in
/// each pane, widen, ＋ link" flow. Span bounds are token indices; pass a
/// negative bound for a span-less side. Null on success, else an owned error.
///
/// # Safety
/// `engine` is a live engine; string params are null or valid NUL-terminated
/// UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_weave_add_link_spans(
    engine: *mut PureEngine,
    name: *const c_char,
    a_ref: *const c_char,
    b_ref: *const c_char,
    a_lo: i32,
    a_hi: i32,
    b_lo: i32,
    b_hi: i32,
    added: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string(
                "engine has no home directory (opened from bytes); cannot author".to_string(),
            );
        };
        let (Some(name), Some(a), Some(b), Some(added)) =
            (opt_str(name), opt_str(a_ref), opt_str(b_ref), opt_str(added))
        else {
            return out_string("null or invalid argument".to_string());
        };
        let (Some(av), Some(bv)) = (VRef::parse_ref_key(a), VRef::parse_ref_key(b)) else {
            return out_string("bad ref".to_string());
        };
        let span = |lo: i32, hi: i32| -> Option<(u16, u16)> {
            if lo < 0 || hi < 0 {
                None
            } else {
                Some((lo.min(hi) as u16, lo.max(hi) as u16))
            }
        };
        let mut study = engine.study_write();
        match weave::add_link(
            &home,
            &study.weaves,
            name,
            WeaveKind::Quotation,
            canon::TOKENIZATION_VERSION,
            added,
            Link::canon_span(av, bv, "", span(a_lo, a_hi), span(b_lo, b_hi)),
        ) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Parse a panel link URI into the typed verb the shell dispatches on
/// (`{verb, …}`; see `pure_core::panel::parse_link`) — the one verb vocabulary,
/// so a non-Rust shell routes clicks through the core instead of re-splitting
/// the URI string and risking drift from what the panel emits. Engine-
/// independent. Null for an unknown verb or malformed payload (a shell then
/// ignores the click).
///
/// # Safety
/// `uri` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn pure_route_link_json(uri: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || match opt_str(uri).and_then(panel::parse_link) {
        Some(link) => out_json(&wire::link_to_wire(link)),
        None => ptr::null_mut(),
    })
}

// ── config / session (engine-independent; shared with the GTK shell) ──────────

/// Load the cross-platform shell config (`%APPDATA%\pure-study\config.json` on
/// Windows) as JSON: `{studyMode, bodySize, openPanes, activePane, firstRun}`.
/// `firstRun` is true only when no config file existed. Never null.
#[no_mangle]
pub extern "C" fn pure_config_load_json() -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (cfg, first_run) = config::load();
        out_json(&wire::config_to_wire(&cfg, first_run))
    })
}

/// Save the shell config from the same JSON shape (unknown fields ignored,
/// `firstRun` ignored). Null on success, else an owned error message.
///
/// # Safety
/// `json` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn pure_config_save_json(json: *const c_char) -> *mut c_char {
    guard_err(|| {
        let Some(s) = opt_str(json) else {
            return out_string("null or invalid argument".to_string());
        };
        let w: wire::WireConfigState = match serde_json::from_str(s) {
            Ok(w) => w,
            Err(e) => return out_string(format!("bad config json: {e}")),
        };
        match config::save(&wire::config_from_wire(&w)) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

// ── Tier 0: copy, personal notes, highlights, themes, warming, guide ──────────

/// Clipboard text for a verse (or its chapter, for the `chapter*` kinds) in one
/// of the shapes `pure_core::export::CopyKind` names (`verse` / `verseRef` /
/// `verseMarkdown` / `chapter` / `chapterMarkdown`). Plain text, not JSON; null
/// on a bad ref, an unknown kind, or a verse the corpus lacks. Caller-freed.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated
/// UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_copy_text(
    engine: *const PureEngine,
    ref_key: *const c_char,
    kind: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk), Some(kind)) = (engine.as_ref(), opt_str(ref_key), opt_str(kind))
        else {
            return ptr::null_mut();
        };
        let (Some(vref), Some(kind)) = (VRef::parse_ref_key(rk), export::parse_kind(kind)) else {
            return ptr::null_mut();
        };
        match export::copy_text(&e.corpus, &vref, kind) {
            Some(s) => out_string(s),
            None => ptr::null_mut(),
        }
    })
}

/// The reader's personal note on a verse as JSON (`{verse,display,text,created,
/// updated}`), or null when the verse has no note (or the engine has no home).
///
/// # Safety
/// `engine` is a live engine; `ref_key` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_user_note_json(
    engine: *const PureEngine,
    ref_key: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let Some(v) = VRef::parse_ref_key(rk) else { return ptr::null_mut() };
        match e.study_read().user_notes.get(&v) {
            Some(ln) => out_json(&wire::WireUserNote {
                verse: v.ref_key(),
                display: v.display(),
                text: ln.note.text.clone(),
                created: ln.note.created.clone(),
                updated: ln.note.updated.clone(),
            }),
            None => ptr::null_mut(),
        }
    })
}

/// All the reader's personal notes as JSON (`{notes:[…]}`), in canonical reading
/// order — for the gutter marks and a "your notes" browser. Never null on a live
/// engine (no notes → empty list).
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_user_notes_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let study = e.study_read();
        let mut notes: Vec<&usernote::LoadedNote> = study.user_notes.values().collect();
        notes.sort_by(|a, b| a.note.vref.reading_key().cmp(&b.note.vref.reading_key()));
        let notes = notes
            .into_iter()
            .map(|ln| wire::WireUserNote {
                verse: ln.note.vref.ref_key(),
                display: ln.note.vref.display(),
                text: ln.note.text.clone(),
                created: ln.note.created.clone(),
                updated: ln.note.updated.clone(),
            })
            .collect();
        out_json(&wire::WireUserNotes { notes })
    })
}

/// Set (or clear, with an empty `text`) the reader's personal note on a verse,
/// atomically, then reload. `stamp` is a caller-supplied UTC timestamp. Null on
/// success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated
/// UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_user_note_set(
    engine: *mut PureEngine,
    ref_key: *const c_char,
    text: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string(
                "engine has no home directory (opened from bytes); cannot author".to_string(),
            );
        };
        let (Some(rk), Some(text), Some(stamp)) = (opt_str(ref_key), opt_str(text), opt_str(stamp))
        else {
            return out_string("null or invalid argument".to_string());
        };
        let Some(v) = VRef::parse_ref_key(rk) else {
            return out_string(format!("bad ref: {rk}"));
        };
        let mut study = engine.study_write();
        match usernote::set_note(&home, &v, text, stamp) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Set (or clear, with a null `color`) the swatch colour of the tag named
/// `name`, then reload. Drives highlighting (a colour-bearing tag washes its
/// verses). Null on success, else an owned error.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated
/// UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_tag_set_color(
    engine: *mut PureEngine,
    name: *const c_char,
    color: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string(
                "engine has no home directory (opened from bytes); cannot author".to_string(),
            );
        }
        let Some(name) = opt_str(name) else {
            return out_string("null or invalid name".to_string());
        };
        let mut study = engine.study_write();
        match tag::set_color(&study.tags, name, opt_str(color)) {
            Ok(()) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Add a word-precise highlight range to the tag named `name` (created on first
/// use, taking `color` as its tone). The range runs from `start_ref`+`start_tok`
/// to `end_ref`+`end_tok` (inclusive token indices under `kjv1769-tok2`);
/// endpoints are ordered canonically here, so a backwards drag is fine. `color`
/// may be null (the range then inherits the tag's colour). `added` is a
/// caller-supplied UTC timestamp. Null on success, else an owned error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_highlight_add(
    engine: *mut PureEngine,
    name: *const c_char,
    color: *const c_char,
    start_ref: *const c_char,
    start_tok: u32,
    end_ref: *const c_char,
    end_tok: u32,
    added: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(name), Some(sr), Some(er), Some(added)) =
            (opt_str(name), opt_str(start_ref), opt_str(end_ref), opt_str(added))
        else {
            return out_string("null or invalid argument".to_string());
        };
        let (Some(sv), Some(ev)) = (VRef::parse_ref_key(sr), VRef::parse_ref_key(er)) else {
            return out_string("bad ref".to_string());
        };
        let (st, et) = (start_tok.min(u16::MAX as u32) as u16, end_tok.min(u16::MAX as u32) as u16);
        // Canonicalize so start ≤ end (a drag can go either direction).
        let ((sv, st), (ev, et)) = if (sv.reading_key(), st) <= (ev.reading_key(), et) {
            ((sv, st), (ev, et))
        } else {
            ((ev, et), (sv, st))
        };
        let range = tag::HighlightRange {
            start: sv,
            start_tok: st,
            end: ev,
            end_tok: et,
            color: opt_str(color).map(str::to_string),
            note: None,
            added: added.to_string(),
        };
        let mut study = engine.study_write();
        match tag::add_highlight(&home, &study.tags, name, canon::TOKENIZATION_VERSION, range, added) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Remove the highlight range with these endpoints from the tag named `name`.
/// Endpoints are ordered canonically to match how they were stored. A missing
/// range is a no-op; a missing tag is an error. Null on success, else an error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_highlight_remove(
    engine: *mut PureEngine,
    name: *const c_char,
    start_ref: *const c_char,
    start_tok: u32,
    end_ref: *const c_char,
    end_tok: u32,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let (Some(name), Some(sr), Some(er)) =
            (opt_str(name), opt_str(start_ref), opt_str(end_ref))
        else {
            return out_string("null or invalid argument".to_string());
        };
        let (Some(sv), Some(ev)) = (VRef::parse_ref_key(sr), VRef::parse_ref_key(er)) else {
            return out_string("bad ref".to_string());
        };
        let (st, et) = (start_tok.min(u16::MAX as u32) as u16, end_tok.min(u16::MAX as u32) as u16);
        let ((sv, st), (ev, et)) = if (sv.reading_key(), st) <= (ev.reading_key(), et) {
            ((sv, st), (ev, et))
        } else {
            ((ev, et), (sv, st))
        };
        let range = tag::HighlightRange {
            start: sv,
            start_tok: st,
            end: ev,
            end_tok: et,
            color: None,
            note: None,
            added: String::new(),
        };
        let wanted = name.trim().to_lowercase();
        let mut study = engine.study_write();
        let found = study.tags.iter().find(|lt| lt.tag.name.to_lowercase() == wanted).cloned();
        match found {
            Some(lt) => match tag::remove_highlight(&lt, &range) {
                Ok(()) => {
                    *study = load_study(&engine.home);
                    ptr::null_mut()
                }
                Err(e) => out_string(e.to_string()),
            },
            None => out_string(format!("no tag named {name}")),
        }
    })
}

/// Drop every highlight range covering `verse_ref` from all tags, then reload —
/// the drag-remove path (a whole range goes even if only one of its verses was
/// targeted). Null on success, else an owned error.
///
/// # Safety
/// `engine` is valid; `verse_ref` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_highlight_clear_verse(
    engine: *mut PureEngine,
    verse_ref: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(vr) = opt_str(verse_ref).and_then(VRef::parse_ref_key) else {
            return out_string("bad ref".to_string());
        };
        let rk = vr.reading_key();
        let covers = |h: &tag::HighlightRange| h.start.reading_key() <= rk && rk <= h.end.reading_key();
        let mut study = engine.study_write();
        let affected: Vec<_> = study
            .tags
            .iter()
            .filter(|lt| lt.tag.highlights.iter().any(covers))
            .cloned()
            .collect();
        for lt in &affected {
            let mut t = lt.tag.clone();
            t.highlights.retain(|h| !covers(h));
            if let Err(e) = tag::write_tag(&lt.file, &t) {
                return out_string(e.to_string());
            }
        }
        if !affected.is_empty() {
            *study = load_study(&engine.home);
        }
        ptr::null_mut()
    })
}

/// The highlight washes for a chapter as JSON (`{book,chapter,verses:[{verse,
/// color}]}`): each verse that belongs to a colour-bearing tag, with the tone
/// the shell washes behind it. Never null on a live engine (none → empty list).
///
/// # Safety
/// `engine` is a live engine; `book` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_chapter_highlights_json(
    engine: *const PureEngine,
    book: *const c_char,
    chapter: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(book)) = (engine.as_ref(), opt_str(book)) else {
            return ptr::null_mut();
        };
        let Ok(chapter) = u16::try_from(chapter) else { return ptr::null_mut() };
        let study = e.study_read();
        let chapter_verses = e.corpus.chapter_verses(book, chapter);
        let verses = chapter_verses
            .iter()
            .filter_map(|v| {
                let vref = v.vref();
                tag::verse_color(&study.tags, &vref)
                    .map(|c| wire::WireVerseHighlight { verse: vref.ref_key(), color: c.to_string() })
            })
            .collect();
        // Word-precise cross-verse ranges → per-verse [lo,hi] runs (Tier 0 #4).
        let mut runs = Vec::new();
        for v in chapter_verses {
            let vref = v.vref();
            let len = u16::try_from(v.tokens.len()).unwrap_or(u16::MAX);
            for r in tag::verse_highlight_runs(&study.tags, &vref, len) {
                runs.push(wire::WireHighlightRun {
                    verse: vref.ref_key(),
                    lo: r.lo,
                    hi: r.hi,
                    color: r.color,
                });
            }
        }
        out_json(&wire::WireChapterHighlights { book: book.to_string(), chapter, verses, runs })
    })
}

/// The colour palette for a theme (`light`/`dark`/`night`; unknown → light) as
/// JSON — every semantic role as a `#rrggbb` hex. Engine-independent. Never null.
///
/// # Safety
/// `theme` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn pure_theme_palette_json(theme: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let t = opt_str(theme).and_then(theme::Theme::parse).unwrap_or(theme::Theme::Light);
        out_json(&theme::palette(t))
    })
}

/// The fixed highlight tones (`{tones:[{name,hex}]}`) — the shell's swatch menu.
/// Engine-independent. Never null.
#[no_mangle]
pub extern "C" fn pure_theme_highlight_tones_json() -> *mut c_char {
    guard(ptr::null_mut(), || out_json(&wire::highlight_tones_to_wire()))
}

/// Force the lazy analytics indexes (concept engine, leitwort scan, SIF verse
/// similarity) to build now — call once on a background thread at startup in
/// Full mode so the first study click doesn't stall. Safe to call from any
/// thread (the builds are `OnceLock`-guarded) and idempotent. Null on success,
/// else an owned error.
///
/// # Safety
/// `engine` is a live engine (or null → an error string).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_warm_indexes(engine: *const PureEngine) -> *mut c_char {
    guard_err(|| {
        let Some(e) = engine.as_ref() else {
            return out_string("null engine".to_string());
        };
        e.concept();
        e.leitwort();
        e.verse_sim();
        ptr::null_mut()
    })
}

// ── memorization (Tier 2 #15): SRS cards, drills, coverage + activity ─────────

/// Grade the verse `verse_ref` at `now` (RFC3339 UTC), creating its SRS card on
/// first review; SM-2 reschedules and appends to the review log. `grade` is one
/// of `again` / `hard` / `good` / `easy`. Null on success, else an owned error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_grade(
    engine: *mut PureEngine,
    verse_ref: *const c_char,
    grade: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(vr), Some(grade_s), Some(now)) = (opt_str(verse_ref), opt_str(grade), opt_str(now)) else {
            return out_string("null or invalid argument".to_string());
        };
        let Some(vref) = VRef::parse_ref_key(vr) else {
            return out_string("bad ref".to_string());
        };
        let Some(g) = memory::Grade::parse(grade_s) else {
            return out_string(format!("unknown grade: {grade_s}"));
        };
        let (cards, _) = memory::load_cards(&home);
        match memory::grade_verse(&home, &cards, &vref, canon::TOKENIZATION_VERSION, g, now) {
            Ok(_) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Start memorizing `verse_ref` — seed its SRS card (due now) if it isn't
/// already one; no review is logged. `now` is a caller-supplied UTC timestamp.
/// Null on success, else an owned error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_add(
    engine: *mut PureEngine,
    verse_ref: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory; cannot author".to_string());
        };
        let (Some(vr), Some(now)) = (opt_str(verse_ref), opt_str(now)) else {
            return out_string("null or invalid argument".to_string());
        };
        let Some(vref) = VRef::parse_ref_key(vr) else {
            return out_string("bad ref".to_string());
        };
        let (cards, _) = memory::load_cards(&home);
        if cards.contains_key(&vref) {
            return ptr::null_mut();
        }
        let card = memory::Card::new(vref, canon::TOKENIZATION_VERSION, now);
        match memory::write_card(&home, &card) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Stop memorizing `verse_ref` (remove its card); a missing card is a no-op.
/// Null on success, else an owned error.
///
/// # Safety
/// `engine` is valid; `verse_ref` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_remove(
    engine: *mut PureEngine,
    verse_ref: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory; cannot author".to_string());
        };
        let Some(vref) = opt_str(verse_ref).and_then(VRef::parse_ref_key) else {
            return out_string("bad ref".to_string());
        };
        match memory::remove_card(&home, &vref) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// The verse's SRS card as JSON (schedule + mastery + review log), or null if
/// the verse isn't being memorized (or the engine has no home).
///
/// # Safety
/// `engine` is a live engine; `verse_ref` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_card_json(
    engine: *const PureEngine,
    verse_ref: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref)) =
            (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key))
        else {
            return ptr::null_mut();
        };
        let Some(home) = e.home.as_ref() else { return ptr::null_mut() };
        let (cards, _) = memory::load_cards(home);
        match cards.get(&vref) {
            Some(c) => out_json(&wire::memory_card_to_wire(c)),
            None => ptr::null_mut(),
        }
    })
}

/// Verses due for review at `now` (RFC3339), reading order — the study queue, as
/// `{refs:[...]}`. Never null on a live engine (empty when nothing is due).
///
/// # Safety
/// `engine` is a live engine; `now` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_due_json(
    engine: *const PureEngine,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(now)) = (engine.as_ref(), opt_str(now)) else {
            return ptr::null_mut();
        };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        let refs = memory::due_queue(&cards, now).iter().map(VRef::ref_key).collect();
        out_json(&wire::WireMemoryDue { refs })
    })
}

/// The coverage-map data at `now`: per-verse standing (mastery + recency) plus
/// the 8-section rollup, as `{verses:[...],sections:[...]}`.
///
/// # Safety
/// `engine` is a live engine; `now` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_coverage_json(
    engine: *const PureEngine,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(now)) = (engine.as_ref(), opt_str(now)) else {
            return ptr::null_mut();
        };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        out_json(&wire::WireMemoryCoverage {
            verses: memory::coverage(&cards, now),
            sections: memory::coverage_by_section(&cards),
        })
    })
}

/// The activity heatmap as `{days:[{day,reviews}]}` — reviews per calendar day,
/// oldest first, from every card's review log. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_activity_json(engine: *const PureEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        out_json(&wire::WireMemoryActivity { days: memory::activity_by_day(&cards) })
    })
}

/// A drill prompt for `verse_ref` at blank-out `level` (0 = full text … max):
/// the verse text, its first-letter skeleton, and the blanked form. Null if the
/// verse isn't found.
///
/// # Safety
/// `engine` is a live engine; `verse_ref` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_drill_json(
    engine: *const PureEngine,
    verse_ref: *const c_char,
    level: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref)) =
            (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key))
        else {
            return ptr::null_mut();
        };
        let Some(v) = e.corpus.verse(&vref) else { return ptr::null_mut() };
        let text = v.body();
        let level = level.min(u8::MAX as u32) as u8;
        out_json(&wire::WireMemoryDrill {
            reference: vref.ref_key(),
            first_letters: memory::first_letters(&text),
            blanked: memory::blank_out(&text, level),
            text,
            level,
            max_level: memory::MAX_BLANK_LEVEL,
        })
    })
}

/// Score a typed recall of `verse_ref` against the verse text — `{accuracy,
/// words:[{word,ok}]}`, LCS-aligned. Null if the verse isn't found.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn pure_engine_memory_score_json(
    engine: *const PureEngine,
    verse_ref: *const c_char,
    typed: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref), Some(typed)) =
            (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key), opt_str(typed))
        else {
            return ptr::null_mut();
        };
        let Some(v) = e.corpus.verse(&vref) else { return ptr::null_mut() };
        out_json(&memory::score_recall(typed, &v.body()))
    })
}

/// The in-app guide as panel blocks. Engine-independent (static content). Never
/// null.
#[no_mangle]
pub extern "C" fn pure_panel_guide_blocks_json() -> *mut c_char {
    guard(ptr::null_mut(), || out_json(&wire::blocks_to_wire(panel::guide_blocks())))
}

/// The About card as panel blocks. Engine-independent (static content). Never
/// null.
#[no_mangle]
pub extern "C" fn pure_panel_about_blocks_json() -> *mut c_char {
    guard(ptr::null_mut(), || out_json(&wire::blocks_to_wire(panel::about_blocks())))
}

/// Parse a `(kind, value)` pair into a [`TagTarget`].
///
/// # Safety
/// `kind`/`value` are null or valid NUL-terminated UTF-8 for the call.
unsafe fn parse_target(kind: *const c_char, value: *const c_char) -> Result<TagTarget, String> {
    let (Some(kind), Some(value)) = (opt_str(kind), opt_str(value)) else {
        return Err("null kind or value".to_string());
    };
    match kind {
        "verse" => VRef::parse_ref_key(value)
            .map(TagTarget::Verse)
            .ok_or_else(|| format!("bad ref: {value}")),
        "concept" => Ok(TagTarget::Concept(value.to_string())),
        other => Err(format!("bad target kind: {other}")),
    }
}

#[cfg(test)]
mod tests;
