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

use crate::PlumblineMeasureFn;

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
