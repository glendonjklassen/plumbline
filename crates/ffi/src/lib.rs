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
use pure_core::tag::{self, LoadedTag, TagTarget};
use pure_core::thread::{self, LoadedThread, ThreadEntry};
use pure_core::weave::{self, Link, LoadedWeave, WeaveKind};
use pure_core::{canon, notes, VRef};
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
}

/// Load notes + threads + tags + weaves from `home` (empty without one).
fn load_study(home: &Option<PathBuf>) -> StudyData {
    match home {
        Some(home) => StudyData {
            notes: notes::load_notes(home.join("data").join("kjv-notes.jsonl"))
                .unwrap_or_default(),
            threads: thread::load_threads(home).0,
            tags: tag::load_tags(home).0,
            weaves: weave::load_weaves(home).0,
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
            .map(|p| wire::WireBridgePartner { code: p.code, sources: p.sources, prior: p.prior })
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
