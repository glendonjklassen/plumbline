//! Web-shell (wasm32) shims — compiled only for `wasm32-*` targets.
//!
//! JS cannot allocate linear memory or synthesize a C function pointer, so two
//! additions close the gap with the plain C ABI:
//!
//!  - [`plumbline_web_alloc`] / [`plumbline_web_free`]: a byte allocator so the shell can
//!    copy NUL-terminated UTF-8 arguments into the module before a call.
//!  - [`plumbline_web_measure_fnptr`]: the shell provides text measurement as a wasm
//!    *import* (`plumbline.plumbline_js_measure`, backed by canvas `measureText`);
//!    this export hands it back as a [`PlumblineMeasureFn`] table index so the
//!    existing `plumbline_layout_*` surface works unchanged.
//!
//! Strings *returned* by the ABI are freed with
//! [`plumbline_string_free`](crate::plumbline_string_free); this pair is only
//! for caller-owned argument buffers.

use std::ffi::{c_char, c_void};

use crate::{guard, PlumblineEngine, PlumblineMeasureFn};

#[link(wasm_import_module = "plumbline")]
extern "C" {
    /// Provided by the web shell at instantiation: the advance width, in px,
    /// of `text` (NUL-terminated UTF-8) in the current reader font. Must obey
    /// the [`PlumblineMeasureFn`] contract (total; finite, non-negative result).
    fn plumbline_js_measure(ctx: *mut c_void, text: *const c_char) -> f32;
}

extern "C" fn measure_trampoline(ctx: *mut c_void, text: *const c_char) -> f32 {
    unsafe { plumbline_js_measure(ctx, text) }
}

/// The JS-backed measure callback as a C function pointer (a wasm table
/// index), for the `measure`/`measure_ctx` params of the `plumbline_layout_*` calls.
#[no_mangle]
pub extern "C" fn plumbline_web_measure_fnptr() -> PlumblineMeasureFn {
    Some(measure_trampoline)
}

/// Do one slice of the web shell's warm-up. Returns 1 while work remains, 0
/// when there is nothing left (or the engine is null). Idempotent.
///
/// Only the search index is pre-built (the search box must answer the first
/// keystroke); everything else builds on first use behind a loading state.
/// It is built a slice of verses at a time because this thread also answers
/// layout and taps, and the whole fold is seconds of blocking work. `step` is
/// ignored — progress lives in the engine's partial builder.
///
/// [`plumbline_engine_warm_indexes`](crate::plumbline_engine_warm_indexes) is the
/// build-it-all-now alternative, for callers that can afford the block.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_warm_step(engine: *const PlumblineEngine, _step: u32) -> i32 {
    let Some(e) = (unsafe { engine.as_ref() }) else {
        return 0;
    };
    // ~2k verses a slice: per-call overhead disappears, a tap still waits only milliseconds.
    // Lives in lib.rs so the slicing tests drive exactly the size the shell does.
    use crate::WARM_SLICE as SLICE;
    // Search first (the search box), then the two indexes a word click needs — same work
    // either way, but off the critical path and in slices.
    guard(0, || e.warm_next(SLICE))
}

/// Declare that this shell warms the indexes in slices, so the engine must never
/// build one inside a reader's request — it answers with what is ready and the
/// shell re-asks once the warm has filled the rest in.
///
/// Call it immediately after open: deriving it from the first
/// [`plumbline_engine_warm_step`] call is a race the reader wins, since the warm
/// starts ~550 ms after text appears and the first tap lands inside that window.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_defer_builds(engine: *const PlumblineEngine, on: i32) {
    if let Some(e) = unsafe { engine.as_ref() } {
        guard((), || e.set_defer_builds(on != 0));
    }
}

/// Load one machine-tier artifact: step 1 the morphology sidecar. Step 0 was the
/// retired concept embedding — a no-op kept so the two-step contract and the
/// shell's load loop stay unchanged. Returns 1 while steps remain, 0 when done
/// (or on a null engine). Idempotent — an artifact already loaded, or still
/// missing from the home, is a cheap no-op.
///
/// [`plumbline_engine_load_rnd_data`](crate::plumbline_engine_load_rnd_data) does
/// both in one call, parsing ~17 MB of text — seconds during which this thread
/// answers no taps. Split, the worker can serve one between artifacts.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_load_rnd_step(engine: *const PlumblineEngine, step: u32) -> i32 {
    let Some(e) = (unsafe { engine.as_ref() }) else {
        return 0;
    };
    guard(0, || match step {
        0 => 1,
        1 => {
            e.load_morph_only();
            0
        }
        _ => 0,
    })
}

/// The word-usage card (the word-first study candidate) as a typed block list
/// (`{blocks:[…]}`). Pass EITHER a non-empty `word` (following a `wusage:`
/// link) or `ref_key` + `token_index` (a tap — the word and its Strong's codes
/// then come from the token itself). A non-empty `code` opens the card in its
/// original-word lens (`lusage:` links): that code's occurrences instead of
/// the surface word's. `scope` is a
/// [`SearchScope::token`](plumbline_core::search::SearchScope::token) string
/// (`all`, `ot`, `nt`, `book:Gen`, …); null or empty means `all`. While the
/// search index is still warming the card carries its loading line and the
/// shell re-asks on `warmReady`, like every other panel.
///
/// Wasm-only while the candidate bakes, hence on the plumbline-bindgen exclude
/// list and out of the C header.
///
/// # Safety
/// `engine` is a live engine or null; `word`, `code`, `ref_key`, `scope` are
/// null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_word_usage_blocks_json(
    engine: *const PlumblineEngine,
    word: *const c_char,
    code: *const c_char,
    ref_key: *const c_char,
    token_index: u32,
    scope: *const c_char,
    page: u32,
    gates: u32,
) -> *mut c_char {
    guard(std::ptr::null_mut(), || {
        let Some(e) = (unsafe { engine.as_ref() }) else {
            return std::ptr::null_mut();
        };
        let scope = unsafe { crate::opt_str(scope) }.filter(|s| !s.is_empty()).unwrap_or("all");
        let lens = unsafe { crate::opt_str(code) }.filter(|c| !c.is_empty());
        let refkey = unsafe { crate::opt_str(ref_key) }.filter(|r| !r.is_empty());
        let mut word = unsafe { crate::opt_str(word) }.unwrap_or("").to_string();
        let origin = refkey.map(|r| (r, token_index));
        let mut codes: Vec<String> = Vec::new();
        if let Some((r, t)) = origin {
            if let Some(tok) = plumbline_core::VRef::parse_ref_key(r)
                .and_then(|v| e.corpus.verse(&v))
                .and_then(|verse| verse.tokens.get(t as usize))
            {
                if word.is_empty() {
                    word = tok.word.clone();
                }
                codes = tok.strongs.clone();
            }
        }
        if codes.is_empty() && !word.is_empty() {
            codes = plumbline_core::panel::PanelSource::word_codes(e, &word);
        }
        // A lens code the chips did not offer still renders (a stale link): put it in the chip
        // row so the reader can see what they are looking at and switch back.
        if let Some(c) = lens {
            if !codes.iter().any(|k| k == c) {
                codes.push(c.to_string());
            }
        }
        let q = plumbline_core::panel::UsageQuery { word: &word, lens, scope, page, origin, codes: &codes };
        crate::out_json(&crate::wire::blocks_to_wire(plumbline_core::panel::word_usage_card(
            e,
            plumbline_core::panel::Gates::from_bits(gates),
            &q,
        )))
    })
}

/// One thread's detail as blocks, with the edit flag: `edit != 0` renders the
/// per-entry reorder/remove/note controls (the PWA's replacement for drag).
/// `plumbline_engine_thread_blocks_json` keeps its shape and serves the read
/// view; this variant is wasm-only and excluded from the C header.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_thread_blocks2_json(
    engine: *const PlumblineEngine,
    index: u32,
    edit: u32,
) -> *mut c_char {
    guard(std::ptr::null_mut(), || match unsafe { engine.as_ref() } {
        Some(e) => crate::out_json(&crate::wire::blocks_to_wire(plumbline_core::panel::thread_detail(
            e,
            index as usize,
            edit != 0,
        ))),
        None => std::ptr::null_mut(),
    })
}

/// Allocate `len` bytes the shell will fill with a NUL-terminated UTF-8
/// argument. Null when `len` is 0. Release with [`plumbline_web_free`].
#[no_mangle]
pub extern "C" fn plumbline_web_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Free a buffer from [`plumbline_web_alloc`]. `len` must be the allocated length;
/// null is a no-op.
///
/// # Safety
/// `ptr` must be null or a `plumbline_web_alloc(len)` result not already freed.
#[no_mangle]
pub unsafe extern "C" fn plumbline_web_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}
