//! Encrypted backups over the ABI: seal bytes under a phrase, open them again,
//! and make a phrase. Pure functions of their arguments — no engine, no home —
//! so a shell can back up before, or without, an engine.
//!
//! The format is age (`plumbline_core::backup`): a reader with the `age` CLI
//! and their phrase can open the file without this app.
//!
//! Byte buffers come back caller-owned with their length written through
//! `out_len`; free them with [`plumbline_bytes_free`]. Strings follow the usual
//! rule ([`plumbline_string_free`](crate::plumbline_string_free)).

use std::ffi::c_char;
use std::ptr;

use plumbline_core::backup;

use crate::{guard, opt_str, out_string};

/// Random bytes [`plumbline_backup_encrypt`] needs from the shell's CSPRNG.
/// (A literal, not the core's name: cbindgen cannot evaluate a path, and a test
/// below keeps the two equal.)
pub const PLUMBLINE_BACKUP_ENTROPY_LEN: usize = 48;
/// Random bytes [`plumbline_backup_phrase`] needs from the shell's CSPRNG.
pub const PLUMBLINE_BACKUP_PHRASE_ENTROPY_LEN: usize = 8;

/// Errors cross as JSON `{kind, message}`. `kind` is one of `entropy`,
/// `notEncrypted`, `wrongPhrase`, `damaged`: a shell asks for the phrase again
/// on `wrongPhrase` and reports the others in `message`'s words.
fn err_json(e: &backup::Error) -> *mut c_char {
    use backup::Error::*;
    let kind = match e {
        Entropy => "entropy",
        NotAge => "notEncrypted",
        WrongPhrase => "wrongPhrase",
        Header(_) | HeaderMac | Payload(_) => "damaged",
    };
    let message = serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"\"".to_string());
    out_string(format!(r#"{{"kind":"{kind}","message":{message}}}"#))
}

/// A borrowed byte argument; null or zero-length is the empty slice.
///
/// # Safety
/// `p` points at `n` readable bytes for the call, or is null.
unsafe fn bytes_in<'a>(p: *const u8, n: usize) -> &'a [u8] {
    if p.is_null() || n == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(p, n) }
    }
}

/// Hand a buffer to the caller, its length through `out_len` (checked non-null by
/// the entry point).
unsafe fn bytes_out(v: Vec<u8>, out_len: *mut usize) -> *mut u8 {
    let b = v.into_boxed_slice();
    unsafe { *out_len = b.len() };
    Box::into_raw(b) as *mut u8
}

/// Free a buffer returned by [`plumbline_backup_encrypt`] or
/// [`plumbline_backup_decrypt`]. `len` is the length that call reported. Null is
/// a no-op.
///
/// # Safety
/// `ptr`/`len` came from one of those calls and are freed once.
#[no_mangle]
pub unsafe extern "C" fn plumbline_bytes_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)) });
    }
}

/// Seal `data` under `phrase` as an age file. `entropy` is at least
/// [`PLUMBLINE_BACKUP_ENTROPY_LEN`] bytes from a CSPRNG, fresh for every call.
/// Returns the file, its length in `*out_len`; null with `*out_err` set (JSON,
/// see above) on failure. `out_len` must not be null.
///
/// # Safety
/// Pointers are valid for the call; `phrase` is NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_backup_encrypt(
    data: *const u8,
    len: usize,
    phrase: *const c_char,
    entropy: *const u8,
    entropy_len: usize,
    out_len: *mut usize,
    out_err: *mut *mut c_char,
) -> *mut u8 {
    guard(ptr::null_mut(), || {
        if !out_err.is_null() {
            unsafe { *out_err = ptr::null_mut() };
        }
        if out_len.is_null() {
            return ptr::null_mut();
        }
        let phrase = unsafe { opt_str(phrase) }.unwrap_or("");
        let plain = unsafe { bytes_in(data, len) };
        let entropy = unsafe { bytes_in(entropy, entropy_len) };
        match backup::encrypt(plain, phrase, entropy) {
            Ok(v) => unsafe { bytes_out(v, out_len) },
            Err(e) => {
                if !out_err.is_null() {
                    unsafe { *out_err = err_json(&e) };
                }
                ptr::null_mut()
            }
        }
    })
}

/// Open an age file sealed with `phrase`. Returns the plaintext, its length in
/// `*out_len`; null with `*out_err` set on failure — `kind: "wrongPhrase"` is
/// the one to ask again on. `out_len` must not be null.
///
/// # Safety
/// Pointers are valid for the call; `phrase` is NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_backup_decrypt(
    data: *const u8,
    len: usize,
    phrase: *const c_char,
    out_len: *mut usize,
    out_err: *mut *mut c_char,
) -> *mut u8 {
    guard(ptr::null_mut(), || {
        if !out_err.is_null() {
            unsafe { *out_err = ptr::null_mut() };
        }
        if out_len.is_null() {
            return ptr::null_mut();
        }
        let phrase = unsafe { opt_str(phrase) }.unwrap_or("");
        let file = unsafe { bytes_in(data, len) };
        match backup::decrypt(file, phrase) {
            Ok(v) => unsafe { bytes_out(v, out_len) },
            Err(e) => {
                if !out_err.is_null() {
                    unsafe { *out_err = err_json(&e) };
                }
                ptr::null_mut()
            }
        }
    })
}

/// Five words for a new backup phrase, from at least
/// [`PLUMBLINE_BACKUP_PHRASE_ENTROPY_LEN`] bytes of CSPRNG output. Null if the
/// entropy is short. Caller-freed.
///
/// # Safety
/// `entropy` points at `entropy_len` readable bytes, or is null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_backup_phrase(entropy: *const u8, entropy_len: usize) -> *mut c_char {
    guard(ptr::null_mut(), || match backup::phrase(unsafe { bytes_in(entropy, entropy_len) }) {
        Ok(p) => out_string(p),
        Err(_) => ptr::null_mut(),
    })
}

/// 1 if `data` is an age file (an encrypted backup), 0 if not (a plain zip, say).
///
/// # Safety
/// `data` points at `len` readable bytes, or is null.
#[no_mangle]
pub unsafe extern "C" fn plumbline_backup_is_encrypted(data: *const u8, len: usize) -> i32 {
    guard(0, || backup::is_encrypted(unsafe { bytes_in(data, len) }) as i32)
}

/// Merge a backup into this device's study data (`plumbline_core::merge`), so
/// two devices' data add together rather than the backup's copy winning.
///
/// `request` is `{"local":{path:text}, "incoming":{path:text}}` — home-relative
/// paths (`notes/…`, `tags/…`) and each file's UTF-8 text, the device's and the
/// backup's. Answers `{"write":{path:text}, "delete":[path]}`: what to write over
/// the device and what to remove from it. Null only when `request` is not that
/// JSON.
///
/// # Safety
/// `request` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_backup_merge_json(request: *const c_char) -> *mut c_char {
    use std::collections::BTreeMap;
    #[derive(serde::Deserialize)]
    struct Req {
        local: BTreeMap<String, String>,
        incoming: BTreeMap<String, String>,
    }
    #[derive(serde::Serialize)]
    struct Resp {
        write: BTreeMap<String, String>,
        delete: Vec<String>,
    }
    guard(ptr::null_mut(), || {
        let Some(req) = unsafe { opt_str(request) }.and_then(|s| serde_json::from_str::<Req>(s).ok()) else {
            return ptr::null_mut();
        };
        let bytes = |m: BTreeMap<String, String>| m.into_iter().map(|(k, v)| (k, v.into_bytes())).collect();
        let m = plumbline_core::merge::merge(&bytes(req.local), &bytes(req.incoming));
        let write = m.write.into_iter().map(|(k, v)| (k, String::from_utf8_lossy(&v).into_owned())).collect();
        match serde_json::to_string(&Resp { write, delete: m.delete }) {
            Ok(s) => out_string(s),
            Err(_) => ptr::null_mut(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CStr, CString};

    fn take(p: *mut c_char) -> Option<String> {
        if p.is_null() {
            return None;
        }
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { crate::plumbline_string_free(p) };
        Some(s)
    }

    #[test]
    fn seal_and_open_over_the_abi() {
        let entropy: Vec<u8> = (0..48u8).collect();
        let phrase = CString::new("harbor pencil olive drum acid").unwrap();
        let plain = b"PK\x03\x04 the reader's zip";
        let mut len = 0usize;
        let mut err: *mut c_char = ptr::null_mut();
        let sealed = unsafe {
            plumbline_backup_encrypt(
                plain.as_ptr(),
                plain.len(),
                phrase.as_ptr(),
                entropy.as_ptr(),
                entropy.len(),
                &mut len,
                &mut err,
            )
        };
        assert!(!sealed.is_null() && err.is_null());
        assert_eq!(unsafe { plumbline_backup_is_encrypted(sealed, len) }, 1);
        assert_eq!(unsafe { plumbline_backup_is_encrypted(plain.as_ptr(), plain.len()) }, 0);

        let mut out_len = 0usize;
        let opened = unsafe { plumbline_backup_decrypt(sealed, len, phrase.as_ptr(), &mut out_len, &mut err) };
        assert!(!opened.is_null() && err.is_null());
        assert_eq!(unsafe { bytes_in(opened, out_len) }, plain);
        unsafe { plumbline_bytes_free(opened, out_len) };

        // The wrong phrase names itself, so the shell knows to ask again.
        let wrong = CString::new("harbor pencil olive drum acorn").unwrap();
        let none = unsafe { plumbline_backup_decrypt(sealed, len, wrong.as_ptr(), &mut out_len, &mut err) };
        assert!(none.is_null());
        let e: serde_json::Value = serde_json::from_str(&take(err).unwrap()).unwrap();
        assert_eq!(e["kind"], "wrongPhrase");
        assert!(e["message"].as_str().unwrap().contains("phrase"));
        unsafe { plumbline_bytes_free(sealed, len) };

        // A plain zip handed to decrypt is not an encrypted backup.
        let mut err: *mut c_char = ptr::null_mut();
        let none =
            unsafe { plumbline_backup_decrypt(plain.as_ptr(), plain.len(), phrase.as_ptr(), &mut out_len, &mut err) };
        assert!(none.is_null());
        let e: serde_json::Value = serde_json::from_str(&take(err).unwrap()).unwrap();
        assert_eq!(e["kind"], "notEncrypted");

        // Short entropy is refused rather than used.
        let mut err: *mut c_char = ptr::null_mut();
        let none = unsafe {
            plumbline_backup_encrypt(
                plain.as_ptr(),
                plain.len(),
                phrase.as_ptr(),
                entropy.as_ptr(),
                8,
                &mut len,
                &mut err,
            )
        };
        assert!(none.is_null());
        let e: serde_json::Value = serde_json::from_str(&take(err).unwrap()).unwrap();
        assert_eq!(e["kind"], "entropy");

        // Nulls are tolerated, not dereferenced.
        unsafe { plumbline_bytes_free(ptr::null_mut(), 0) };
        assert!(unsafe {
            plumbline_backup_encrypt(
                plain.as_ptr(),
                plain.len(),
                phrase.as_ptr(),
                entropy.as_ptr(),
                48,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        }
        .is_null());
    }

    #[test]
    fn the_header_constants_are_the_core_constants() {
        assert_eq!(PLUMBLINE_BACKUP_ENTROPY_LEN, backup::ENTROPY_LEN);
        assert_eq!(PLUMBLINE_BACKUP_PHRASE_ENTROPY_LEN, backup::PHRASE_ENTROPY_LEN);
    }

    #[test]
    fn a_merge_over_the_abi_keeps_both_devices_notes() {
        let note = |id: &str, text: &str| {
            format!(
                r#"{{"format":"plumbline-note-v2","id":"{id}","ref":"John 3:16","text":"{text}","created":"t","updated":"t"}}"#
            )
        };
        let req = serde_json::json!({
            "local": {"notes/john-3-16-a.json": note("a", "mine")},
            "incoming": {"notes/john-3-16-b.json": note("b", "theirs"), "notes/john-3-16-a.json": note("a", "mine")},
        });
        let c = CString::new(req.to_string()).unwrap();
        let out: serde_json::Value =
            serde_json::from_str(&take(unsafe { plumbline_backup_merge_json(c.as_ptr()) }).unwrap()).unwrap();
        assert_eq!(out["write"].as_object().unwrap().keys().collect::<Vec<_>>(), ["notes/john-3-16-b.json"]);
        assert_eq!(out["delete"], serde_json::json!([]));
        assert!(unsafe { plumbline_backup_merge_json(ptr::null()) }.is_null());
        let bad = CString::new("not json").unwrap();
        assert!(unsafe { plumbline_backup_merge_json(bad.as_ptr()) }.is_null());
    }

    #[test]
    fn a_phrase_over_the_abi() {
        let entropy = [7u8, 1, 2, 3, 4, 5, 6, 7];
        let p = take(unsafe { plumbline_backup_phrase(entropy.as_ptr(), entropy.len()) }).unwrap();
        assert_eq!(p.split(' ').count(), 5);
        assert_eq!(p, take(unsafe { plumbline_backup_phrase(entropy.as_ptr(), entropy.len()) }).unwrap());
        assert!(unsafe { plumbline_backup_phrase(entropy.as_ptr(), 3) }.is_null());
        assert!(unsafe { plumbline_backup_phrase(ptr::null(), 0) }.is_null());
    }
}
