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
    /// One of the `kinds` ids.
    kind: &'static str,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireKind {
    id: &'static str,
    answerable: bool,
    /// The refKey of the example's first verse — where a tap goes.
    example: &'static str,
    /// The example as a label, a refKey or a verse range (`Ps 145:1–3`).
    example_label: String,
}

#[derive(Serialize)]
struct WirePassage {
    /// The passage as a label, a refKey or a verse range (`Ps 51:1–4`).
    label: String,
    /// The refKey of every verse in it, in order.
    verses: Vec<String>,
}

/// `Ps 51:1` to verse 4 → the label and `Ps 51:1` … `Ps 51:4`.
fn passage(first: &str, to: Option<u8>) -> WirePassage {
    let (head, verse) = first.rsplit_once(':').unwrap_or((first, ""));
    let from: u8 = verse.parse().unwrap_or(1);
    WirePassage {
        label: label(first, to),
        verses: (from..=to.unwrap_or(from)).map(|v| format!("{head}:{v}")).collect(),
    }
}

fn label(first: &str, to: Option<u8>) -> String {
    match to {
        Some(to) => format!("{first}\u{2013}{to}"),
        None => first.to_string(),
    }
}

#[derive(Serialize, Default)]
struct WirePrayers {
    /// On the list, the latest added first.
    praying: Vec<WirePrayer>,
    /// Answered, the latest answer first.
    answered: Vec<WirePrayer>,
    /// Every type of prayer, in picker order.
    kinds: Vec<WireKind>,
    /// The verses shown for Confession, which is not a type and is never added.
    confession: Vec<WirePassage>,
}

#[derive(Serialize)]
struct WirePrayerPace {
    windows: Vec<prayer::PrayerWindow>,
}

fn wire(p: &prayer::Prayer, today: &str) -> WirePrayer {
    WirePrayer {
        id: p.id.clone(),
        title: p.title.clone(),
        kind: prayer::kind_of(&p.kind).id,
        created: p.created.clone(),
        prayed_days: p.prayed.len(),
        prayed_today: p.prayed.iter().any(|d| d == today),
        last_prayed: p.prayed.last().cloned(),
        answered: p.answered.clone(),
        answer: p.answer.clone(),
    }
}

/// The prayer list at `today` (`YYYY-MM-DD`) as
/// `{praying:[…],answered:[…],kinds:[…],confession:[{label,verses}]}`, each prayer
/// `{id,title,kind,created,prayedDays,prayedToday,lastPrayed?,answered?,
/// answer?}` and each kind `{id,answerable,example,exampleLabel}`. Both lists
/// are newest first: `praying` by the day added, `answered` by the day
/// answered. Never null on a live engine.
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
        let kinds = prayer::KINDS
            .iter()
            .map(|k| WireKind {
                id: k.id,
                answerable: k.answerable,
                example: k.example,
                example_label: label(k.example, k.example_to),
            })
            .collect();
        let confession = prayer::CONFESSION.iter().map(|(first, to)| passage(first, *to)).collect();
        let mut out = WirePrayers { kinds, confession, ..WirePrayers::default() };
        let all = e.home.as_ref().map(|h| prayer::load(h).0).unwrap_or_default();
        for lp in &all {
            let p = &lp.prayer;
            let list = if p.answered.is_some() { &mut out.answered } else { &mut out.praying };
            list.push(wire(p, today));
        }
        // `load` answers oldest first; both lists show the newest first. The
        // sort is stable, so two answered on one day stay newest-added first.
        out.praying.reverse();
        out.answered.reverse();
        out.answered.sort_by(|a, b| b.answered.cmp(&a.answered));
        out_json(&out)
    })
}

/// Prayer activity over the reading pace's windows, as
/// `{windows:[{id,days,prayedDays}]}` — [`prayer::pace`] over the home's
/// prayers at `today` (`YYYY-MM-DD`). No prayers, or no home, answers no
/// windows. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine (or null → null); `today` is null or valid
/// NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_pace_json(
    engine: *const PlumblineEngine,
    today: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let all = e.home.as_ref().map(|h| prayer::load(h).0).unwrap_or_default();
        let windows = opt_str(today).and_then(|t| prayer::pace(&all, t)).unwrap_or_default();
        out_json(&WirePrayerPace { windows })
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

/// Add a prayer of type `kind` (one of the `kinds` ids). `stamp` is a
/// caller-supplied UTC timestamp. An empty title and an unknown type are
/// refused. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_add(
    engine: *mut PlumblineEngine,
    title: *const c_char,
    kind: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [title, kind, stamp]) {
        Ok((_, home, [title, kind, stamp])) => {
            done(prayer::add(&home, title, kind, &store::new_id(), stamp).map(|_| ()))
        }
        Err(e) => e,
    })
}

/// Replace prayer `id`'s title and type. An answered prayer cannot take a type
/// that is not answerable. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is a live engine; the string args are valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_prayer_edit(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    title: *const c_char,
    kind: *const c_char,
    stamp: *const c_char,
) -> *mut c_char {
    guard_err(|| match authoring(engine, [id, title, kind, stamp]) {
        Ok((_, home, [id, title, kind, stamp])) => done(prayer::edit(&home, id, title, kind, stamp)),
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
/// `comment` (empty for none). A type that is not answerable is refused. Null on success, else an owned error string.
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
