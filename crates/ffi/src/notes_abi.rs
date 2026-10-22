//! The reader's own notes over the ABI — any number per verse, each with an id.
//!
//! **Breaking (v0.78.0):** `plumbline_engine_user_note_set(ref, text, stamp)`
//! is gone, and `plumbline_engine_user_note_json` takes a note id, not a ref:
//! a verse no longer has *the* note. Add with [`plumbline_engine_user_note_add`],
//! change or delete one note by id.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::{store, tombstone, usernote, VRef};

use crate::{guard, guard_err, load_study, opt_str, out_json, out_string, record_deleted, wire, PlumblineEngine};

fn wire_note(ln: &usernote::LoadedNote) -> wire::WireUserNote {
    wire::WireUserNote {
        id: ln.note.id.clone(),
        verse: ln.note.vref.ref_key(),
        display: ln.note.vref.display(),
        text: ln.note.text.clone(),
        created: ln.note.created.clone(),
        updated: ln.note.updated.clone(),
    }
}

/// All the reader's notes as JSON (`{notes:[{id,verse,display,text,created,
/// updated}]}`), in canonical reading order and oldest first on a verse. Never
/// null on a live engine (no notes → empty list).
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_user_notes_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = (unsafe { engine.as_ref() }) else { return ptr::null_mut() };
        let study = e.study_read();
        out_json(&wire::WireUserNotes { notes: study.user_notes.iter().map(wire_note).collect() })
    })
}

/// One note by id as JSON (`{id,verse,display,text,created,updated}`), or null
/// when there is no such note.
///
/// # Safety
/// `engine` is a live engine; `id` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_user_note_json(
    engine: *const PlumblineEngine,
    id: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(id)) = (unsafe { engine.as_ref() }, unsafe { opt_str(id) }) else {
            return ptr::null_mut();
        };
        match e.study_read().user_notes.iter().find(|n| n.note.id == id) {
            Some(ln) => out_json(&wire_note(ln)),
            None => ptr::null_mut(),
        }
    })
}

/// Add a new note on the verse `ref_key`. `stamp` is a caller-supplied UTC
/// timestamp. Empty text is refused. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated
/// UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_user_note_add(
    engine: *mut PlumblineEngine,
    ref_key: *const c_char,
    text: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = (unsafe { engine.as_mut() }) else { return out_string("null engine".to_string()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let args = unsafe { (opt_str(ref_key), opt_str(text), opt_str(stamp)) };
        let (Some(rk), Some(text), Some(stamp)) = args else {
            return out_string("null or invalid argument".to_string());
        };
        let Some(v) = VRef::parse_ref_key(rk) else { return out_string(format!("bad ref: {rk}")) };
        let mut study = engine.study_write();
        match usernote::add_note(&home, &v, text, &store::new_id(), stamp) {
            Ok(_) => {
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Replace the text of note `id`. Empty or whitespace text **deletes** it.
/// `stamp` is a caller-supplied UTC timestamp. Null on success, else an owned
/// error string.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated
/// UTF-8 for the call.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_user_note_edit(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    text: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = (unsafe { engine.as_mut() }) else { return out_string("null engine".to_string()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let args = unsafe { (opt_str(id), opt_str(text), opt_str(stamp)) };
        let (Some(id), Some(text), Some(stamp)) = args else {
            return out_string("null or invalid argument".to_string());
        };
        let mut study = engine.study_write();
        match usernote::edit_note(&home, &study.user_notes, id, text, stamp) {
            Ok(written) => {
                if written.is_none() {
                    record_deleted(&engine.home, vec![tombstone::note(id)]);
                }
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Delete note `id`. A missing note is a success. Null on success, else an
/// owned error string.
///
/// # Safety
/// `engine` is a live engine; `id` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_user_note_delete(
    engine: *mut PlumblineEngine,
    id: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = (unsafe { engine.as_mut() }) else { return out_string("null engine".to_string()) };
        if engine.home.is_none() {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        }
        let Some(id) = (unsafe { opt_str(id) }) else { return out_string("null or invalid argument".to_string()) };
        let mut study = engine.study_write();
        match usernote::delete_note(&study.user_notes, id) {
            Ok(removed) => {
                if removed {
                    record_deleted(&engine.home, vec![tombstone::note(id)]);
                }
                *study = load_study(&engine.home);
                ptr::null_mut()
            }
            Err(e) => out_string(e.to_string()),
        }
    })
}
