//! `plumbline-ffi` — the single, flat C ABI over `plumbline-core` + `plumbline-layout`.
//!
//! Decision #1 (native-per-platform) says: define the app's data surface
//! **once** here as a C ABI, then let each native UI bind to it — csbindgen /
//! P-Invoke for C# (WinUI) and JNA/UniFFI for Kotlin (Android). Every shell
//! paints the display list the core produces and forwards input coordinates
//! back across this boundary; no study logic is reimplemented in Kotlin or C#.
//!
//! ## Shape of the ABI
//!
//! * Two **opaque handles**: [`PlumblineEngine`] (the loaded corpus + Strong's +
//!   search/occurrence indices) and [`PlumblineDisplayList`] (one laid-out chapter).
//!   C sees them as forward-declared structs; only these functions touch them.
//! * **Primitives** for scalar params (chapter numbers, coordinates).
//! * **JSON** (NUL-terminated UTF-8) for every structured return value. JSON is
//!   the lowest common denominator across C#, Kotlin, Swift and JS, it keeps the
//!   ABI tiny and stable (a future field is additive, not a struct-layout
//!   break), and it is exactly what a cross-device sync SaaS will speak later.
//!   The wire schemas live in the [`wire`] module and are the frozen contract.
//! * Layout keeps living in Rust: the caller passes a [`PlumblineMeasureFn`] callback
//!   so `plumbline_layout::layout_chapter` measures text with the platform's own
//!   engine (Pango/DirectWrite/Android) while the hard line-breaking + per-word
//!   hit-region bookkeeping stays written once, here.
//!
//! ## Memory & safety contract (read before binding)
//!
//! * Every `*mut c_char` returned by a `plumbline_*` function is owned by the caller
//!   and must be released with [`plumbline_string_free`]. A null return means
//!   "no value" (blank query, unknown code) or an error (see per-fn docs).
//! * Every handle (`*mut PlumblineEngine`, `*mut PlumblineDisplayList`) must be released
//!   with its matching `*_free`. Freeing null is a no-op; double-free is UB.
//! * Input `*const c_char` / byte pointers are **borrowed for the call only**;
//!   the caller keeps ownership. Strings must be valid UTF-8.
//! * Every entry point is wrapped in `catch_unwind`: a Rust panic can never
//!   unwind across the C boundary (that would be UB). A panic surfaces as a
//!   null / `0` / `0.0` return instead.
//! * A `*const PlumblineEngine` may be shared across threads for these read-only
//!   calls; a `*mut PlumblineDisplayList` is single-owner (do not hit-test one from
//!   two threads at once — though all calls here are `&`-only, so it is in
//!   practice also safe to read concurrently).

// Wasm-only shims for the web shell's TS binding — not part of the native C
// ABI surface (header / C# / Kotlin). cbindgen doesn't evaluate `cfg`, so
// plumbline-bindgen excludes this module's items by name; keep its exclude list in
// step with the exports here.
#[cfg(target_arch = "wasm32")]
mod wasm;

use std::ffi::{c_char, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::ptr;
use std::sync::OnceLock;

use plumbline_core::akjv;
use plumbline_core::config;
use plumbline_core::corpus::{self, Corpus};
use plumbline_core::crossref::{self, XRefIx};
use plumbline_core::memory;
use plumbline_core::panel::{self, PanelSource};
use plumbline_core::reading;
use plumbline_core::renderings::{self, Renderings};
use plumbline_core::search::{self, Notes, SearchIx};
use plumbline_core::strongs::{self, OccurrenceIx, StrongsDict};
use plumbline_core::tag::{self, LoadedTag, TagTarget};
use plumbline_core::thread::{self, LoadedThread, ThreadEntry};
use plumbline_core::weave::{self, Link, LoadedWeave, WeaveKind};
use plumbline_core::{canon, devotional, export, hymnal, i18n, notes, session_slot, theme, usernote, VRef};
use plumbline_layout::{layout_chapter, DisplayList, LayoutConfig, Measure, MeasureMemo, Memoized};
use plumbline_rnd::{bridge, burst, concept, morph};

pub mod devotionals;
pub mod dwell;
pub mod plans;
pub mod reading_map;
pub mod share;
mod wire;

// ── token flag bits (mirror the core's `FLAG_*`; exported to bindings) ───────
//
// Written as bare literals (not `= corpus::FLAG_*`) so cbindgen can const-fold
// them into `#define`s in the C header. The `const _` assertions below fail the
// build if they ever drift from the core's canonical values, so the mirror
// stays honest without costing the bindings.
//
// EVERY flag bit a shell tests belongs here, with its assertion — otherwise a
// bare literal in a shell answers to nothing while it drifts from the core.
// `flag_bits_are_exported_with_their_assertion` in `tests.rs` checks the header,
// the assertions, and both shells' mirrors.

/// Word supplied by the KJV translators (rendered in italics).
pub const PLUMBLINE_FLAG_ADDED: u32 = 1;
/// The divine name.
pub const PLUMBLINE_FLAG_DIVINE: u32 = 2;
/// Psalm superscription / title text.
pub const PLUMBLINE_FLAG_TITLE: u32 = 4;
/// A paragraph mark (¶) precedes this word.
pub const PLUMBLINE_FLAG_PARA: u32 = 8;
/// Display only: this word is an AKJV re-rendering, set by the overlay on the
/// display list as it passes. NEVER present in `kjv.jsonl`, whose bitfield is a
/// frozen contract — so a shell reads this bit off a display-list item or a
/// panel token, not off stored data.
pub const PLUMBLINE_FLAG_RERENDERED: u32 = 16;

const _: () = assert!(PLUMBLINE_FLAG_ADDED == corpus::FLAG_ADDED);
const _: () = assert!(PLUMBLINE_FLAG_DIVINE == corpus::FLAG_DIVINE);
const _: () = assert!(PLUMBLINE_FLAG_TITLE == corpus::FLAG_TITLE);
const _: () = assert!(PLUMBLINE_FLAG_PARA == corpus::FLAG_PARA);
const _: () = assert!(PLUMBLINE_FLAG_RERENDERED == akjv::FLAG_RERENDERED);

/// How many verse references an occurrence list returns before it is capped
/// (`total` in the JSON stays honest above this).
pub const OCCURRENCE_CAP: usize = 500;

/// The wire-JSON contract version. Bump on any **non-additive** change to the
/// payload shapes in `wire.rs` (renames/removals/retypes) so typed decoders
/// can fail loudly instead of silently reading nulls; purely additive fields
/// do not bump it. Exported to the C header; golden samples are pinned in
/// `tests.rs`.
/// Currently 2: the last bump was `rename_all_fields` on the tagged unions in
/// `wire.rs` (a rename, so it bumped by the rule above). Nothing compares this
/// constant yet (TODO §H tracks making it a live handshake), so the value is a
/// record rather than a gate.
pub const PLUMBLINE_WIRE_VERSION: u32 = 2;

/// Verses per warm slice on the web's chunked warm-up
/// (`plumbline_engine_warm_step`). Lives here rather than in `wasm.rs` so the
/// slicing tests, which cannot call the wasm-only export, drive the same size the
/// shell does — a test with its own copy of this number can pass at a slice size
/// the product never uses.
///
/// `allow(dead_code)` because its two callers are the wasm-only export and the
/// tests, and a plain host build compiles neither.
#[allow(dead_code)]
pub(crate) const WARM_SLICE: usize = 2048;

// ── opaque handles ────────────────────────────────────────────────────────────

/// The loaded, immutable study core: corpus + Strong's dictionary + the search
/// and occurrence indices every lookup rides on. Opaque to C; construct with
/// [`plumbline_engine_open`] / [`plumbline_engine_open_from_bytes`], release with
/// [`plumbline_engine_free`].
pub struct PlumblineEngine {
    corpus: Corpus,
    /// Strong's dictionary — late-loadable (TODO #28): the web boots on the
    /// corpus ALONE for the fastest possible text-on-screen, and the rest of
    /// the core pack arrives moments later via `load_core_data`. Unset means
    /// "not here yet": lookups answer empty and fill on the shell's re-fetch.
    strongs: OnceLock<StrongsDict>,
    /// Whether `strongs` holds a LOCALIZED dictionary rather than the English
    /// source (`strongs_for`) — the panel labels renderings for the right Bible
    /// and carries the machine-translation caveat from this.
    strongs_localized: std::sync::atomic::AtomicBool,
    /// The corpus-derived indexes are built LAZILY (TODO #28: boot cost —
    /// the app opens like Instagram, many times a day; every millisecond of
    /// open is paid every time). First access builds; `warm_indexes` forces
    /// them right after the shell hands its UI over, off-thread.
    search_ix: OnceLock<SearchIx>,
    /// Partially-folded search index, for the web's sliced warm-up
    /// (`plumbline_engine_warm_step`). A Mutex because Android may call the
    /// ABI from more than one thread; the web worker is single-threaded.
    search_partial: std::sync::Mutex<Option<search::SearchIxBuilder>>,
    /// Sliced builders for the two indexes a WORD CLICK needs. Warmed at boot
    /// the same way the search index is, so a click does not build them whole
    /// on the first tap of every session.
    occ_partial: std::sync::Mutex<Option<strongs::OccurrenceIxBuilder>>,
    renderings_partial: std::sync::Mutex<Option<renderings::RenderingsBuilder>>,
    concept_partial: std::sync::Mutex<Option<concept::ConceptBuilder>>,
    xref_partial: std::sync::Mutex<Option<crossref::XRefIxBuilder>>,
    leitwort_partial: std::sync::Mutex<Option<burst::LeitwortBuilder>>,
    /// How far the chunked warm has got. An explicit phase (rather than
    /// "build the next thing that is missing") guarantees the loop terminates
    /// even when a build legitimately cannot happen yet.
    warm_phase: std::sync::atomic::AtomicUsize,
    /// Whether a SLICED warm is driving this engine. Set by the first
    /// `warm_next` call and never cleared.
    ///
    /// While it is set, a reader's tap must never BUILD a missing index. The
    /// whole point of slicing is that the work is spread across macrotasks so the
    /// one thread that answers taps and layouts stays answerable — and a
    /// `get_or_init` inside a study call throws every bit of that away in a
    /// single blocking lump. Measured on a phone: **21,966 ms inside one
    /// `wordStudyBlocks`**, which froze the worker so completely that it also
    /// stranded its own in-flight downloads.
    ///
    /// So the study answers with what is READY. The warm keeps going in the
    /// background and the shell re-fetches as each index lands, which is the same
    /// fill-in-later path Strong's has always used before stage 2 arrives.
    ///
    /// Android never calls `warm_next` — it uses `plumbline_engine_warm_indexes`,
    /// which builds everything up front in well under a second — so this stays
    /// false there and nothing about that shell changes.
    defer_builds: std::sync::atomic::AtomicBool,
    occ_ix: OnceLock<OccurrenceIx>,
    /// The rendering lens: code → English renderings and surface word → codes,
    /// both corpus-derived and immutable after open (like `occ_ix`).
    renderings: OnceLock<Renderings>,
    /// The data home, if opened from one — required to author (write) study
    /// data. `None` when opened from bytes (study data is then read-only/empty).
    home: Option<PathBuf>,
    /// Personal study data (margin notes, threads, tags, the weave graph),
    /// loaded from `home` and **reloaded after any authoring write** — so it
    /// sits behind an RwLock: the README promises `*const PlumblineEngine` is safe
    /// to share across threads for reads, and a C# shell may author off its UI
    /// thread while another thread reads.
    study: std::sync::RwLock<StudyData>,
    /// R&D tier: the fused OT↔NT bridge plus the optional morphology artifact.
    /// The artifact loads at open when present in the
    /// home, but they may also *arrive after open* — the web shell boots on
    /// the core pack and fetches the R&D pack in the background, then calls
    /// [`plumbline_engine_load_rnd_data`] — hence `OnceLock` (set-once through
    /// `&self`, thread-safe): unset means "not (yet) available".
    bridge: OnceLock<bridge::FusedBridge>,
    morph: OnceLock<morph::MorphData>,
    /// TSK topical cross-references (parsed lazily from the home — an 8.5 MB
    /// TSV nobody should pay for at every open).
    xref_ix: OnceLock<XRefIx>,
    /// The hymnal, parsed lazily: nobody pays for it before the hymn tab opens.
    /// Set only from a NON-EMPTY parse (the [`Self::strongs`] stance) — on the
    /// web `data/hymnal.json` rides the study stage and lands moments AFTER
    /// open, so a hymn tab opened in that gap probes empty, and caching that
    /// probe would keep the book empty for the whole session. The file is
    /// deliberately NOT on the web's eviction list — the first successful read
    /// can come at any point in a session, so the bytes have to still be there
    /// when it does.
    hymnal: OnceLock<hymnal::Hymnal>,
    /// The devotional catalogue, parsed lazily and on the same terms as the
    /// hymnal: set only from a NON-EMPTY parse, because `data/devotional.json`
    /// rides the pack too and a probe that lands before it would otherwise
    /// cache "no devotionals" for the whole session.
    devotionals: OnceLock<Vec<devotional::Devotional>>,
    /// The plain-English overlay (the AKJV delta), when the home carries one.
    /// A READING aid: it re-words the reader's view and nothing else — never a
    /// memory card, a Present hand-off, or copied text.
    akjv: OnceLock<akjv::Akjv>,
    /// Whether the reader has the overlay switched on. Engine state rather than
    /// a layout argument so a shell cannot end up with one pane modernised and
    /// the next not; OFF until asked, because the text is the KJV.
    akjv_on: std::sync::atomic::AtomicBool,
    /// The symbolic concept engine (collocations, distribution, communities)
    /// and the leitwort scan — corpus-wide sweeps, built lazily like the SIF
    /// model and cached for the engine's lifetime.
    concept: OnceLock<concept::Concept>,
    leitwort: OnceLock<std::collections::HashMap<String, burst::Burst>>,
    /// Words per chapter for the whole canon — the reading map's denominators.
    /// Built on first use and cached: the navigator asks for all 1,189 chapters
    /// every time it opens, and re-walking 31,102 verses per open buys nothing.
    reading_words: OnceLock<reading::ChapterWords>,
    /// How long the chapter on screen has really been read (`core::reading::
    /// DwellTracker`, driven by `plumbline_engine_reading_tick_json`). It lives
    /// on the engine because it is per-reader state over a clock the core does
    /// not have: a shell samples once a second and the core decides what that
    /// second was worth. A Mutex because Android ticks from a coroutine while
    /// the UI thread reads.
    dwell: std::sync::Mutex<reading::DwellTracker>,
    /// Remembered text widths, so a run the shell has already measured is never
    /// measured across the ABI again (see [`font_identity`] and
    /// `plumbline_layout::memo`). Engine-scoped rather than global: it dies with
    /// the engine, and each test gets its own.
    ///
    /// A Mutex because Android lays out on `Dispatchers.Default` — a pool thread
    /// that differs from turn to turn, so a thread-local memo would be cold on
    /// most chapter turns there. The lock is never held across the measurement
    /// callback itself.
    measure_memo: std::sync::Mutex<MeasureMemo>,
}

impl PlumblineEngine {
    fn new(corpus: Corpus, strongs: Option<StrongsDict>, home: Option<PathBuf>) -> PlumblineEngine {
        let strongs_cell = OnceLock::new();
        if let Some(sd) = strongs {
            let _ = strongs_cell.set(sd);
        }
        // R&D artifacts. The bridge's etymology layer works from the in-memory
        // dict even without a home; external witnesses + the embedding/morph
        // sidecars need a home's files. Without a home, no filesystem is
        // probed at all (a CWD-relative probe would be nondeterministic and a
        // mild data-injection surface).

        // KJV-ONLY, and this gate is load-bearing rather than an optimisation.
        // Morphology is keyed by (refKey, TOKEN INDEX) against `kjv1769-tok2`,
        // and the German corpus tokenizes the same verse into different words at
        // different indices — so applying it there would attach English grammar
        // notes to whichever German word happened to sit at that index. Same
        // argument for Strong's and for the plain-English overlay below, which is
        // a delta over KJV token runs.
        //
        // Not a version check on the sidecar file: the file is fine, it is the
        // CORPUS that is a different text.
        let kjv_text = corpus.tokenization_version() == canon::TOKENIZATION_VERSION;
        let morph = OnceLock::new();
        if let (Some(h), true) = (&home, kjv_text) {
            let data = h.join("data");
            if let Some(m) = morph::load_morph(canon::TOKENIZATION_VERSION, data.join("morphology.jsonl")) {
                let _ = morph.set(m);
            }
        }
        let study = load_study(&home);
        PlumblineEngine {
            corpus,
            strongs: strongs_cell,
            strongs_localized: std::sync::atomic::AtomicBool::new(false),
            search_ix: OnceLock::new(),
            search_partial: std::sync::Mutex::new(None),
            occ_partial: std::sync::Mutex::new(None),
            renderings_partial: std::sync::Mutex::new(None),
            concept_partial: std::sync::Mutex::new(None),
            xref_partial: std::sync::Mutex::new(None),
            leitwort_partial: std::sync::Mutex::new(None),
            warm_phase: std::sync::atomic::AtomicUsize::new(0),
            defer_builds: std::sync::atomic::AtomicBool::new(false),
            occ_ix: OnceLock::new(),
            renderings: OnceLock::new(),
            home,
            study: std::sync::RwLock::new(study),
            bridge: OnceLock::new(),
            morph,
            xref_ix: OnceLock::new(),
            akjv: OnceLock::new(),
            akjv_on: std::sync::atomic::AtomicBool::new(false),
            hymnal: OnceLock::new(),
            devotionals: OnceLock::new(),
            concept: OnceLock::new(),
            leitwort: OnceLock::new(),
            reading_words: OnceLock::new(),
            dwell: std::sync::Mutex::new(reading::DwellTracker::default()),
            measure_memo: std::sync::Mutex::new(MeasureMemo::new()),
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

    /// Load the optional R&D artifact (morphology) from the home if it was
    /// absent at open. Idempotent; nothing loads twice.
    ///
    /// The concept embedding no longer loads here — it went with the last thing
    /// that read it and no longer ships in the pack.
    fn load_rnd_data(&self) {
        self.load_morph_only();
    }

    /// The morphology sidecar alone (~10 MB of JSONL, or 3.3 MB packed, to parse).
    ///
    /// KJV-ONLY, the same gate `new` applies and for the same reason: morphology
    /// is keyed by (refKey, TOKEN INDEX) against `kjv1769-tok2`, so on the German
    /// corpus it would describe whichever word happened to sit at that index.
    /// [`PanelSource::is_kjv_text`] already withholds the gloss, so what the gate
    /// was missing here was not correctness but the WORK: a German reader with
    /// machine analysis on parsed 355,603 entries — in one synchronous block, on
    /// the only thread that answers taps — to build an index nothing would ever
    /// read. `new` had the gate; this path is the one the web shell takes, because
    /// its pack arrives after the engine opens.
    fn load_morph_only(&self) {
        let Some(h) = &self.home else { return };
        if self.morph.get().is_some() || self.corpus.tokenization_version() != canon::TOKENIZATION_VERSION {
            return;
        }
        let path = h.join("data").join("morphology.jsonl");
        if let Some(m) = morph::load_morph(canon::TOKENIZATION_VERSION, path) {
            let _ = self.morph.set(m);
        }
    }

    /// Strong's — empty until the dictionary loads (never cached-empty: the
    /// cell only ever sets from a real parse).
    fn strongs(&self) -> &StrongsDict {
        static EMPTY: OnceLock<StrongsDict> = OnceLock::new();
        self.strongs.get().unwrap_or_else(|| EMPTY.get_or_init(StrongsDict::new))
    }

    /// The fused OT↔NT bridge — needs Strong's, so it is None until the
    /// dictionary is in; built once on first use after that.
    fn bridge(&self) -> Option<&bridge::FusedBridge> {
        if let Some(b) = self.bridge.get() {
            return Some(b);
        }
        let sd = self.strongs.get()?;
        let built = match &self.home {
            Some(h) => bridge::FusedBridge::build(sd, h),
            None => bridge::FusedBridge::etymology_only(sd),
        };
        let _ = self.bridge.set(built);
        self.bridge.get()
    }

    /// Load the stage-2 core data (Strong's dictionary; the study data —
    /// incl. the 1769 margin notes — reloads) once the files have arrived in
    /// the home. Idempotent; cheap while they are still missing. NOTE: a
    /// search index built before this call keeps the notes it saw then.
    fn load_core_data(&self) {
        let Some(h) = &self.home else { return };
        // The study data (the 1769 margin notes among it) reloads whatever text
        // is open. Strong's loads for EITHER corpus — the dictionary is keyed
        // by code and each corpus carries its own token tags now (the German
        // corpus since merge-strongs.py), so nothing here is anchored to KJV
        // token indices.
        if self.strongs.get().is_none() {
            let (path, is_localized) = strongs_for(&h.join("data"));
            if let Ok(sd) = strongs::load_strongs(path) {
                let _ = self.strongs.set(sd);
                self.strongs_localized.store(is_localized, std::sync::atomic::Ordering::Relaxed);
            }
        }
        // THE MODERNIZATION BELONGS TO A TEXT, and the OPEN text is what picks
        // it — not the reader's language. A German reader whose Luther download
        // has not landed is reading the KJV, and the AKJV is a correct and
        // useful thing to offer them while they are on it.
        //
        // A language whose row names no modernization simply never loads one,
        // which is what makes `AkjvAvailable()` false and hides the toggle. That
        // used to be a comparison against the KJV's tokenization here, which
        // said "this feature is the norm and other texts are the exception"; it
        // is one English feature among the per-language columns now.
        let open_tok = self.corpus.tokenization_version().to_string();
        if let Some(file) = i18n::Lang::for_tokenization(&open_tok).and_then(|l| l.spec().modernization) {
            if self.akjv.get().is_none() {
                // Stage 2, beside Strong's: small, and wanted the moment the
                // reader flips the toggle rather than after a download they
                // must approve.
                if let Some(a) = akjv::load_akjv(&open_tok, h.join("data").join(file)) {
                    let _ = self.akjv.set(a);
                }
            }
        }
        *self.study_write() = load_study(&self.home);
    }

    /// The overlay, if this home has one and it matches the tokenization.
    fn akjv(&self) -> Option<&akjv::Akjv> {
        self.akjv.get()
    }

    /// The overlay only when the reader has actually asked for it.
    fn akjv_view(&self) -> Option<&akjv::Akjv> {
        self.akjv_on.load(std::sync::atomic::Ordering::Relaxed).then(|| self.akjv()).flatten()
    }

    /// The search index, built on first use; the reader's notes attach then
    /// (same content as the old at-open attach — notes searchable either way).
    fn search_ix(&self) -> &SearchIx {
        self.search_ix.get_or_init(|| {
            let mut ix = SearchIx::build(&self.corpus);
            ix.attach_notes(&self.corpus, &self.study_read().notes);
            ix
        })
    }

    /// Fold up to `n` more verses into the search index, returning 1 while
    /// work remains and 0 once it is built and installed. The web shell's
    /// sliced warm-up (`plumbline_engine_warm_step`); a no-op once the index
    /// exists, whoever built it.
    fn warm_search_slice(&self, n: usize) -> i32 {
        if self.search_ix.get().is_some() {
            return 0;
        }
        let Ok(mut guard) = self.search_partial.lock() else {
            return 0; // poisoned: leave it to the build-on-first-use path
        };
        let b = guard.get_or_insert_with(search::SearchIxBuilder::default);
        if b.feed(&self.corpus, n) {
            return 1;
        }
        let mut ix = guard.take().expect("builder present").finish();
        ix.attach_notes(&self.corpus, &self.study_read().notes);
        let _ = self.search_ix.set(ix);
        0
    }

    /// The occurrence index, built on first use (or warmed in slices below).
    fn occ_ix(&self) -> &OccurrenceIx {
        self.occ_ix.get_or_init(|| OccurrenceIx::build(&self.corpus))
    }

    /// Whether a sliced warm is running and on-demand builds are therefore
    /// forbidden. See [`PlumblineEngine::defer_builds`].
    fn deferring(&self) -> bool {
        self.defer_builds.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Declare that this shell warms in slices, so nothing may be built inside a
    /// reader's request.
    ///
    /// MUST BE SET AT OPEN, not when the warm happens to start. Arming it on the
    /// first `warm_next` looks equivalent and is not: the web's warm begins only
    /// after stage 2 has been fetched and parsed, which on a phone is ~550 ms
    /// after text appears — and a reader taps a word inside that window. The flag
    /// would still be false, the tap would build everything, and the freeze this
    /// prevents happens anyway. A desktop hides it because stage 2 there takes
    /// 40 ms and the warm wins the race.
    ///
    /// `allow(dead_code)`: only a slicing shell arms it, and the only slicing
    /// shell is the web — Android builds everything up front.
    #[allow(dead_code)]
    pub(crate) fn set_defer_builds(&self, on: bool) {
        self.defer_builds.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    // ── "ready" accessors ─────────────────────────────────────────────────────
    // Each returns the index only if using it costs nothing. Under a sliced warm
    // that means "only if already built"; otherwise it is the ordinary
    // build-on-first-use accessor and behaviour is unchanged.
    //
    // EVERY reader-facing panel path goes through these rather than the builders
    // above. A single one left pointing at a builder reintroduces the whole
    // freeze, because a study touches most of them in one call.

    fn occ_ix_ready(&self) -> Option<&OccurrenceIx> {
        if self.deferring() {
            self.occ_ix.get()
        } else {
            Some(self.occ_ix())
        }
    }

    fn renderings_ready(&self) -> Option<&Renderings> {
        if self.deferring() {
            self.renderings.get()
        } else {
            Some(self.renderings())
        }
    }

    fn xref_ix_ready(&self) -> Option<&XRefIx> {
        if self.deferring() {
            self.xref_ix.get()
        } else {
            Some(self.xref_ix())
        }
    }

    fn concept_ready(&self) -> Option<&concept::Concept> {
        if self.deferring() {
            self.concept.get()
        } else {
            Some(self.concept())
        }
    }

    fn leitwort_ready(&self) -> Option<&std::collections::HashMap<String, burst::Burst>> {
        if self.deferring() {
            self.leitwort.get()
        } else {
            Some(self.leitwort())
        }
    }

    fn bridge_ready(&self) -> Option<&bridge::FusedBridge> {
        if self.deferring() {
            self.bridge.get()
        } else {
            self.bridge()
        }
    }

    /// Fold `n` more verses into the occurrence index. 1 while work remains.
    fn warm_occ_slice(&self, n: usize) -> i32 {
        if self.occ_ix.get().is_some() {
            return 0;
        }
        let Ok(mut guard) = self.occ_partial.lock() else {
            return 0; // poisoned: leave it to the build-on-first-use path
        };
        let b = guard.get_or_insert_with(strongs::OccurrenceIxBuilder::default);
        if b.feed(&self.corpus, n) {
            return 1;
        }
        let _ = self.occ_ix.set(guard.take().expect("builder present").finish());
        0
    }

    /// One macrotask of warm-up. Returns 1 while work remains, 0 when the
    /// indexes a study needs are all in.
    ///
    /// `slice` is [`WARM_SLICE`] on every shipped path; it is a parameter only so
    /// tests can drive smaller steps.
    ///
    /// This is the whole point of the boot warm: every one of these is built on
    /// FIRST USE otherwise, and none of them survives the tab, so without it the
    /// reader's first word click of every session pays for all of them at once.
    /// The sliced phases come first because they are the biggest; what is left as
    /// a single build is only what measured small enough to be one — the bridge,
    /// at 3 ms — so a tap between phases is still answered. Re-running after the
    /// R&D pack lands picks up the SIF model.
    ///
    /// `allow(dead_code)` for the same reason as [`WARM_SLICE`]: the only callers
    /// are the wasm-only export and the tests, and a plain host build compiles
    /// neither. The allow covers the whole warm cluster this roots — the
    /// `warm_*_slice` helpers and the `*_partial` / `warm_phase` fields they
    /// touch — because rustc treats an allowed item as live and walks on from it.
    #[allow(dead_code)]
    fn warm_next(&self, slice: usize) -> i32 {
        use std::sync::atomic::Ordering;
        // A shell that warms in slices has promised to keep this thread
        // answerable, so from here on nothing may be built inside a reader's tap.
        // See `defer_builds`.
        self.defer_builds.store(true, Ordering::Relaxed);
        loop {
            let phase = self.warm_phase.load(Ordering::Relaxed);
            let more = match phase {
                0 => self.warm_search_slice(slice),
                1 => self.warm_occ_slice(slice),
                2 => self.warm_renderings_slice(slice),
                3 => self.warm_xref_slice(slice),
                4 => self.warm_concept_slice(slice),
                5 => self.warm_leitwort_slice(slice),
                6 => {
                    self.bridge();
                    0
                }
                _ => return 0,
            };
            if more == 1 {
                return 1; // same phase, more slices to feed
            }
            self.warm_phase.store(phase + 1, Ordering::Relaxed);
            // A single-shot phase did real work — yield now and come back.
            if phase >= 3 {
                return 1;
            }
        }
    }

    /// Advance the concept model by one budgeted stage-slice. 1 while work
    /// remains. The heaviest thing the warm does — twelve stages over the
    /// corpus, the co-occurrence counts, PPMI, kNN and label propagation — and
    /// as one call it blocked the worker for ~640ms.
    fn warm_concept_slice(&self, n: usize) -> i32 {
        if self.concept.get().is_some() {
            return 0;
        }
        let Ok(mut guard) = self.concept_partial.lock() else {
            return 0; // poisoned: leave it to the build-on-first-use path
        };
        let b = guard.get_or_insert_with(concept::ConceptBuilder::default);
        if b.step(&self.corpus, n) {
            return 1;
        }
        if let Some(model) = b.take() {
            let _ = self.concept.set(model);
        }
        *guard = None;
        0
    }

    /// Parse `n` more rows of the cross-reference TSV. 1 while work remains.
    ///
    /// 344k rows, 89 ms in one call on the maintainer's desktop — a phone's whole
    /// warm-chunk budget, spent with the worker unable to answer a tap. Sliced,
    /// like the three phases before it.
    fn warm_xref_slice(&self, n: usize) -> i32 {
        if self.xref_ix.get().is_some() {
            return 0;
        }
        let Ok(mut guard) = self.xref_partial.lock() else {
            return 0; // poisoned: leave it to the build-on-first-use path
        };
        let b = guard.get_or_insert_with(|| match &self.home {
            Some(h) => crossref::XRefIxBuilder::from_path(crossref::cross_refs_path(h)),
            // No home: the same empty index `xref_ix()` would have made.
            None => crossref::XRefIxBuilder::empty(),
        });
        // Rows, not verses: at ~12 references per source verse a verse-sized
        // slice would be a twelfth of the work the other phases do per call.
        if b.feed(n * 8) {
            return 1;
        }
        if let Some(b) = guard.take() {
            let _ = self.xref_ix.set(b.finish());
        }
        0
    }

    /// Advance leitwort discovery by one budgeted slice. 1 while work remains.
    ///
    /// Two cursored stages (positions over the corpus, then the burst scan over
    /// the codes) — 83 ms as a single call, measured the same day as the xref
    /// parse above.
    fn warm_leitwort_slice(&self, n: usize) -> i32 {
        if self.leitwort.get().is_some() {
            return 0;
        }
        let Ok(mut guard) = self.leitwort_partial.lock() else {
            return 0;
        };
        let b = guard.get_or_insert_with(|| burst::LeitwortBuilder::new(&burst::BurstParams::default()));
        if b.step(&self.corpus, n) {
            return 1;
        }
        if let Some(found) = b.take() {
            let _ = self.leitwort.set(found.into_iter().map(|b| (b.strongs.clone(), b)).collect());
        }
        *guard = None;
        0
    }

    /// Fold `n` more verses into the rendering lens. 1 while work remains.
    fn warm_renderings_slice(&self, n: usize) -> i32 {
        if self.renderings.get().is_some() {
            return 0;
        }
        let Ok(mut guard) = self.renderings_partial.lock() else {
            return 0;
        };
        let b = guard.get_or_insert_with(renderings::RenderingsBuilder::default);
        if b.feed(&self.corpus, n) {
            return 1;
        }
        let _ = self.renderings.set(guard.take().expect("builder present").finish());
        0
    }

    /// The rendering lens, built on first use.
    fn renderings(&self) -> &Renderings {
        self.renderings.get_or_init(|| Renderings::build(&self.corpus))
    }

    /// The TSK cross-references, parsed from the home on first use.
    fn xref_ix(&self) -> &XRefIx {
        self.xref_ix.get_or_init(|| match &self.home {
            Some(h) => crossref::load_cross_refs(crossref::cross_refs_path(h)),
            None => XRefIx::new(),
        })
    }

    /// The hymnal, parsed from the home on first use — but NEVER cached-empty
    /// (the same stance as [`Self::strongs`], for the same reason). On the web
    /// `data/hymnal.json` rides the study stage, which lands moments AFTER the
    /// engine opens; a reader who taps the hymn tab in that window must get the
    /// book on the shell's re-fetch, not an empty tab for the whole session.
    /// Unreadable data also answers empty — the ABI degrades, the pack checks
    /// catch bad data at build time.
    fn hymnal(&self) -> &hymnal::Hymnal {
        if let Some(book) = self.hymnal.get() {
            return book;
        }
        static EMPTY: OnceLock<hymnal::Hymnal> = OnceLock::new();
        let empty = || EMPTY.get_or_init(hymnal::Hymnal::default);
        let Some(h) = &self.home else { return empty() };
        match hymnal::load(h.join("data").join("hymnal.json")) {
            Ok(book) if !book.hymns.is_empty() => {
                let _ = self.hymnal.set(book);
                self.hymnal.get().expect("just set")
            }
            _ => empty(),
        }
    }

    /// The devotional catalogue, parsed from the home on first use — never
    /// cached-empty, the [`Self::hymnal`] stance and for the same reason: the
    /// file arrives with the pack, and a probe in the gap before it lands must
    /// not fix "no devotionals" for the session. A reader whose first run opens
    /// the new-believer booklet is exactly that probe.
    fn devotionals(&self) -> &[devotional::Devotional] {
        if let Some(all) = self.devotionals.get() {
            return all;
        }
        static EMPTY: OnceLock<Vec<devotional::Devotional>> = OnceLock::new();
        let empty = || EMPTY.get_or_init(Vec::new).as_slice();
        let Some(h) = &self.home else { return empty() };
        match devotional::load(h.join("data").join("devotional.json")) {
            Ok(all) if !all.is_empty() => {
                let _ = self.devotionals.set(all);
                self.devotionals.get().expect("just set")
            }
            _ => empty(),
        }
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

/// The reloadable personal study state (see [`PlumblineEngine::study`]).
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
            notes: notes::load_notes(home.join("data").join("kjv-notes.jsonl")).unwrap_or_default(),
            threads: thread::load_threads(home).0,
            tags: tag::load_tags(home).0,
            weaves: weave::load_weaves(home).0,
            user_notes: usernote::load_notes(home).0,
        },
        None => StudyData::default(),
    }
}

// The ABI promises `*const PlumblineEngine` is safe to share across threads for
// reads while authoring may happen on another thread — which is exactly
// `Send + Sync`. Fails to compile if a field ever loses that property.
fn _assert_engine_is_send_sync() {
    fn assert<T: Send + Sync>() {}
    assert::<PlumblineEngine>();
}

/// One laid-out chapter: the positioned display list a shell paints and
/// hit-tests. Opaque to C; produced by [`plumbline_engine_layout_chapter`], released
/// with [`plumbline_layout_free`].
pub struct PlumblineDisplayList {
    inner: DisplayList,
}

// ── layout config + measurement callback ──────────────────────────────────────

/// Layout parameters, all in device pixels — the C-ABI mirror of
/// `plumbline_layout::LayoutConfig` (passed by value, so it is `#[repr(C)]`).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PlumblineLayoutConfig {
    pub width: f32,
    pub line_height: f32,
    pub space_width: f32,
    pub verse_num_gap: f32,
    pub para_indent: f32,
    pub para_spacing: f32,
    /// Nonzero: start every verse on a fresh line (verse-per-line mode).
    pub verse_break: u32,
    /// Nonzero: paint the leading verse numbers (the default). Zero lays the
    /// chapter out as prose — and it is a LAYOUT input rather than something a
    /// shell can skip at paint time, because the number's width and its gap
    /// belong to the line whether or not anything is drawn in them.
    pub verse_numbers: u32,
}

impl From<PlumblineLayoutConfig> for LayoutConfig {
    fn from(c: PlumblineLayoutConfig) -> LayoutConfig {
        LayoutConfig {
            width: c.width,
            line_height: c.line_height,
            space_width: c.space_width,
            verse_num_gap: c.verse_num_gap,
            para_indent: c.para_indent,
            para_spacing: c.para_spacing,
            verse_break: c.verse_break != 0,
            verse_numbers: c.verse_numbers != 0,
            // Overwritten by the caller from the open corpus's language; the
            // ABI carries no direction. See `plumbline_engine_layout_chapter`.
            rtl: false,
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
///
/// # Contract — the config must describe the font the callback measures with
/// Widths are **memoized on this side of the ABI** (see [`font_identity`]), and
/// nothing in this ABI names a typeface. So a shell that changes the font the
/// callback measures with must also move `line_height` or `space_width` in the
/// [`PlumblineLayoutConfig`] it passes with it. Both shipped shells do so by
/// construction, because they derive those two BY MEASURING in the current font
/// — the web's `space_width` is `measure(" ")` through this very callback, and
/// Android's is `Paint.measureText("n n") − measureText("nn")` off the same Paint,
/// with `line_height` coming from that font's own metrics. A shell that switched
/// typeface while holding both bit-identical would be handed the previous
/// typeface's widths: a mis-laid-out chapter.
pub type PlumblineMeasureFn = Option<extern "C" fn(ctx: *mut c_void, text: *const c_char) -> f32>;

/// Adapts a C measurement callback to the [`Measure`] trait the layout wants.
/// Wrapped in a [`Memoized`] before it reaches the layout, so only text this
/// engine has never measured in this font actually crosses back out.
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

/// The identity of the font the shell is measuring with, as far as this ABI can
/// see it — the key the width memo is held under. Two layouts that agree on it
/// may share remembered widths; anything else re-measures.
///
/// Nothing in this ABI names a typeface or a size, so the identity is the only
/// two config fields a shell derives FROM the font it measures with:
///
///  - `space_width` — both shells obtain it by measuring in the current font, the
///    web literally by calling the measure callback with `" "` at every layout, so
///    it is a live probe of the text stack rather than a stored setting. That is
///    what makes it notice a webfont that finished loading between two layouts.
///  - `line_height` — that font's own extent times the reader's line spacing.
///
/// Deliberately NOT in it:
///
///  - The **callback's own address**. JNA allocates a native trampoline per
///    `Callback` instance and `StudyEngine.LayoutChapter` builds a fresh
///    `MeasureCallback` inside every call, so on Android that address changes from
///    one chapter turn to the next (and can be reused after GC) — keying on it
///    would empty the memo on every turn of the gold-standard shell. `measure_ctx`
///    is null on Android and 0 on the web, so it discriminates nothing either.
///  - `width`, `verse_break` and `verse_numbers`, none of which can change a
///    glyph's advance — they move boxes around, and a box is measured the same
///    wherever it lands. Leaving them out is what makes a rotation, a margin
///    drag, a verse-per-line toggle or turning the numbers off re-lay out the
///    chapter with **zero** crossings — the case the memo exists for.
///    `para_indent`, `para_spacing` and `verse_num_gap` are arithmetic on the
///    two fields above and would add nothing.
///  - The AKJV overlay. It changes the TEXT, and the text is the memo's key, so a
///    re-worded verse simply misses.
fn font_identity(cfg: &PlumblineLayoutConfig) -> u64 {
    // Packed rather than hashed: two f32s fit a u64 exactly, so this identity is
    // lossless and there is no collision to reason about. Bit equality is the
    // right test — a value that differs at all was computed from different font
    // state, and the cost of an unnecessary clear is one slow layout.
    ((cfg.line_height.to_bits() as u64) << 32) | cfg.space_width.to_bits() as u64
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

/// The engine's own UTC stamp, in the frozen wire form
/// `YYYY-MM-DDThh:mm:ssZ` — for the mutations whose shell caller sends none.
///
/// **This is the only clock in the product's Rust.** The core is pure and takes
/// every timestamp from its caller (`crates/core/src/civil.rs` says so), which is
/// what keeps its tests deterministic; the shells send one for the authoring
/// calls that CREATE something (`added`). But `updated` (docs/STABLE-IDS.md) has
/// to move on every mutating save, including the several that carry no stamp at
/// all — setting a thread's notes, clearing an entry's note, dropping a tag
/// member — and an `updated` that only sometimes moves is worse than none: a
/// future importer choosing between two copies would trust a stale one.
///
/// So the clock sits here, at the edge that already owns files and handles,
/// rather than in the core or in seven new ABI parameters. Both shipped targets
/// have a real one: Android natively, and the browser's WASI shim answers
/// `clock_time_get(CLOCKID_REALTIME)` from `Date`. A clock that somehow reads
/// before the epoch yields the epoch rather than a negative stamp.
fn now_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    plumbline_core::civil::stamp_from_epoch_secs(secs)
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

/// The Plumbline core version as a caller-freed NUL-terminated UTF-8 string.
/// Never null.
#[no_mangle]
pub extern "C" fn plumbline_version() -> *mut c_char {
    out_string(env!("CARGO_PKG_VERSION").to_string())
}

/// Free a string previously returned by any `plumbline_*` function. Null is a no-op.
///
/// # Safety
/// `ptr` must be a pointer returned by this library and not already freed.
#[no_mangle]
pub unsafe extern "C" fn plumbline_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

// ── engine lifecycle ────────────────────────────────────────────────────────────

/// The corpus file for a language, under `home` — the path, not a promise that
/// it is there.
///
/// One place, so the engine, the hydrator and any tool agree. The file comes
/// from the language's row in the registry (`plumbline_core::i18n`) — English
/// `data/kjv.jsonl`, German `data/luther1912.jsonl`, Spanish `data/rv1909.jsonl`
/// (`data-prep/README.md`). Whether it can actually be opened is
/// [`open_corpus`]'s question.
pub fn corpus_for(home: &str, lang: i18n::Lang) -> PathBuf {
    PathBuf::from(home).join("data").join(lang.corpus().file)
}

/// The Strong's dictionary for the active language: the localized one when the
/// language's row names it and the pack shipped it (machine-translated
/// definitions + renderings derived from that language's own tagged corpus —
/// BIBLIOGRAPHY.md), else the English source. Same file shape either way; the
/// bool says which, so the panel can label the renderings for the right Bible
/// and carry the machine-translation caveat.
fn strongs_for(data: &std::path::Path) -> (PathBuf, bool) {
    strongs_for_lang(data, i18n::active())
}

/// [`strongs_for`] for an EXPLICIT language — what a second engine opened on
/// another text needs, since the active language is the UI's and a pane's text
/// language is now its own (see [`plumbline_engine_open_lang`]).
///
/// `localizedLexiconOff` still applies: it is a preference about DEFINITIONS
/// ("give me Strong's own English"), not about which Bible is on screen, so it
/// holds for every pane the reader opens.
fn strongs_for_lang(data: &std::path::Path, lang: i18n::Lang) -> (PathBuf, bool) {
    let base = data.join(i18n::Lang::En.spec().lexicon.map(|l| l.file).unwrap_or("strongs.json"));
    // Read here, at pick time, because the toggle reloads the app exactly like a
    // language change does.
    if !config::load().0.localized_lexicon_off {
        if let Some(lex) = lang.spec().lexicon {
            let local = data.join(lex.file);
            if local != base && local.exists() {
                return (local, true);
            }
        }
    }
    (base, false)
}

/// The corpus for `lang`, falling back to the KJV when it will not open.
///
/// BY TRYING, not by testing for the file, and the difference is not academic:
/// the web's home is a WASI shim over an in-memory tree where `Path::exists` does
/// not answer usefully, so an existence check silently sent every German reader
/// to the English text (caught by e2e/language.spec.ts). "Can this be opened" is
/// also the question actually being asked.
///
/// The fallback is the ordinary case rather than an error path: the German text
/// is an optional download, so a reader who switches language before fetching it
/// gets a German interface over the English text and a Bible either way.
fn open_corpus(home: &str, lang: i18n::Lang) -> Result<Corpus, plumbline_core::Error> {
    let path = corpus_for(home, lang);
    match corpus::load_corpus(&path) {
        Ok(c) => Ok(c),
        Err(e) if lang != i18n::Lang::En => {
            // The reader is owed a Bible, not a diagnosis — but somebody
            // debugging a device deserves to know which text they are looking at
            // and why it is not the one they asked for.
            eprintln!("plumbline: {} unavailable ({e}); opening the KJV", path.display());
            corpus::load_corpus(corpus_for(home, i18n::Lang::En))
        }
        Err(e) => Err(e),
    }
}

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
pub unsafe extern "C" fn plumbline_engine_open(home: *const c_char, out_err: *mut *mut c_char) -> *mut PlumblineEngine {
    guard(ptr::null_mut(), || {
        if !out_err.is_null() {
            *out_err = ptr::null_mut();
        }
        let Some(home) = opt_str(home) else {
            set_err(out_err, "home path is null or not valid UTF-8".into());
            return ptr::null_mut();
        };
        // WHICH TEXT. The reader's language decides, and the language was set by
        // `plumbline_i18n_set_language` before this call — both shells do that in
        // their startup, before anything reads a book name.
        //
        // FALLS BACK TO THE KJV when the German corpus is not on the device, and
        // that is the common case rather than an error: the German text is an
        // optional download, so a reader who has switched language but not yet
        // fetched it gets a German interface over the English text instead of a
        // dead app. The shell offers the download; nothing here fails.
        let corpus = match open_corpus(home, i18n::active()) {
            Ok(c) => c,
            Err(e) => {
                set_err(out_err, e.to_string());
                return ptr::null_mut();
            }
        };
        // Stage-1 boots (web) may not have strongs.json yet — open on the
        // corpus alone; load_core_data brings the dictionary in later. A file
        // that EXISTS but fails to parse stays a hard error. A German reader
        // gets strongs-de.json when the pack ships it (`strongs_for`).
        let (strongs_path, strongs_is_localized) = strongs_for(&PathBuf::from(home).join("data"));
        let strongs = if strongs_path.exists() {
            match strongs::load_strongs(&strongs_path) {
                Ok(s) => Some(s),
                Err(e) => {
                    set_err(out_err, e.to_string());
                    return ptr::null_mut();
                }
            }
        } else {
            None
        };
        let engine = PlumblineEngine::new(corpus, strongs, Some(PathBuf::from(home)));
        engine.strongs_localized.store(strongs_is_localized, std::sync::atomic::Ordering::Relaxed);
        Box::into_raw(Box::new(engine))
    })
}

/// Open a SECOND engine on a named language's text — what a per-pane language
/// rides on: German beside English, without the UI language moving.
///
/// The same home, so the reader's own data (threads, tags, weaves, notes) is
/// the SAME data — every text sits at the KJV's verse addresses, so a refKey
/// means one verse in all of them and nothing needs mapping. After an authoring
/// write, call [`plumbline_engine_load_core_data`] on this handle too, or its
/// study view stays as it was when it opened.
///
/// TWO DIFFERENCES from [`plumbline_engine_open`], both deliberate:
///
/// 1. The language is a PARAMETER, not the global the UI language lives in.
/// 2. There is NO English fallback. `plumbline_engine_open` falls back because
///    a reader is owed a Bible; here the caller asked for one specific text to
///    put beside another, and quietly handing back the one already on screen
///    would paint English under a pane labelled Deutsch. A missing text is an
///    error the shell can act on — it is the shell that offers the download.
///
/// Returns null on failure (unknown language code, or the text is not on the
/// device); `out_err` behaves as in [`plumbline_engine_open`]. Free it with
/// [`plumbline_engine_free`] like any other engine.
///
/// # Safety
/// `home` and `lang` are valid NUL-terminated UTF-8; `out_err` is null or a
/// writable slot for one `*mut c_char`.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_open_lang(
    home: *const c_char,
    lang: *const c_char,
    out_err: *mut *mut c_char,
) -> *mut PlumblineEngine {
    guard(ptr::null_mut(), || {
        if !out_err.is_null() {
            *out_err = ptr::null_mut();
        }
        let (Some(home), Some(code)) = (opt_str(home), opt_str(lang)) else {
            set_err(out_err, "home path or language is null or not valid UTF-8".into());
            return ptr::null_mut();
        };
        // STRICT: `Lang::parse` reads an unknown code as English, which is right
        // for a UI language and wrong here for the same reason the fallback is.
        let Some(lang) = i18n::Lang::shipped(code) else {
            set_err(out_err, format!("this build does not ship the language `{code}`"));
            return ptr::null_mut();
        };
        let corpus = match corpus::load_corpus(corpus_for(home, lang)) {
            Ok(c) => c,
            Err(e) => {
                set_err(out_err, e.to_string());
                return ptr::null_mut();
            }
        };
        let data = PathBuf::from(home).join("data");
        let (strongs_path, strongs_is_localized) = strongs_for_lang(&data, lang);
        let strongs = if strongs_path.exists() {
            match strongs::load_strongs(&strongs_path) {
                Ok(s) => Some(s),
                Err(e) => {
                    set_err(out_err, e.to_string());
                    return ptr::null_mut();
                }
            }
        } else {
            None
        };
        let engine = PlumblineEngine::new(corpus, strongs, Some(PathBuf::from(home)));
        engine.strongs_localized.store(strongs_is_localized, std::sync::atomic::Ordering::Relaxed);
        Box::into_raw(Box::new(engine))
    })
}

/// Open an engine from in-memory bytes — for shells that bundle the data as
/// assets/resources (decision #3): the `kjv.jsonl` text and the `strongs.json`
/// object, each as a length-delimited byte buffer (need not be NUL-terminated).
///
/// Returns null on failure; `out_err` behaves as in [`plumbline_engine_open`].
///
/// # Safety
/// Each `*_ptr`/`*_len` pair describes a readable buffer of that length (a null
/// pointer with length 0 is treated as empty and will error); `out_err` is null
/// or a writable slot.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_open_from_bytes(
    kjv_ptr: *const u8,
    kjv_len: usize,
    strongs_ptr: *const u8,
    strongs_len: usize,
    out_err: *mut *mut c_char,
) -> *mut PlumblineEngine {
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
        Box::into_raw(Box::new(PlumblineEngine::new(corpus, Some(strongs), None)))
    })
}

/// Release an engine. Null is a no-op.
///
/// # Safety
/// `engine` must be a pointer from `plumbline_engine_open*` and not already freed.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_free(engine: *mut PlumblineEngine) {
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
pub unsafe extern "C" fn plumbline_engine_toc_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(engine) = engine.as_ref() else {
            return ptr::null_mut();
        };
        let books: Vec<wire::TocBook> = canon::BOOKS
            .iter()
            .map(|b| wire::TocBook {
                id: b.id,
                name: i18n::book_name(i18n::active(), b.id),
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
pub unsafe extern "C" fn plumbline_engine_chapter_count(engine: *const PlumblineEngine, book: *const c_char) -> u32 {
    guard(0, || match (engine.as_ref(), opt_str(book)) {
        (Some(engine), Some(book)) => engine.corpus.chapter_count(book) as u32,
        _ => 0,
    })
}

/// The highest verse number in `book` chapter `chapter` — how many verses a
/// shell may offer for that chapter (the passage-memorize picker's range).
/// 0 for a null engine or a chapter the corpus lacks.
///
/// # Safety
/// `engine` is valid; `book` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_chapter_verse_count(
    engine: *const PlumblineEngine,
    book: *const c_char,
    chapter: u32,
) -> u32 {
    guard(0, || match (engine.as_ref(), opt_str(book)) {
        (Some(engine), Some(book)) => engine
            .corpus
            .chapter_verses(book, chapter.min(u16::MAX as u32) as u16)
            .iter()
            .map(|v| v.verse as u32)
            .max()
            .unwrap_or(0),
        _ => 0,
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
pub unsafe extern "C" fn plumbline_engine_verse_json(
    engine: *const PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_token_json(
    engine: *const PlumblineEngine,
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
        match engine.corpus.verse(&vref).and_then(|v| v.tokens.get(token_index as usize)) {
            Some(t) => out_json(&wire::token_to_wire(t)),
            None => ptr::null_mut(),
        }
    })
}

// ── the plain-English overlay ────────────────────────────────────────────────────

/// Switch the AKJV overlay on or off for this engine. Off by default: the text
/// is the KJV, and the overlay is a reading aid the reader opts into.
///
/// Affects the READER only. Memory cards, Present, copied text and shared links
/// are the KJV whatever this says — a modernised word must never end up on
/// someone's memory card or in a hand-off, or the overlay has quietly become a
/// second translation.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_set_akjv_overlay(engine: *const PlumblineEngine, on: bool) {
    if let Some(e) = engine.as_ref() {
        e.akjv_on.store(on, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Whether this home actually carries a usable overlay — a shell hides the
/// toggle when it doesn't, rather than offering a switch that does nothing.
/// False until the stage-2 load has run.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_akjv_available(engine: *const PlumblineEngine) -> bool {
    engine.as_ref().is_some_and(|e| e.akjv().is_some())
}

/// What the AKJV does to one token, as `{"akjv":"you shall","kjv":"thou shalt"}`
/// — the line a word study shows under the headword. Null when the token is not
/// re-rendered, or on a bad ref. `kjv` is the run's ORIGINAL words, which is the
/// whole point: the reader can always see what was replaced.
///
/// # Safety
/// `engine` is valid; `ref_key` is null or a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_akjv_token_json(
    engine: *const PlumblineEngine,
    ref_key: *const c_char,
    token_index: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let (Some(a), Some(v)) = (e.akjv(), VRef::parse_ref_key(rk)) else {
            return ptr::null_mut();
        };
        let Ok(tok) = u16::try_from(token_index) else { return ptr::null_mut() };
        let Some(span) = a.span_at(&v, tok) else { return ptr::null_mut() };
        let Some(verse) = e.corpus.verse(&v) else { return ptr::null_mut() };
        let kjv = plumbline_core::corpus::render_tokens(
            verse.tokens.iter().take(span.end as usize + 1).skip(span.start as usize),
        );
        out_json(&wire::AkjvTokenWire { akjv: span.text.clone(), kjv: kjv.trim().to_string() })
    })
}

// ── layout + hit-testing ─────────────────────────────────────────────────────────

/// Lay out a chapter into a display list, measuring text through `measure`
/// (called with `measure_ctx`). Returns an opaque handle to release with
/// [`plumbline_layout_free`], or null on a null engine, a null callback, or an
/// unknown/out-of-range book+chapter (no such verses). Because the KJV has no
/// empty chapters, a null return reliably means "past the end" — a shell can
/// page by advancing until it gets null.
///
/// # Safety
/// `engine` is valid; `book` is a valid NUL-terminated UTF-8 string; `measure`
/// is a valid function pointer for the call and `measure_ctx` is whatever it
/// expects (it is passed back verbatim).
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_layout_chapter(
    engine: *const PlumblineEngine,
    book: *const c_char,
    chapter: u32,
    cfg: PlumblineLayoutConfig,
    measure: PlumblineMeasureFn,
    measure_ctx: *mut c_void,
) -> *mut PlumblineDisplayList {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(book), Some(measure)) = (engine.as_ref(), opt_str(book), measure) else {
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
        let shell = FfiMeasure { f: measure, ctx: measure_ctx };
        // The overlay is applied HERE, on the way into the layout, so the
        // corpus itself is never touched and a verse the AKJV leaves alone
        // costs nothing (`overlay_verse` returns None and the original is laid
        // out in place).
        let overlaid: Vec<plumbline_core::corpus::Verse>;
        let verses = match engine.akjv_view() {
            Some(a) => {
                overlaid = verses.iter().map(|v| a.overlay_verse(v).unwrap_or_else(|| v.clone())).collect();
                &overlaid[..]
            }
            None => verses,
        };
        // Measure through the engine's memo. Counted from `data/kjv.jsonl`:
        // 58% of a cold chapter's measurements are of a run this
        // layout already measured (Gen 1: 828 runs, 229 distinct), 84% over twenty
        // consecutive chapters through one memo, and a re-layout at the same font
        // measures nothing at all. It sits below the ABI, so Android's JNA upcalls
        // and the web's wasm→JS crossings both shrink from one implementation —
        // and the web's `measureCalls()` diagnostic stays honest, because a memo
        // hit never reaches the callback that increments it.
        let m = Memoized::new(&engine.measure_memo, font_identity(&cfg), &shell);
        // DIRECTION IS DERIVED FROM THE OPEN TEXT, not taken from the shell.
        //
        // It could have been a field on `PlumblineLayoutConfig`, and that would
        // be an ABI break for a fact neither shell is in a position to know
        // better than the engine: the corpus's own tokenization stamp names its
        // language, and the language's row says which way it reads. A shell
        // that passed direction could disagree with the text it is showing —
        // which is exactly what happens to a reader whose Arabic download has
        // not landed yet and who is therefore looking at the KJV.
        //
        // Not part of `font_identity`, deliberately: the memo caches WIDTHS,
        // and a word is the same width whichever way the line runs. The mirror
        // happens after every measurement is in.
        let mut layout = LayoutConfig::from(cfg);
        layout.rtl = plumbline_core::i18n::Lang::for_tokenization(engine.corpus.tokenization_version())
            .is_some_and(|l| l.is_rtl());
        let dl = layout_chapter(verses, &m, &layout);
        Box::into_raw(Box::new(PlumblineDisplayList { inner: dl }))
    })
}

/// The full display list as JSON (see [`wire`] for the schema): positioned
/// items plus the total painted `width`/`height`. Caller-freed; null on a null
/// handle.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn plumbline_layout_to_json(dl: *const PlumblineDisplayList) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_layout_height(dl: *const PlumblineDisplayList) -> f32 {
    guard(0.0, || dl.as_ref().map(|d| d.inner.height).unwrap_or(0.0))
}

/// The column width the layout targeted. 0 on a null handle.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn plumbline_layout_width(dl: *const PlumblineDisplayList) -> f32 {
    guard(0.0, || dl.as_ref().map(|d| d.inner.width).unwrap_or(0.0))
}

/// Number of placed items in the display list. 0 on a null handle.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn plumbline_layout_item_count(dl: *const PlumblineDisplayList) -> u32 {
    guard(0, || dl.as_ref().map(|d| d.inner.items.len() as u32).unwrap_or(0))
}

/// Resolve a point (in the display list's own coordinate space) to the word
/// under it. Returns a `Hit` JSON (`{"verse","display","tokenIndex","strongs"}`)
/// or null when the point hits a verse number, a gap, or nothing. Caller-freed.
///
/// # Safety
/// `dl` is a valid display-list pointer.
#[no_mangle]
pub unsafe extern "C" fn plumbline_layout_hit_test_json(
    dl: *const PlumblineDisplayList,
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
/// `dl` must be a pointer from [`plumbline_engine_layout_chapter`] and not already
/// freed.
#[no_mangle]
pub unsafe extern "C" fn plumbline_layout_free(dl: *mut PlumblineDisplayList) {
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
pub unsafe extern "C" fn plumbline_engine_strongs_json(
    engine: *const PlumblineEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        match engine.strongs().get(code) {
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
pub unsafe extern "C" fn plumbline_engine_strongs_occurrences_json(
    engine: *const PlumblineEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        // Not ready under a sliced warm: null, and the shell re-asks on warmReady.
        let Some(occ) = engine.occ_ix_ready() else {
            return ptr::null_mut();
        };
        let all = occ.verses(code);
        let total = all.len();
        let verses: Vec<String> = all.iter().take(OCCURRENCE_CAP).map(|v| v.ref_key()).collect();
        out_json(&wire::Occurrences { code: code.to_string(), total, capped: total > verses.len(), verses })
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
pub unsafe extern "C" fn plumbline_engine_renderings_json(
    engine: *const PlumblineEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let renderings = engine
            .renderings()
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
                wire::WireRendering { rendering: r.label.to_string(), total, capped: total > refs.len(), refs }
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
pub unsafe extern "C" fn plumbline_engine_word_codes_json(
    engine: *const PlumblineEngine,
    word: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(word)) = (engine.as_ref(), opt_str(word)) else {
            return ptr::null_mut();
        };
        let codes = engine
            .renderings()
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
pub unsafe extern "C" fn plumbline_engine_search_json(
    engine: *const PlumblineEngine,
    query: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(query)) = (engine.as_ref(), opt_str(query)) else {
            return ptr::null_mut();
        };
        let study = engine.study_read();
        match search::run_search(&engine.corpus, &study.notes, engine.search_ix(), query) {
            Some(answer) => out_json(&wire::search_to_wire(&answer)),
            None => ptr::null_mut(),
        }
    })
}

/// [`plumbline_engine_search_json`] narrowed to a scope — the search screen's
/// chips. `scope` is `all` | `ot` | `nt` | `book:<osis>` |
/// `chapter:<osis>:<ch>`; anything else (or null) searches everything.
///
/// A REFERENCE query still answers `goto` whatever the scope: the reader typed
/// an address, and a chip must not refuse to take them there.
///
/// # Safety
/// `engine` is valid; `query` is a valid NUL-terminated UTF-8 string; `scope`
/// is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_search_scoped_json(
    engine: *const PlumblineEngine,
    query: *const c_char,
    scope: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(engine), Some(query)) = (engine.as_ref(), opt_str(query)) else {
            return ptr::null_mut();
        };
        let study = engine.study_read();
        let scope = opt_str(scope).and_then(search::SearchScope::parse).unwrap_or_default();
        match search::run_search_scoped(&engine.corpus, &study.notes, engine.search_ix(), query, &scope) {
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
pub unsafe extern "C" fn plumbline_engine_threads_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_tags_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_verse_xrefs_json(
    engine: *const PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_suggested_weaves_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::suggested_weaves_to_wire(&e.study_read().weaves)),
        None => ptr::null_mut(),
    })
}

// ── R&D tier: read (morphology, fused bridge) ────────────────────────────────
//
// These consume the offline artifacts loaded at open (see `data-prep`). Each
// returns null when its artifact is absent (or the engine/ref is invalid), so a
// shell shows the section exactly when it exists — no training happens here.

/// The fused OT↔NT bridge partners of a Strong's code as JSON:
/// `{"code","partners":[{code,sources,prior}]}`, ranked by trust prior. The
/// etymology layer works from the dictionary alone, so this is available even
/// for a bytes-opened engine (external witnesses need a home). Null on a null
/// engine / invalid code.
///
/// # Safety
/// `engine` is valid; `code` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_bridge_partners_json(
    engine: *const PlumblineEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let partners = e
            .bridge()
            .map(|b| b.partners(code))
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                // Authority provenance, classified once here (overlay `Tier`):
                // the additive tier set + research-grade flag travel with each
                // partner so non-Rust shells need not reimplement the mapping.
                let tiers = bridge::tiers_of(&p.sources).into_iter().map(|t| t.wire_name().to_string()).collect();
                let research_grade = p.sources.iter().any(|s| bridge::research_grade(s));
                wire::WireBridgePartner { code: p.code, sources: p.sources, prior: p.prior, tiers, research_grade }
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
pub unsafe extern "C" fn plumbline_engine_morph_json(
    engine: *const PlumblineEngine,
    ref_key: *const c_char,
    token_index: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let (Some(md), Some(vref)) = (e.morph.get(), VRef::parse_ref_key(rk)) else {
            return ptr::null_mut();
        };
        let Some(entry) = md.entries(&vref).iter().find(|en| en.tok == token_index) else {
            return ptr::null_mut();
        };
        let Some(gloss) = md.gloss(&vref, token_index) else { return ptr::null_mut() };
        out_json(&wire::WireMorph { verse: vref.ref_key(), token_index, code: entry.code.clone(), gloss })
    })
}

// ── study data: authoring (write) ──────────────────────────────────────────────
//
// These mutate on-disk study data through the cross-platform `core::store`
// atomic writer, then reload the engine's in-memory copies. Each returns **null
// on success** and an owned error string on failure (free it with
// `plumbline_string_free`). All require an engine opened from a home directory
// (`plumbline_engine_open`); an engine opened from bytes returns an error.

/// Add the whole verse `ref_key` to the thread named `name` (created on first
/// use). `note` may be null; `added` is a caller-supplied UTC timestamp.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_thread_add(
    engine: *mut PlumblineEngine,
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
        let (Some(name), Some(rk), Some(added)) = (opt_str(name), opt_str(ref_key), opt_str(added)) else {
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
        let entry = ThreadEntry { vref, span, text, note: opt_str(note).map(str::to_string), added: added.to_string() };
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

/// Delete the thread named `name` — its file and every entry on it. Matched
/// case-insensitively, like `plumbline_engine_thread_add`. A name with no thread
/// is a success (the caller wanted it gone; it is gone). Null on success, else an
/// owned error string.
///
/// # Safety
/// `engine` is valid; `name` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_thread_remove(
    engine: *mut PlumblineEngine,
    name: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let Some(name) = opt_str(name) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match thread::remove_thread(&study.threads, name) {
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
pub unsafe extern "C" fn plumbline_engine_tag_add(
    engine: *mut PlumblineEngine,
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

/// Remove a target (see [`plumbline_engine_tag_add`] for `kind`/`value`) from the tag
/// named `name`. A missing target is a no-op; a missing tag is an error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_tag_remove(
    engine: *mut PlumblineEngine,
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
            Some(lt) => match tag::remove_member(&lt, &target, &now_stamp()) {
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

/// Delete the whole tag named `name` — its file and every member on it. Matched
/// case-insensitively, like `plumbline_engine_tag_add`. A name with no tag is a
/// success (the caller wanted it gone; it is gone). The members' verses are the
/// canon's, not the tag's — nothing else is touched. Null on success, else an
/// owned error string.
///
/// # Safety
/// `engine` is valid; `name` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_tag_delete(engine: *mut PlumblineEngine, name: *const c_char) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let Some(name) = opt_str(name) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match tag::remove_tag(&study.tags, name) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Rename the tag `from` to `to`, KEEPING ITS IDENTITY. Matched
/// case-insensitively, like the other tag calls. A change of case only is a
/// legal rename onto itself.
///
/// Refuses a blank new name, and refuses a name another tag already answers to —
/// that is a MERGE, which is destructive and has to be asked for by name
/// (`plumbline_engine_tag_merge`) rather than fallen into because two names
/// collided. A `from` that names no tag is a success with nothing done.
///
/// Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; `from`/`to` are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_tag_rename(
    engine: *mut PlumblineEngine,
    from: *const c_char,
    to: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(from), Some(to)) = (opt_str(from), opt_str(to)) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match tag::rename_tag(&home, &study.tags, from, to, &now_stamp()) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Set or clear the tag's CATEGORY — the grouping heading the tag lists file it
/// under. An empty (or blank) `category` clears it. The management screen's
/// verb: nothing on the reading path calls this. A `name` that answers to no
/// tag is a success with nothing done, and setting the category a tag already
/// carries writes nothing.
///
/// Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; `name`/`category` are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_tag_set_category(
    engine: *mut PlumblineEngine,
    name: *const c_char,
    category: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(name), Some(category)) = (opt_str(name), opt_str(category)) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match tag::set_tag_category(&study.tags, name, Some(category), &now_stamp()) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Fold the tag `from` into the tag `into`, then delete `from`. Members already
/// in `into` are not duplicated, and the SURVIVOR's copy of a shared member wins
/// — letting the source overwrite would discard a note the reader wrote on the
/// tag they chose to keep.
///
/// DESTRUCTIVE: the source tag's file is removed. Refuses a merge of a tag into
/// itself (source and destination would be one file, written and then deleted)
/// and refuses a name that no tag answers to.
///
/// Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; `from`/`into` are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_tag_merge(
    engine: *mut PlumblineEngine,
    from: *const c_char,
    into: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let (Some(from), Some(into)) = (opt_str(from), opt_str(into)) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match tag::merge_tags(&study.tags, from, into, &now_stamp()) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Weave the two whole verses `a_ref` / `b_ref` into the weave named `name`
/// (created on first use). `added` is a caller-supplied UTC timestamp.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_weave_add_link(
    engine: *mut PlumblineEngine,
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

/// Weave a tag's passages into a canon-ordered **chain** of links — the
/// accumulate-then-organize flow: the reader tags a topic over time (e.g.
/// "Rapture"), then turns the tag — or a chosen subset of its members — into a
/// weave to read as one thread through the canon. Re-running after the tag
/// grows just adds the new edges (find-or-create + link dedup).
///
/// `refs_json` is null to take every verse member, else a JSON array of
/// refKeys selecting a subset (non-members are ignored). `weave_name` is null
/// to reuse the tag's name. Returns null on success, else a caller-freed
/// error string.
///
/// # Safety
/// `engine` is a valid engine from `plumbline_engine_open*`; string params are null
/// or valid NUL-terminated UTF-8 for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_weave_from_tag(
    engine: *mut PlumblineEngine,
    tag_name: *const c_char,
    refs_json: *const c_char,
    weave_name: *const c_char,
    added: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(tag_name), Some(added)) = (opt_str(tag_name), opt_str(added)) else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        let wanted = tag_name.trim().to_lowercase();
        let Some(lt) = study.tags.iter().find(|lt| lt.tag.name.to_lowercase() == wanted) else {
            return out_string(format!("no tag named \"{tag_name}\""));
        };
        // Verse members only — a concept member has no place in a chain.
        let mut refs: Vec<VRef> = lt
            .tag
            .members
            .iter()
            .filter_map(|m| match &m.target {
                tag::TagTarget::Verse(v) => Some(v.clone()),
                _ => None,
            })
            .collect();
        if let Some(subset) = opt_str(refs_json) {
            let keys: Vec<String> = match serde_json::from_str(subset) {
                Ok(k) => k,
                Err(e) => return out_string(format!("bad refs JSON: {e}")),
            };
            let set: std::collections::BTreeSet<String> = keys.into_iter().collect();
            refs.retain(|v| set.contains(&v.ref_key()));
        }
        let name = opt_str(weave_name).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&lt.tag.name).to_string();
        match weave::add_chain(
            &home,
            &study.weaves,
            &name,
            WeaveKind::Typological,
            canon::TOKENIZATION_VERSION,
            added,
            &refs,
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
/// `plumbline_engine_suggested_weaves_json`) into the engine's flat weave list.
fn nth_suggested(weaves: &[LoadedWeave], index: usize) -> Option<usize> {
    weaves.iter().enumerate().filter(|(_, lw)| weave::is_suggested(lw)).nth(index).map(|(i, _)| i)
}

/// **Approve** the `index`-th suggested weave: promote it into `home/weaves`
/// with all links approved (merging into a same-named weave there if present)
/// and remove the suggestion. `index` is the ordinal from
/// `plumbline_engine_suggested_weaves_json`. Null on success, else an owned error.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_weave_approve(engine: *mut PlumblineEngine, index: u32) -> *mut c_char {
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
        match weave::approve_weave(&home, &study.weaves[i], &now_stamp()) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// **Reject** the `index`-th suggested weave: delete its file. `index` is the
/// ordinal from `plumbline_engine_suggested_weaves_json`. Null on success, else an
/// owned error.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_weave_reject(engine: *mut PlumblineEngine, index: u32) -> *mut c_char {
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

/// **Delete** the `index`-th weave in the library — its file and every link on
/// it. `index` is the flat-library ordinal (`plumbline_engine_weaves_json`, the
/// `weave:i` link verb) — NOT the suggested ordinal `plumbline_engine_weave_reject`
/// takes. It reaches a suggestion too: deleting one is the same act as
/// rejecting it. Null on success, else an owned error.
///
/// # Safety
/// `engine` is a valid engine pointer.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_weave_delete(engine: *mut PlumblineEngine, index: u32) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let mut study = engine.study_write();
        let Some(lw) = study.weaves.get(index as usize) else {
            return out_string(format!("no weave at index {index}"));
        };
        match weave::reject_weave(lw) {
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
pub unsafe extern "C" fn plumbline_engine_thread_set_notes(
    engine: *mut PlumblineEngine,
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
        match thread::set_thread_notes(&study.threads, name, notes, &now_stamp()) {
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
pub unsafe extern "C" fn plumbline_engine_thread_entry_set_note(
    engine: *mut PlumblineEngine,
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
        match thread::set_entry_note(
            &study.threads,
            name,
            index as usize,
            opt_str(note).map(str::to_string),
            &now_stamp(),
        ) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Drop entry `index` from the thread named `name`. Null on success, else an
/// owned error. The thread SURVIVES its last entry — deleting the thread itself
/// is [`plumbline_engine_thread_remove`], asked for deliberately.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_thread_entry_remove(
    engine: *mut PlumblineEngine,
    name: *const c_char,
    index: u32,
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
        match thread::remove_from_thread(&study.threads, name, index as usize, &now_stamp()) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Move entry `from` to position `to` in the thread named `name`. Null on
/// success, else an owned error.
///
/// A thread's ORDER is the argument it makes, so this is a reorder rather than
/// a sort. `to` past the end clamps to the last position, so "move the last one
/// down" is a no-op instead of an error the shell has to special-case — and a
/// no-op does not rewrite the file.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_thread_entry_move(
    engine: *mut PlumblineEngine,
    name: *const c_char,
    from: u32,
    to: u32,
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
        match thread::move_in_thread(&study.threads, name, from as usize, to as usize, &now_stamp()) {
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
pub unsafe extern "C" fn plumbline_engine_weave_set_notes(
    engine: *mut PlumblineEngine,
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
        match weave::set_weave_notes(&study.weaves, name, notes, &now_stamp()) {
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
pub unsafe extern "C" fn plumbline_engine_verse_notes_json(
    engine: *const PlumblineEngine,
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
            Some(ns) if !ns.is_empty() => out_json(&wire::WireVerseNotes { verse: vref.ref_key(), notes: ns.clone() }),
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
pub unsafe extern "C" fn plumbline_engine_study_xrefs_json(
    engine: *const PlumblineEngine,
    ref_key: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let Some(vref) = VRef::parse_ref_key(rk) else {
            return ptr::null_mut();
        };
        match e.xref_ix_ready().and_then(|ix| ix.get(&vref)) {
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
pub unsafe extern "C" fn plumbline_engine_weaves_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_link_pairs_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_canon_segments_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(_) => out_json(&wire::canon_segments_to_wire()),
        None => ptr::null_mut(),
    })
}

/// The hymnal's table of contents, in book-number order:
/// `{"hymns":[{id,number,titles,firstLines,tune,meter}]}` — `titles` and
/// `firstLines` map language code → string for every language the hymn ships
/// in. Empty `hymns` when the home carries no `data/hymnal.json` (an old pack);
/// null only on a null engine. Caller-freed.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_hymnal_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || match engine.as_ref() {
        Some(e) => out_json(&wire::hymnal_to_wire(e.hymnal())),
        None => ptr::null_mut(),
    })
}

/// One hymn by id, its chords transposed by `transpose` semitones and split
/// into painted `parts` (chord? + text) per line. `transposedKey` is what a
/// transpose control displays; chords are spelled for the key they LAND in.
/// `transpose` is folded into one octave (-11..=11 effective). Null for an
/// unknown id. Caller-freed.
///
/// # Safety
/// `engine` is a live engine; `id` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_hymn_json(
    engine: *const PlumblineEngine,
    id: *const c_char,
    transpose: i32,
) -> *mut c_char {
    guard(ptr::null_mut(), || match (engine.as_ref(), opt_str(id)) {
        (Some(e), Some(id)) => match e.hymnal().get(id) {
            Some(h) => out_json(&wire::hymn_to_wire(h, transpose % 12)),
            None => ptr::null_mut(),
        },
        _ => ptr::null_mut(),
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
pub unsafe extern "C" fn plumbline_engine_chord_map_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_constellation_json(
    engine: *const PlumblineEngine,
    page: u32,
    pins_json: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let pins: Vec<usize> = opt_str(pins_json).and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        out_json(&wire::constellation_to_wire(&e.study_read().weaves, &e.corpus, page as usize, &pins))
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
pub unsafe extern "C" fn plumbline_engine_concept_json(
    engine: *const PlumblineEngine,
    code: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        let Some(ce) = e.concept_ready() else {
            return ptr::null_mut();
        };
        let Some(stat) = ce.stat(code) else {
            return ptr::null_mut();
        };
        let (ot, nt) = ce.testament_split(code);
        let leitwort = e.leitwort_ready().and_then(|l| l.get(code)).map(|b| wire::WireLeitwort {
            n: b.n,
            win_count: b.win_count,
            score: b.score,
            label: burst::span_label(|id| i18n::book_name(i18n::active(), id), &b.win_start, &b.win_end),
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
                    display: i18n::book_name(i18n::active(), &book),
                    book,
                    count,
                })
                .collect(),
            by_book: stat.by_book.clone(),
            // Same name filter as the radial map: a concept's neighbours are
            // concepts (see `name_noise`).
            collocates: wire::scored_to_wire(
                ce.collocates(code, 48).into_iter().filter(|(c, _)| !name_noise(e, c)).take(12).collect(),
            ),
            community: ce.community(code).into_iter().filter(|c| !name_noise(e, c)).take(12).collect(),
            leitwort,
        })
    })
}

/// The proper nouns that stay in a concept neighbourhood. In this corpus the
/// divine name and Christ *are* concepts, not incidental names — everything
/// the book says about salvation runs through them.
const CONCEPT_KEEP_NAMES: &[&str] = &["H3068", "H3069", "H3050", "H136", "G2424", "G5547"];

/// Whether `code` is a proper noun that has no business ringing a concept:
/// "faith" surrounded by Ephraim, Jerusalem and Shechem reads as noise rather
/// than meaning. Names stay fully reachable in word study,
/// concordance and search; only the collocate lists drop them.
fn name_noise(e: &PlumblineEngine, code: &str) -> bool {
    !CONCEPT_KEEP_NAMES.contains(&code) && e.strongs().get(code).is_some_and(strongs::is_proper_noun)
}

/// A short English gloss for a Strong's code — the modal KJV rendering across
/// its occurrences (≤80 sampled), falling back to a distilled dictionary
/// clause. Plain text (not JSON); null when nothing sensible exists.
///
/// # Safety
/// `engine` is a live engine; `code` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_gloss(engine: *const PlumblineEngine, code: *const c_char) -> *mut c_char {
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
// One Rust producer (`plumbline_core::panel`) builds the typed block list for every
// panel view; this projects the engine's data into it and serves the blocks as
// JSON. `full` (Full study vs simple reader) is a shell setting, so the endpoints
// that gate on it take a `full` flag — the FFI itself is mode-agnostic.

/// How many concordance verses a panel card lists before an "… N more" tail
/// (matches the shells' prior cap).
const PANEL_OCC_CAP: usize = 300;

impl PanelSource for PlumblineEngine {
    /// Whether the open text is the KJV — the same question `new` asks before it
    /// wires morphology and the overlay, and for the same reason.
    fn is_kjv_text(&self) -> bool {
        self.corpus.tokenization_version() == canon::TOKENIZATION_VERSION
    }
    fn lexicon_localized(&self) -> bool {
        self.strongs_localized.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn token_word(&self, verse: &str, token: u32) -> Option<String> {
        let v = VRef::parse_ref_key(verse)?;
        self.corpus.verse(&v)?.tokens.get(token as usize).map(|t| t.word.clone())
    }
    fn verse_display(&self, refkey: &str) -> Option<String> {
        VRef::parse_ref_key(refkey).map(|v| v.display())
    }
    fn morph_gloss(&self, verse: &str, token: u32) -> Option<String> {
        let (md, v) = (self.morph.get()?, VRef::parse_ref_key(verse)?);
        md.gloss(&v, token)
    }
    fn occurrence_count(&self, code: &str) -> usize {
        self.occ_ix_ready().map_or(0, |ix| ix.verses(code).len())
    }
    fn strongs(&self, code: &str) -> Option<panel::StrongsView> {
        let e = self.strongs().get(code)?;
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
            lemma: self.strongs().get(code).and_then(|e| e.lemma.clone()),
        }
    }
    fn renderings(&self, code: &str) -> Vec<panel::RenderingView> {
        let Some(rx) = self.renderings_ready() else { return Vec::new() };
        rx.renderings(code)
            .into_iter()
            .map(|r| panel::RenderingView { rendering: r.label.to_string(), total: r.count as u32 })
            .collect()
    }
    fn rendering_refs(&self, code: &str, rendering: &str) -> Option<panel::RenderingRefsView> {
        let key = plumbline_core::renderings::normalize(rendering);
        let r = self
            .renderings_ready()?
            .renderings(code)
            .into_iter()
            .find(|r| plumbline_core::renderings::normalize(r.label) == key)?;
        Some(panel::RenderingRefsView {
            rendering: r.label.to_string(),
            total: r.count as u32,
            refs: r.occs.iter().take(PANEL_OCC_CAP).map(|o| (o.vref.ref_key(), o.vref.display())).collect(),
        })
    }
    fn word_codes(&self, word: &str) -> Vec<String> {
        self.renderings_ready()
            .map(|rx| rx.word_codes(word).into_iter().map(|(c, _)| c.to_string()).collect())
            .unwrap_or_default()
    }
    fn occurrences(&self, code: &str) -> panel::OccurrencesView {
        let Some(ix) = self.occ_ix_ready() else {
            return panel::OccurrencesView { total: 0, verses: Vec::new() };
        };
        let all = ix.verses(code);
        panel::OccurrencesView {
            total: all.len() as u32,
            verses: all.iter().take(PANEL_OCC_CAP).map(|v| (v.ref_key(), v.display())).collect(),
        }
    }
    fn bridge_partners(&self, code: &str) -> Vec<panel::BridgePartnerView> {
        self.bridge_ready()
            .map(|b| b.partners(code))
            .unwrap_or_default()
            .into_iter()
            .map(|p| panel::BridgePartnerView {
                sources: p.sources.iter().map(|s| bridge::source_label(s).to_string()).collect(),
                tiers: bridge::tiers_of(&p.sources).into_iter().map(|t| t.wire_name().to_string()).collect(),
                research_grade: p.sources.iter().any(|s| bridge::research_grade(s)),
                code: p.code,
            })
            .collect()
    }
    fn concept(&self, code: &str) -> Option<panel::ConceptView> {
        let ce = self.concept_ready()?;
        ce.stat(code)?;
        let (ot, nt) = ce.testament_split(code);
        let leitwort = self.leitwort_ready().and_then(|l| l.get(code)).map(|b| panel::LeitwortView {
            n: b.n,
            win_count: b.win_count,
            score: b.score,
            label: burst::span_label(|id| i18n::book_name(i18n::active(), id), &b.win_start, &b.win_end),
        });
        Some(panel::ConceptView {
            community: ce.community(code),
            top_books: ce
                .top_books(code, 5)
                .into_iter()
                .map(|(b, n)| (i18n::book_name(i18n::active(), &b), n))
                .collect(),
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
        match self.xref_ix_ready().and_then(|ix| ix.get(&v)) {
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
                    .any(|m| matches!(&m.target, plumbline_core::tag::TagTarget::Verse(v) if v.ref_key() == verse));
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
                category: lt.tag.category.clone(),
                members: lt
                    .tag
                    .members
                    .iter()
                    .map(|m| match &m.target {
                        plumbline_core::tag::TagTarget::Verse(v) => panel::TagMemberView {
                            kind: "verse".into(),
                            verse: Some(v.ref_key()),
                            display: Some(v.display()),
                            strongs: None,
                            note: m.note.clone(),
                        },
                        plumbline_core::tag::TagTarget::Concept(c) => panel::TagMemberView {
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
                suggested: plumbline_core::weave::is_suggested(lw),
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
            .filter(|lw| plumbline_core::weave::is_suggested(lw))
            .enumerate()
            .map(|(index, lw)| {
                let lib_index = study
                    .weaves
                    .iter()
                    .position(|x| plumbline_core::weave::is_suggested(x) && x.weave.name == lw.weave.name);
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
        self.search_scoped(query, "all")
    }
    fn search_scoped(&self, query: &str, scope: &str) -> panel::SearchView {
        let study = self.study_read();
        // An unparseable token searches EVERYTHING rather than nothing: a
        // shell that sends a scope this build does not know still gets an
        // answer to the reader's query.
        let scope = search::SearchScope::parse(scope).unwrap_or_default();
        match search::run_search_scoped(&self.corpus, &study.notes, self.search_ix(), query, &scope) {
            Some(search::SearchAnswer::GoTo { book, chapter, verse }) => {
                let display = match verse {
                    Some(v) => VRef::new(book.clone(), chapter, v).display(),
                    None => VRef::new(book.clone(), chapter, 1).chapter_display_in(i18n::active()),
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
                        .map(|h| panel::SearchHitView {
                            verse: h.vref.ref_key(),
                            display: h.vref.display(),
                            note: h.note,
                            why: h.why,
                        })
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
pub unsafe extern "C" fn plumbline_engine_word_study_blocks_json(
    engine: *const PlumblineEngine,
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

/// [`plumbline_engine_word_study_blocks_json`] with per-tier gates instead of the
/// legacy Simple/Full flag: `gates` bit 0 = curated-scholarship (human)
/// analysis, bit 1 = learned/statistical (machine) analysis. The text and the
/// reader's own data are always on.
///
/// # Safety
/// `engine` is a live engine; `ref_key` is a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_word_study_blocks2_json(
    engine: *const PlumblineEngine,
    ref_key: *const c_char,
    token_index: u32,
    gates: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk)) = (engine.as_ref(), opt_str(ref_key)) else {
            return ptr::null_mut();
        };
        let codes: Vec<String> = VRef::parse_ref_key(rk)
            .and_then(|v| e.corpus.verse(&v).and_then(|verse| verse.tokens.get(token_index as usize).cloned()))
            .map(|t| t.strongs)
            .unwrap_or_default();
        out_json(&wire::blocks_to_wire(panel::word_study_gated(
            e,
            panel::Gates::from_bits(gates),
            rk,
            token_index,
            &codes,
        )))
    })
}

/// The standalone `code:CODE[:word]` study card (the reverse rendering-lens
/// target). `word` may be null. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_code_study_blocks_json(
    engine: *const PlumblineEngine,
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

/// [`plumbline_engine_code_study_blocks_json`] with per-tier gates (bit 0 = human
/// analysis, bit 1 = machine analysis).
///
/// # Safety
/// `engine` is a live engine; `code` is valid NUL-terminated UTF-8; `word` is
/// null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_code_study_blocks2_json(
    engine: *const PlumblineEngine,
    code: *const c_char,
    word: *const c_char,
    gates: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(code)) = (engine.as_ref(), opt_str(code)) else {
            return ptr::null_mut();
        };
        out_json(&wire::blocks_to_wire(panel::code_study_card_gated(
            e,
            panel::Gates::from_bits(gates),
            code,
            opt_str(word).unwrap_or(""),
        )))
    })
}

/// The full concordance for a code as blocks. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; `code` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_concordance_blocks_json(
    engine: *const PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_rendering_concordance_blocks_json(
    engine: *const PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_threads_blocks_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_thread_blocks_json(
    engine: *const PlumblineEngine,
    index: u32,
) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_tags_blocks_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_tag_blocks_json(engine: *const PlumblineEngine, index: u32) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_weaves_blocks_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_suggested_blocks_json(engine: *const PlumblineEngine) -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_engine_compare_blocks_json(
    engine: *const PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_search_blocks_json(
    engine: *const PlumblineEngine,
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

/// [`plumbline_engine_search_blocks_json`] narrowed to a scope token — see
/// [`plumbline_engine_search_scoped_json`] for the vocabulary.
///
/// # Safety
/// `engine` is a live engine; `query` and `scope` are null or valid
/// NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_search_blocks_scoped_json(
    engine: *const PlumblineEngine,
    query: *const c_char,
    scope: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(query)) = (engine.as_ref(), opt_str(query)) else {
            return ptr::null_mut();
        };
        if query.trim().is_empty() {
            return ptr::null_mut();
        }
        out_json(&wire::blocks_to_wire(panel::search_in(e, query, opt_str(scope).unwrap_or("all"))))
    })
}

/// How many occurrence verses the english-gloss tally samples.
const GLOSS_SAMPLE: usize = 80;

/// The modal KJV rendering of a code — what an English reader recognises
/// ("world" for κόσμος) rather than Strong's etymological headword. Ported
/// verbatim from the GTK shell so every shell shows the same chips.
fn english_gloss(e: &PlumblineEngine, code: &str) -> Option<String> {
    let mut tally: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let occ = e.occ_ix_ready()?;
    for r in occ.verses(code).iter().take(GLOSS_SAMPLE) {
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
    let entry = e.strongs().get(code)?;
    entry.def.as_deref().and_then(distil_gloss).or_else(|| entry.kjv.as_deref().and_then(distil_gloss))
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
    let first = cleaned.split([',', ';']).map(str::trim).find(|p| p.chars().any(|c| c.is_alphabetic()))?;
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
pub unsafe extern "C" fn plumbline_engine_weave_add_link_spans(
    engine: *mut PlumblineEngine,
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
/// (`{verb, …}`; see `plumbline_core::panel::parse_link`) — the one verb vocabulary,
/// so a non-Rust shell routes clicks through the core instead of re-splitting
/// the URI string and risking drift from what the panel emits. Engine-
/// independent. Null for an unknown verb or malformed payload (a shell then
/// ignores the click).
///
/// # Safety
/// `uri` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_route_link_json(uri: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || match opt_str(uri).and_then(panel::parse_link) {
        Some(link) => out_json(&wire::link_to_wire(link)),
        None => ptr::null_mut(),
    })
}

// ── config / session (engine-independent; shared with the GTK shell) ──────────

/// Load the cross-platform shell config (`%APPDATA%\plumbline\config.json` on
/// Windows) as JSON: `{studyMode, bodySize, openPanes, activePane, firstRun}`.
/// `firstRun` is true only when no config file existed. Never null.
#[no_mangle]
pub extern "C" fn plumbline_config_load_json() -> *mut c_char {
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
pub unsafe extern "C" fn plumbline_config_save_json(json: *const c_char) -> *mut c_char {
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

// ── Tier 0: copy, personal notes, themes, warming, guide ─────────────────────

/// Clipboard text for a verse (or its chapter, for the `chapter*` kinds) in one
/// of the shapes `plumbline_core::export::CopyKind` names (`verse` / `verseRef` /
/// `verseMarkdown` / `chapter` / `chapterMarkdown`). Plain text, not JSON; null
/// on a bad ref, an unknown kind, or a verse the corpus lacks. Caller-freed.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated
/// UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_copy_text(
    engine: *const PlumblineEngine,
    ref_key: *const c_char,
    kind: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(rk), Some(kind)) = (engine.as_ref(), opt_str(ref_key), opt_str(kind)) else {
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
pub unsafe extern "C" fn plumbline_engine_user_note_json(
    engine: *const PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_user_notes_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let study = e.study_read();
        let mut notes: Vec<&usernote::LoadedNote> = study.user_notes.values().collect();
        notes.sort_by_key(|a| a.note.vref.reading_key());
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
pub unsafe extern "C" fn plumbline_engine_user_note_set(
    engine: *mut PlumblineEngine,
    ref_key: *const c_char,
    text: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(rk), Some(text), Some(stamp)) = (opt_str(ref_key), opt_str(text), opt_str(stamp)) else {
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

/// The colour palette for a theme (`light`/`dark`/`night`; unknown → light) as
/// JSON — every semantic role as a `#rrggbb` hex. Engine-independent. Never null.
///
/// # Safety
/// `theme` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_theme_palette_json(theme: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let t = opt_str(theme).and_then(theme::Theme::parse).unwrap_or(theme::Theme::Light);
        out_json(&theme::palette(t))
    })
}

/// Which SEATING a local date and hour fall in — `"sunday-morning"`,
/// `"sunday-evening"`, `"wednesday-evening"` or `"other"`. Engine-independent,
/// never null.
///
/// The shells pass their OWN local date (`YYYY-MM-DD`) and hour (0–23), because
/// the core has no clock and no timezone: a slot computed in UTC would put a
/// Sunday-evening service in Monday for half the world. The RULE lives here so
/// the two shells cannot drift on when a service is, which is exactly the kind
/// of thing that would be written twice and quietly diverge.
///
/// # Safety
/// `date` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_session_slot(date: *const c_char, hour: u32) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let d = opt_str(date).unwrap_or("");
        out_string(session_slot::slot_for(d, hour).token().to_string())
    })
}

/// [`plumbline_session_slot`] to the minute, honouring a configured Sunday
/// service time. `minute` is minutes since local midnight (0–1439);
/// `sunday_service` is the config's `sundayService` value, or **-1 when the
/// reader never set one**, which keeps the before-noon rule. With a time set,
/// `"sunday-morning"` runs from the service start until 1.5 hours after it —
/// see `core::session_slot::slot_for_at`. Engine-independent, never null.
///
/// # Safety
/// `date` is null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_session_slot_at(
    date: *const c_char,
    minute: u32,
    sunday_service: i32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let d = opt_str(date).unwrap_or("");
        let service = u32::try_from(sunday_service).ok();
        out_string(session_slot::slot_for_at(d, minute, service).token().to_string())
    })
}

/// EVERY user-visible string, for one language, in ONE call:
/// `{"lang","strings":{id: text, …},"languages":[{"code","endonym"}]}`.
///
/// The whole catalogue at once, deliberately. A shell asks at startup and holds
/// the map; a call per string would be thousands of round trips across the wasm
/// boundary to render one screen.
///
/// Takes BOTH the reader's setting and the device's locale, and resolves them
/// here rather than in each shell: an empty setting means "follow the device"
/// (`Config::language`), and that rule implemented twice is a rule that
/// disagrees with itself once. Either may be null. Both tolerate a region tag —
/// a browser reporting `de-CH` gets German — and anything unrecognised falls
/// through to English, so an unsupported locale gets a working app rather than
/// an error. The reply's `lang` says which one won.
///
/// Strings absent from the resolved language fall back to English key by key,
/// so every id the shell asks for resolves to something printable.
///
/// `languages` rides along because a language picker needs the list, each
/// labelled in ITSELF — someone looking for German is looking for "Deutsch".
///
/// Engine-independent: the shells need their chrome before an engine exists.
/// Never null.
///
/// # Safety
/// `chosen` and `device` are null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_i18n_catalog_json(chosen: *const c_char, device: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let l = i18n::resolve(opt_str(chosen).unwrap_or(""), opt_str(device).unwrap_or(""));
        out_json(&wire::catalog_to_wire(l))
    })
}

/// Set the language the ENGINE writes in, and answer with the code it resolved.
///
/// A shell calls this ONCE, at startup, alongside
/// `plumbline_i18n_catalog_json`. The catalogue covers what a shell spells; this
/// covers what the CORE spells — every book name and every reference it hands
/// back, in the table of contents, search hits, weave endpoints, note headers,
/// thread entries, the reading map. Without it a German reader gets a German
/// interface listing a book called Genesis, which is worse than either language
/// on its own.
///
/// Two calls rather than one on purpose. Resolving a language and choosing one
/// are different acts, and a getter with a global side effect would mean every
/// test that asked for a catalogue silently repainted the whole process.
///
/// Same arguments and same rule as the catalogue call: an empty or unknown
/// `chosen` falls through to `device`, and an unknown device is English.
/// Caller-freed; never null.
///
/// # Safety
/// `chosen` and `device` are null or valid NUL-terminated UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_i18n_set_language(chosen: *const c_char, device: *const c_char) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let l = i18n::resolve(opt_str(chosen).unwrap_or(""), opt_str(device).unwrap_or(""));
        i18n::set_active(l);
        out_string(l.code().to_string())
    })
}

/// Force the lazy analytics indexes (concept engine, leitwort scan) to build
/// now — call once on a background thread at startup in Full mode so the first
/// study click doesn't stall. Safe to call from any thread (the builds are
/// `OnceLock`-guarded) and idempotent. Null on success, else an owned error.
///
/// # Safety
/// `engine` is a live engine (or null → an error string).
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_warm_indexes(engine: *const PlumblineEngine) -> *mut c_char {
    guard_err(|| {
        let Some(e) = engine.as_ref() else {
            return out_string("null engine".to_string());
        };
        e.search_ix();
        e.occ_ix();
        e.renderings();
        e.xref_ix();
        e.bridge();
        e.concept();
        e.leitwort();
        ptr::null_mut()
    })
}

/// Load the stage-2 core data (Strong's dictionary + a study reload for the
/// 1769 margin notes) once those files have arrived in the home — the web
/// boots on the corpus alone (TODO #28: text on screen is the north star)
/// and calls this when the rest of the core pack lands. Idempotent, cheap
/// when nothing is missing. Null on success, else an owned error.
///
/// # Safety
/// `engine` is a live engine (or null → an error string).
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_load_core_data(engine: *const PlumblineEngine) -> *mut c_char {
    guard_err(|| {
        let Some(e) = engine.as_ref() else {
            return out_string("null engine".to_string());
        };
        e.load_core_data();
        ptr::null_mut()
    })
}

/// Load the optional R&D artifact (the morphology sidecar) from the engine's
/// home if it was absent at open. The web shell boots on the core data pack for
/// a fast first paint, fetches the R&D pack in the background, writes the files
/// into the home, then calls this. Idempotent (nothing loads twice), cheap when
/// the file is still missing, safe from any thread. Null on success, else an
/// owned error.
///
/// # Safety
/// `engine` is a live engine (or null → an error string).
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_load_rnd_data(engine: *const PlumblineEngine) -> *mut c_char {
    guard_err(|| {
        let Some(e) = engine.as_ref() else {
            return out_string("null engine".to_string());
        };
        e.load_rnd_data();
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
pub unsafe extern "C" fn plumbline_engine_memory_grade(
    engine: *mut PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_memory_add(
    engine: *mut PlumblineEngine,
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

/// Start memorizing the passage `start_ref`…`through_ref` (inclusive) as ONE
/// card — the whole section recalled in one go, rather than a card per verse.
/// The card is keyed and listed by `start_ref`.
///
/// `through_ref` must name a later verse of the same chapter; anything else
/// seeds a plain single-verse card. Already memorizing `start_ref` is a no-op,
/// so re-running with a different end does NOT silently re-span the card —
/// remove it first. Null on success, else an owned error.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_memory_add_passage(
    engine: *mut PlumblineEngine,
    start_ref: *const c_char,
    through_ref: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory; cannot author".to_string());
        };
        let (Some(sr), Some(tr), Some(now)) = (opt_str(start_ref), opt_str(through_ref), opt_str(now)) else {
            return out_string("null or invalid argument".to_string());
        };
        let (Some(start), Some(through)) = (VRef::parse_ref_key(sr), VRef::parse_ref_key(tr)) else {
            return out_string("bad ref".to_string());
        };
        // Every verse must actually exist, or the drill would prompt for text
        // the reader can never see.
        if engine.corpus.verse(&start).is_none() || engine.corpus.verse(&through).is_none() {
            return out_string("no such verse".to_string());
        }
        let (cards, _) = memory::load_cards(&home);
        if cards.contains_key(&start) {
            return ptr::null_mut();
        }
        let card = memory::Card::new_passage(start, &through, canon::TOKENIZATION_VERSION, now);
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
pub unsafe extern "C" fn plumbline_engine_memory_remove(
    engine: *mut PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_memory_card_json(
    engine: *const PlumblineEngine,
    verse_ref: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref)) = (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key)) else {
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
pub unsafe extern "C" fn plumbline_engine_memory_due_json(
    engine: *const PlumblineEngine,
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
pub unsafe extern "C" fn plumbline_engine_memory_coverage_json(
    engine: *const PlumblineEngine,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(now)) = (engine.as_ref(), opt_str(now)) else {
            return ptr::null_mut();
        };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        out_json(&wire::WireMemoryCoverage {
            verses: memory::coverage(&cards, now),
            cards: memory::card_list(&cards, now),
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
pub unsafe extern "C" fn plumbline_engine_memory_activity_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        out_json(&wire::WireMemoryActivity { days: memory::activity_by_day(&cards) })
    })
}

/// What a memorize card is drilled and scored against: `(label, text, verses)`.
/// A passage card's verses are joined into one continuous text, so the drill
/// prompts for — and the check scores — the whole chunk. A ref with no card yet
/// (or the engine with no home) falls back to the single verse, which is what
/// every card was before passages existed. None if the verse doesn't exist.
fn memory_span(e: &PlumblineEngine, vref: &VRef) -> Option<(String, String, u32)> {
    let card = e.home.as_ref().and_then(|h| memory::load_cards(h).0.remove(vref));
    let refs = card.as_ref().map_or_else(|| vec![vref.clone()], memory::Card::verses);
    let bodies: Vec<String> = refs.iter().filter_map(|r| e.corpus.verse(r)).map(|v| v.body()).collect();
    if bodies.is_empty() {
        return None;
    }
    let label = card.as_ref().map_or_else(|| vref.ref_key(), memory::Card::label);
    Some((label, bodies.join(" "), bodies.len() as u32))
}

/// A drill prompt for `verse_ref` at blank-out `level` (0 = full text … max):
/// the text, its first-letter skeleton, and the blanked form. When `verse_ref`
/// is a passage card's first verse, the drill covers the whole passage. Null if
/// the verse isn't found.
///
/// # Safety
/// `engine` is a live engine; `verse_ref` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_memory_drill_json(
    engine: *const PlumblineEngine,
    verse_ref: *const c_char,
    level: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref)) = (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key)) else {
            return ptr::null_mut();
        };
        let Some((label, text, verses)) = memory_span(e, &vref) else { return ptr::null_mut() };
        let level = level.min(u8::MAX as u32) as u8;
        out_json(&wire::WireMemoryDrill {
            reference: vref.ref_key(),
            label,
            verses,
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
pub unsafe extern "C" fn plumbline_engine_memory_score_json(
    engine: *const PlumblineEngine,
    verse_ref: *const c_char,
    typed: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref), Some(typed)) =
            (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key), opt_str(typed))
        else {
            return ptr::null_mut();
        };
        let Some((_, actual, _)) = memory_span(e, &vref) else { return ptr::null_mut() };
        out_json(&memory::score_recall(typed, &actual))
    })
}

/// The in-app guide as panel blocks. Engine-independent (static content). Never
/// null.
#[no_mangle]
pub extern "C" fn plumbline_panel_guide_blocks_json() -> *mut c_char {
    guard(ptr::null_mut(), || out_json(&wire::blocks_to_wire(panel::guide_blocks())))
}

/// The About card as panel blocks. Engine-independent (static content). Never
/// null.
#[no_mangle]
pub extern "C" fn plumbline_panel_about_blocks_json() -> *mut c_char {
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
        "verse" => VRef::parse_ref_key(value).map(TagTarget::Verse).ok_or_else(|| format!("bad ref: {value}")),
        "concept" => Ok(TagTarget::Concept(value.to_string())),
        other => Err(format!("bad target kind: {other}")),
    }
}

#[cfg(test)]
mod tests;

/// The width memo, exercised through the real C entry point rather than through
/// `plumbline_layout`'s own tests: what is at stake here is the *wiring* — that
/// the memo lives on the engine and so survives between two
/// [`plumbline_engine_layout_chapter`] calls, and that [`font_identity`] notices a
/// font change the shell only reveals through its config.
///
/// (These live in `lib.rs` rather than `tests.rs` because the module they cover
/// is here; the mechanism itself is tested in `crates/layout/src/memo.rs`.)
#[cfg(test)]
mod measure_memo_over_the_abi {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
    use std::sync::Mutex;

    const KJV: &str = concat!(
        r#"{"format":"x","tokenization":"kjv1769-tok2","verses":2}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","In","",[],0],["","the","",[],0],["","beginning","",["H7225"],0],["","God","",["H430"],0]],"v":1}"#,
        "\n",
        r#"{"b":"Gen","c":1,"t":[["","And","",[],0],["","the","",[],0],["","Spirit","",[],0],["","of","",[],0],["","God","",["H430"],0],["","moved",".",[],0]],"v":2}"#,
    );

    /// One increment is one crossing of the C ABI — a canvas `measureText` on the
    /// web, a `Paint` upcall on Android, and exactly what the web shell's
    /// `measureCalls()` diagnostic counts.
    static CROSSINGS: AtomicUsize = AtomicUsize::new(0);
    /// Every run that crossed, in order — so "measured only once" can be checked
    /// as a fact about the text, not just a total.
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
    /// The pretend font: px per character. A shell changes this without telling
    /// the ABI, exactly as a text-size change does.
    static CHAR_W: AtomicU32 = AtomicU32::new(10);
    /// These tests share the statics above, and `cargo test` runs them in
    /// parallel.
    static SERIAL: Mutex<()> = Mutex::new(());

    extern "C" fn counting_measure(_ctx: *mut c_void, text: *const c_char) -> f32 {
        CROSSINGS.fetch_add(1, Ordering::Relaxed);
        let s = unsafe { CStr::from_ptr(text) }.to_str().unwrap_or("").to_string();
        let w = s.chars().count() as f32 * CHAR_W.load(Ordering::Relaxed) as f32;
        SEEN.lock().unwrap().push(s);
        w
    }

    /// Claim the shared statics and reset the counters.
    fn start(char_w: u32) -> std::sync::MutexGuard<'static, ()> {
        let g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        CROSSINGS.store(0, Ordering::Relaxed);
        CHAR_W.store(char_w, Ordering::Relaxed);
        SEEN.lock().unwrap().clear();
        g
    }

    fn cfg(line_height: f32, space_width: f32, width: f32) -> PlumblineLayoutConfig {
        PlumblineLayoutConfig {
            width,
            line_height,
            space_width,
            verse_num_gap: 4.0,
            para_indent: 16.0,
            para_spacing: 8.0,
            verse_break: 0,
            verse_numbers: 1,
        }
    }

    /// Lay Gen 1 out through the ABI and return the width of the box painting
    /// `word`, plus how many runs crossed during this call.
    unsafe fn lay_out(engine: *mut PlumblineEngine, cfg: PlumblineLayoutConfig, word: &str) -> (f32, usize) {
        let before = CROSSINGS.load(Ordering::Relaxed);
        let book = CString::new("Gen").unwrap();
        let dl =
            plumbline_engine_layout_chapter(engine, book.as_ptr(), 1, cfg, Some(counting_measure), ptr::null_mut());
        assert!(!dl.is_null(), "Gen 1 must lay out");
        let json_ptr = plumbline_layout_to_json(dl);
        let json = CStr::from_ptr(json_ptr).to_str().unwrap().to_string();
        plumbline_string_free(json_ptr);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let w = parsed["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["text"] == word)
            .unwrap_or_else(|| panic!("no box paints {word}"))["w"]
            .as_f64()
            .unwrap() as f32;
        plumbline_layout_free(dl);
        (w, CROSSINGS.load(Ordering::Relaxed) - before)
    }

    unsafe fn open() -> *mut PlumblineEngine {
        // An empty Strong's dictionary: layout never consults it.
        const STRONGS: &str = "{}";
        let mut err: *mut c_char = ptr::null_mut();
        let e = plumbline_engine_open_from_bytes(KJV.as_ptr(), KJV.len(), STRONGS.as_ptr(), STRONGS.len(), &mut err);
        assert!(err.is_null() && !e.is_null(), "engine should open");
        e
    }

    #[test]
    fn each_distinct_run_crosses_the_abi_once_and_a_re_layout_crosses_not_at_all() {
        let _serial = start(10);
        unsafe {
            let engine = open();

            // 12 runs are laid out (2 verse numbers + 10 tokens) but "the" and
            // "God" each occur twice, so only 10 distinct ones may cross.
            let (god, cold) = lay_out(engine, cfg(20.0, 5.0, 10_000.0), "God");
            assert_eq!(god, 30.0);
            assert_eq!(cold, 10, "one crossing per DISTINCT run, not per token");
            let seen = SEEN.lock().unwrap().clone();
            let mut once = seen.clone();
            once.sort();
            once.dedup();
            assert_eq!(once.len(), seen.len(), "a run crossed twice: {seen:?}");

            // The same chapter again, same font: the memo is on the ENGINE, so
            // this is free.
            let (again, warm) = lay_out(engine, cfg(20.0, 5.0, 10_000.0), "God");
            assert_eq!(warm, 0, "a re-layout must not cross at all");
            assert_eq!(again, god);

            // A narrower column re-wraps the chapter without re-measuring a
            // glyph: `width` is deliberately outside the font identity, which is
            // what makes a rotation or a margin drag free too.
            let (narrow, resized) = lay_out(engine, cfg(20.0, 5.0, 200.0), "God");
            assert_eq!(resized, 0, "a resize measures nothing new");
            assert_eq!(narrow, god);

            plumbline_engine_free(engine);
        }
    }

    #[test]
    fn a_text_size_change_re_measures_instead_of_reusing_the_old_widths() {
        let _serial = start(10);
        unsafe {
            let engine = open();
            let (small, _) = lay_out(engine, cfg(20.0, 5.0, 10_000.0), "God");
            assert_eq!(small, 30.0);

            // The reader bumps the text size: the shell measures with a bigger
            // font and passes the line height + space advance it derived from
            // that same font. Nothing else tells this ABI anything changed.
            CHAR_W.store(20, Ordering::Relaxed);
            let (big, crossings) = lay_out(engine, cfg(40.0, 10.0, 10_000.0), "God");
            assert_eq!(big, 60.0, "the new size's width, not the remembered one");
            assert_eq!(crossings, 10, "every run is re-measured at the new size");

            // …and the new widths are what is remembered now: going back to the
            // small size re-measures rather than serving these.
            CHAR_W.store(10, Ordering::Relaxed);
            let (back, recrossed) = lay_out(engine, cfg(20.0, 5.0, 10_000.0), "God");
            assert_eq!(back, 30.0);
            assert_eq!(recrossed, 10);

            plumbline_engine_free(engine);
        }
    }
}
