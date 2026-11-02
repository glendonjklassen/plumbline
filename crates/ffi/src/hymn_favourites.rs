//! The reader's favourite hymns — the C ABI over `core::hymnal`'s favourites
//! file. Its own module rather than beside the hymnal reads in `lib.rs`, which
//! is past the tree's file-size rule already.
//!
//! Both tolerate an engine opened from bytes (no home): the list reads empty,
//! the write returns the standard "no home" error.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::{hymnal, tombstone};
use serde::Serialize;

use crate::{guard, guard_err, opt_str, out_json, out_string, PlumblineEngine};

#[derive(Serialize)]
struct WireFavourites {
    /// Hymn ids, in the order they were starred (earliest first).
    ids: Vec<String>,
}

/// The favourite hymns as `{ids:[…]}`, earliest star first. Never null on a
/// live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null).
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_hymn_favourites_json(engine: *const PlumblineEngine) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let favs = e.home.as_ref().map(hymnal::load_favourites).unwrap_or_default();
        let mut by_stamp: Vec<(String, String)> = favs.into_iter().map(|(id, stamp)| (stamp, id)).collect();
        by_stamp.sort();
        out_json(&WireFavourites { ids: by_stamp.into_iter().map(|(_, id)| id).collect() })
    })
}

/// Star (`on`) or unstar a hymn by its hymnal `id` at `now`. Unstarring records
/// a deletion, so a restore does not bring the star back. Repeating the current
/// state is a harmless no-op. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_hymn_favourite_set(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    on: bool,
    now: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let (Some(id), Some(now)) = (opt_str(id), opt_str(now)) else {
            return out_string("null or invalid argument".into());
        };
        if id.trim().is_empty() {
            return out_string("a favourite needs a hymn id".into());
        }
        match hymnal::set_favourite(&home, id, on, now) {
            Ok(Some(before)) if !on => {
                crate::record_deleted(&engine.home, vec![tombstone::hymn_favourite(id)], &before);
                ptr::null_mut()
            }
            Ok(_) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}
