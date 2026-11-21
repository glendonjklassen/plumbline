//! The prayer list over the ABI — `core::prayer`'s files, read and authored.
//!
//! Read straight from the home on every call, like the hymn favourites: the
//! list is small and nothing else in the engine depends on it, so it is not
//! part of the reloadable study state.
//!
//! Days (`today`, `day`) are the reader's own calendar days,
//! `YYYY-MM-DD`; stamps are UTC. An engine opened from bytes (no home) reads an
//! empty list and refuses the writes with the standard "no home" error.

use std::ffi::c_char;
use std::path::PathBuf;
use std::ptr;

use plumbline_core::{prayer, store, tombstone};
use serde::Serialize;

use crate::{guard, guard_err, opt_str, out_json, out_string, record_deleted, PlumblineEngine};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WirePrayer {
    id: String,
    title: String,
    created: String,
    /// How many days it has been prayed for.
    prayed_days: usize,
    prayed_today: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_prayed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    answered: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    answer: Option<String>,
}

#[derive(Serialize, Default)]
struct WirePrayers {
    /// On the list, oldest first.
    praying: Vec<WirePrayer>,
    /// Answered, the latest answer first.
    answered: Vec<WirePrayer>,
}

fn wire(p: &prayer::Prayer, today: &str) -> WirePrayer {
    WirePrayer {
        id: p.id.clone(),
        title: p.title.clone(),
        created: p.created.clone(),
        prayed_days: p.prayed.len(),
        prayed_today: p.prayed.iter().any(|d| d == today),
        last_prayed: p.prayed.last().cloned(),
        answered: p.answered.clone(),
        answer: p.answer.clone(),
    }
}

/// The prayer list at `today` (`YYYY-MM-DD`) as `{praying:[…],answered:[…]}`,
/// each `{id,title,created,prayedDays,prayedToday,lastPrayed?,answered?,
/// answer?}`. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null); `today` is null or valid
/// NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayers_json(
    engine: *const PlumblineEngine,
    today: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let today = opt_str(today).unwrap_or("");
        let mut out = WirePrayers::default();
        let all = e.home.as_ref().map(|h| prayer::load(h).0).unwrap_or_default();
        for lp in &all {
            let p = &lp.prayer;
            let list = if p.answered.is_some() { &mut out.answered } else { &mut out.praying };
            list.push(wire(p, today));
        }
        // Stable, so two answered on one day keep the order they were added in.
        out.answered.sort_by(|a, b| b.answered.cmp(&a.answered));
        out_json(&out)
    })
}

/// The engine's home and the call's string arguments, or the error to return.
unsafe fn authoring<'a, const N: usize>(
    engine: *mut PlumblineEngine,
    args: [*const c_char; N],
) -> Result<(&'a mut PlumblineEngine, PathBuf, [&'a str; N]), *mut c_char> {
    let Some(engine) = engine.as_mut() else { return Err(out_string("null engine".into())) };
    let Some(home) = engine.home.clone() else {
        return Err(out_string("engine has no home directory (opened from bytes); cannot author".into()));
    };
    let mut out = [""; N];
    for (slot, arg) in out.iter_mut().zip(args) {
        match opt_str(arg) {
            Some(s) => *slot = s,
            None => return Err(out_string("null or invalid argument".into())),
        }
    }
    Ok((engine, home, out))
}

fn done(result: Result<(), plumbline_core::Error>) -> *mut c_char {
    match result {
        Ok(()) => ptr::null_mut(),
        Err(e) => out_string(e.to_string()),
    }
}

/// Add a prayer. `stamp` is a caller-supplied UTC timestamp. An empty title is refused. Null
/// on success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_add(
    engine: *mut PlumblineEngine,
    title: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [title, stamp]) {
        Ok((_, home, [title, stamp])) => done(prayer::add(&home, title, &store::new_id(), stamp).map(|_| ())),
        Err(e) => e,
    })
}

/// Replace prayer `id`'s title. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_edit(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    title: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [id, title, stamp]) {
        Ok((_, home, [id, title, stamp])) => done(prayer::edit(&home, id, title, stamp)),
        Err(e) => e,
    })
}

/// Record that prayer `id` was prayed for on `day` (`YYYY-MM-DD`). Repeating a
/// day is a harmless no-op. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_prayed(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    day: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [id, day]) {
        Ok((_, home, [id, day])) => done(prayer::mark_prayed(&home, id, day)),
        Err(e) => e,
    })
}

/// Mark prayer `id` answered on `day` (`YYYY-MM-DD`), with an optional
/// `comment` (empty for none). Null on success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_answer(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    comment: *const c_char,
    day: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [id, comment, day, stamp]) {
        Ok((_, home, [id, comment, day, stamp])) => done(prayer::answer(&home, id, Some(comment), day, stamp)),
        Err(e) => e,
    })
}

/// Move answered prayer `id` back to the list, dropping its answer. Null on
/// success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_reopen(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [id, stamp]) {
        Ok((_, home, [id, stamp])) => done(prayer::reopen(&home, id, stamp)),
        Err(e) => e,
    })
}

/// Delete prayer `id`, recording the deletion so a restore does not bring it
/// back. A missing prayer is a success. Null on success, else an owned error
/// string.
///
/// # Safety
/// `engine` is a live engine; `id` is valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_delete(
    engine: *mut PlumblineEngine,
    id: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [id]) {
        Ok((engine, home, [id])) => match prayer::delete(&home, id) {
            Ok(Some(updated)) => {
                record_deleted(&engine.home, vec![tombstone::prayer(id)], &updated);
                ptr::null_mut()
            }
            Ok(None) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        },
        Err(e) => e,
    })
}
