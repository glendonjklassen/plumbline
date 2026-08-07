//! Web-shell (wasm32) shims — compiled only for `wasm32-*` targets.
//!
//! The TypeScript binding drives the same C ABI as JNA/P-Invoke, but JS can't
//! allocate linear memory or synthesize a C function pointer the way those
//! runtimes do. Two additions close the gap:
//!
//!  - [`plumbline_web_alloc`] / [`plumbline_web_free`]: a byte allocator so the shell can
//!    copy NUL-terminated UTF-8 arguments into the module before a call.
//!  - [`plumbline_web_measure_fnptr`]: the shell provides text measurement as a wasm
//!    *import* (`plumbline.plumbline_js_measure`, backed by canvas `measureText`);
//!    this export hands it back as a [`PlumblineMeasureFn`] table index so the
//!    existing `plumbline_layout_*` surface works unchanged.
//!
//! Strings *returned* by the ABI are freed with the existing
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

/// Do one SLICE of the web shell's warm-up. Returns 1 while work remains, 0
/// when there is nothing left (or the engine is null). Idempotent.
///
/// Only the search index is pre-built, and it is built a slice of verses at a
/// time. Two measurements drove that (a 2026 flagship phone, 2026-07-26):
///
///  - Warming every index cost ~28 s of worker CPU after boot, 12 s of it on
///    machine-tier indexes the reader hadn't switched on. Everything except
///    search now builds on first use — the study panel already shows a
///    loading state, and a reader who never opens a concept map never pays
///    for one. Search is the exception because the search box is expected to
///    answer the first keystroke.
///  - The fold ran as ONE 4.6 s block. This thread also answers layout and
///    taps, so a page turn during the warm waited it out. Slices give the
///    worker a chance to serve those between calls; `step` is ignored, since
///    progress lives in the engine's partial builder.
///
/// Android keeps [`plumbline_engine_warm_indexes`](crate::plumbline_engine_warm_indexes):
/// native builds all of this in well under a second.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_warm_step(engine: *const PlumblineEngine, _step: u32) -> i32 {
    let Some(e) = (unsafe { engine.as_ref() }) else {
        return 0;
    };
    // ~2k verses a slice: enough that the per-call overhead disappears, short
    // enough that a tap waits milliseconds rather than seconds. Defined in
    // lib.rs so the slicing tests drive exactly the size the shell does — a test
    // with its own slice constant can pass at a size the product never uses.
    use crate::WARM_SLICE as SLICE;
    // Search first (it answers the search box), then the two indexes a WORD
    // CLICK needs. Those two used to be built whole on the reader's first click
    // — every session, since nothing survives the tab — which is the "it loads
    // for a while, every time" report of 2026-07-27. Warming them here costs
    // the same work, but off the critical path and in slices, so a tap landing
    // mid-warm waits milliseconds.
    guard(0, || e.warm_next(SLICE))
}

/// Declare that this shell warms the indexes in SLICES, so the engine must never
/// build one inside a reader's request — it answers with what is ready and the
/// shell re-asks once the warm has filled the rest in.
///
/// Call it immediately after open. Deriving it from the first
/// [`plumbline_engine_warm_step`] call instead is a race the reader wins: the
/// web's warm starts only after stage 2 is fetched and parsed, ~550 ms after text
/// appears on a phone, and the first tap lands inside that window. That cost a
/// whole release — the fix shipped, and the phone still froze for 26,042 ms
/// inside one `wordStudyBlocks` (2026-07-28).
///
/// Web-only, hence wasm-only: Android warms through
/// [`plumbline_engine_warm_indexes`](crate::plumbline_engine_warm_indexes), which
/// builds everything up front in well under a second, and wants the ordinary
/// build-on-demand behaviour.
///
/// # Safety
/// `engine` is a live engine or null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_defer_builds(engine: *const PlumblineEngine, on: i32) {
    if let Some(e) = unsafe { engine.as_ref() } {
        guard((), || e.set_defer_builds(on != 0));
    }
}

/// Load ONE machine-tier artifact: step 1 the morphology sidecar. Step 0 was the
/// concept embedding, retired 2026-07-30 — it is now a vestigial no-op kept so
/// the two-step contract and both shells' load loop stay unchanged. Returns 1
/// while steps remain, 0 when done (or on a null engine). Idempotent — an
/// artifact already loaded, or still missing from the home, is a cheap no-op.
///
/// [`plumbline_engine_load_rnd_data`](crate::plumbline_engine_load_rnd_data)
/// does both in one call, which parses ~17 MB of text. On a phone that is many
/// seconds during which this thread answers nothing — the reader tapped a word
/// and the study sheet sat on "— loading —" until it finished (2026-07-27).
/// Split, the worker can serve a tap between artifacts.
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
