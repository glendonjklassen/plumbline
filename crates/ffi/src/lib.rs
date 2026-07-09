//! `pure-ffi` — the single C ABI surface over `pure-core` (+ optional
//! `pure-rnd`).
//!
//! The plan (decision #1, native-per-platform): define the app's API **once**
//! here as a C ABI, then generate language bindings from it —
//! [UniFFI](https://mozilla.github.io/uniffi-rs/) for Kotlin (Android) and
//! [csbindgen](https://github.com/Cysharp/csbindgen) for C# (WinUI). Each
//! native UI paints the display list the core produces and forwards input
//! coordinates back across this boundary; no study logic is reimplemented in
//! Kotlin or C#.
//!
//! **Stub for now.** Only a version probe is exposed so the `cdylib`/`staticlib`
//! link is exercised. The real surface (open corpus → render chapter → hit-test
//! tap) lands once `pure-layout` produces a display list worth marshalling.

use std::ffi::c_char;
use std::ffi::CString;

/// Return the pure-study core version as a NUL-terminated UTF-8 C string.
///
/// # Safety
/// The returned pointer is owned by the caller and must be released with
/// [`pure_study_string_free`]. It is never null.
#[no_mangle]
pub extern "C" fn pure_study_version() -> *mut c_char {
    let v = env!("CARGO_PKG_VERSION");
    CString::new(v).unwrap_or_default().into_raw()
}

/// Free a string previously returned by this library.
///
/// # Safety
/// `ptr` must be a pointer returned by a `pure_study_*` function and not
/// previously freed. Passing null is a no-op.
#[no_mangle]
pub unsafe extern "C" fn pure_study_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_roundtrips_through_c_abi() {
        let p = pure_study_version();
        assert!(!p.is_null());
        let s = unsafe { CStr::from_ptr(p) }.to_str().unwrap().to_owned();
        assert_eq!(s, env!("CARGO_PKG_VERSION"));
        unsafe { pure_study_string_free(p) };
    }
}
