//! Devotionals — the C ABI. A sibling of `plans.rs` for the same reason:
//! `lib.rs` is past the no-3k-line rule, and cbindgen walks the whole crate, so
//! the header is unchanged by the split.
//!
//! Runs load FRESH from `home/devotionals/` on every call (the small-set stance
//! the plan endpoints take): a run file is tens of bytes and a reader has one or
//! two. The CATALOGUE is cached on the engine instead — it is the shipped
//! booklet text, tens of kilobytes, and never changes under a running app.
//!
//! All of these tolerate an engine opened from bytes (no home): the list reads
//! empty, authoring returns the standard "no home" error.
//!
//! **`today` is the reader's own LOCAL `YYYY-MM-DD`,** not a UTC instant. It is
//! the whole of the pacing rule (`core::devotional`), so a UTC date would roll
//! the next entry over at the wrong hour for most of the world. The shell
//! computes it the same way it does for the seating slots.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::devotional::{self, Devotional, Run};
use plumbline_core::i18n;
use serde::Serialize;

use crate::{guard, guard_err, opt_str, out_json, out_string, PlumblineEngine};

// ── wire DTOs ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireDevotionals {
    /// The booklets the reader is running, with derived state.
    running: Vec<WireRunning>,
    /// The catalogue every picker offers.
    catalogue: Vec<WireBooklet>,
}

/// A booklet as the picker shows it — its own words, already resolved to the
/// reader's language, so no shell re-implements the fallback.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireBooklet {
    id: String,
    name: String,
    days: u32,
    /// Whether the new-believer welcome starts this one (see core::devotional).
    new_believer: bool,
    /// Whether this booklet has been translated into the reader's language. A
    /// booklet is still OFFERED without it (reading it in English beats not
    /// being offered it), so this is for saying so, not for filtering.
    translated: bool,
    sections: Vec<WireSection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireSection {
    from: u32,
    to: u32,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireRunning {
    id: String,
    name: String,
    /// When the booklet was started — with the name, how a set-aside run is
    /// introduced ("New Believer Devotional · started 3 Aug").
    started: String,
    /// Set aside, kept whole: holds its place, asks nothing (no chip).
    paused: bool,
    days_total: u32,
    days_done: u32,
    /// The day now open (null once every day is banked — a finished booklet).
    #[serde(skip_serializing_if = "Option::is_none")]
    today: Option<WireToday>,
}

/// The open day, whole: everything the reader's page paints, in one answer, so
/// opening a devotional is ONE round trip and not four.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireToday {
    day: u32,
    /// Whether the entry is INVITED today. False for the rest of the day after
    /// a Done — the page still opens, but no chip asks for it.
    available: bool,
    title: String,
    /// The passages, structured — the shell renders the text from its own
    /// corpus and prints the label in the reader's language.
    scripture: Vec<WireRef>,
    reflection: Vec<String>,
    activity: String,
    /// The section this day sits in, if the booklet is sectioned.
    #[serde(skip_serializing_if = "Option::is_none")]
    section: Option<WireSection>,
    /// The booklet's send-off, present ONLY on its last day — the closing note
    /// belongs at the foot of day 30, not on a page of its own.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    closing: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireRef {
    book: String,
    chapter: u16,
    verse: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    end: Option<u16>,
}

fn wire_refs(refs: &[devotional::ScriptureRef]) -> Vec<WireRef> {
    refs.iter().map(|r| WireRef { book: r.book.clone(), chapter: r.chapter, verse: r.verse, end: r.end }).collect()
}

fn wire_section(s: &devotional::Section) -> WireSection {
    WireSection { from: s.from, to: s.to, title: s.title.clone() }
}

/// The booklet's name in `lang`, or its id when a catalogue somehow carries no
/// text at all — a name is what every list shows, so it must never be blank.
fn booklet_name(d: &Devotional, lang: &str) -> String {
    d.text(lang).map(|t| t.name.clone()).unwrap_or_else(|| d.id.clone())
}

/// One run's derived state. A run whose booklet is no longer in the catalogue
/// (a pack rolled back, an id retired) answers `None` rather than a row with no
/// content: the reader's file stays where it is, and the shell simply has
/// nothing to draw.
fn running_state(run: &Run, catalogue: &[Devotional], lang: &str, today: &str) -> Option<WireRunning> {
    let d = catalogue.iter().find(|d| d.id == run.id)?;
    let open = devotional::next_day(run, d.days, today);
    Some(WireRunning {
        id: run.id.clone(),
        name: booklet_name(d, lang),
        started: run.started.clone(),
        paused: run.paused,
        days_total: d.days,
        days_done: open.as_ref().map(|t| t.days_done).unwrap_or(d.days),
        today: open.and_then(|t| {
            let entry = d.entry(t.day)?;
            let text = entry.text(lang)?;
            Some(WireToday {
                day: t.day,
                available: t.available,
                title: text.title.clone(),
                scripture: wire_refs(&entry.scripture),
                reflection: text.reflection.clone(),
                activity: text.activity.clone(),
                section: d.section(t.day).map(wire_section),
                closing: if t.day == d.days {
                    d.text(lang).map(|b| b.closing.clone()).unwrap_or_default()
                } else {
                    Vec::new()
                },
            })
        }),
    })
}

// ── the ABI ───────────────────────────────────────────────────────────────────

/// Every running devotional with its open day, plus the catalogue every picker
/// offers, as `{running:[…], catalogue:[…]}`. Never null on a live engine.
///
/// `lang` selects the text (falling back to English per entry); null reads as
/// English. `today` is the reader's LOCAL `YYYY-MM-DD`; null reads as "no day",
/// which leaves every open entry `available` — the permissive direction, since
/// the cost of a missing date should never be a reader locked out.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_devotionals_json(
    engine: *const PlumblineEngine,
    lang: *const c_char,
    today: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let catalogue = e.devotionals();
        let lang = opt_str(lang).unwrap_or(devotional::BASE_LANG);
        let today = opt_str(today).unwrap_or_default();
        let runs = e.home.as_ref().map(|h| devotional::load_runs(h).0).unwrap_or_default();
        out_json(&WireDevotionals {
            running: runs.iter().filter_map(|r| running_state(r, catalogue, lang, today)).collect(),
            catalogue: catalogue
                .iter()
                .map(|d| WireBooklet {
                    id: d.id.clone(),
                    name: booklet_name(d, lang),
                    days: d.days,
                    new_believer: d.new_believer,
                    translated: d.has_lang(lang),
                    sections: d.sections.iter().map(wire_section).collect(),
                })
                .collect(),
        })
    })
}

/// One day of a booklet, whether or not it is the open one — what the reader's
/// page paints when they browse back to day 3. Null for a day the booklet does
/// not have, or an unknown id.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_devotional_day_json(
    engine: *const PlumblineEngine,
    id: *const c_char,
    day: u32,
    lang: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let Some(e) = engine.as_ref() else { return ptr::null_mut() };
        let Some(id) = opt_str(id) else { return ptr::null_mut() };
        let lang = opt_str(lang).unwrap_or(devotional::BASE_LANG);
        let Some(d) = e.devotionals().iter().find(|d| d.id == id) else { return ptr::null_mut() };
        let Some(entry) = d.entry(day) else { return ptr::null_mut() };
        let Some(text) = entry.text(lang) else { return ptr::null_mut() };
        out_json(&WireToday {
            day,
            // A day reached by browsing is always readable; "available" is about
            // what the chip INVITES, and only the open day has an invitation.
            available: true,
            title: text.title.clone(),
            scripture: wire_refs(&entry.scripture),
            reflection: text.reflection.clone(),
            activity: text.activity.clone(),
            section: d.section(day).map(wire_section),
            closing: if day == d.days {
                d.text(lang).map(|b| b.closing.clone()).unwrap_or_default()
            } else {
                Vec::new()
            },
        })
    })
}

/// Start a devotional by its catalogue `id`. Starting one already running is a
/// no-op, NOT a re-seed: a reader who taps Start again from a stale list must
/// not lose 12 days of progress, and there is no class exclusivity here to
/// force a replacement (a reader may run two booklets at once). Null on
/// success, else an owned error string.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_devotional_start(
    engine: *mut PlumblineEngine,
    id: *const c_char,
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
        if !engine.devotionals().iter().any(|d| d.id == id) {
            return out_string(format!("unknown devotional: {id}"));
        }
        if devotional::load_runs(&home).0.iter().any(|r| r.id == id) {
            return ptr::null_mut(); // already running: keep the reader's progress
        }
        let lang = i18n::stamp();
        let run = Run::new(id, now, (!lang.is_empty()).then_some(lang.as_str()));
        match devotional::write_run(&home, &run) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Stop a devotional, removing its run file and its progress. An absent run is
/// a no-op, not an error (the `plan_stop` stance). Null on success, else an
/// owned error string.
///
/// # Safety
/// `engine` is valid; `id` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_devotional_stop(
    engine: *mut PlumblineEngine,
    id: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let Some(id) = opt_str(id) else { return out_string("null or invalid argument".into()) };
        match devotional::remove_run(&home, id) {
            Ok(_) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Bank day `day` of a running devotional on the reader's LOCAL `today`
/// (`YYYY-MM-DD`) — the Done at the foot of the page, and the only signal that
/// a day was read. Banking a day already banked is a no-op that does NOT
/// re-stamp the date, so a double tap cannot push tomorrow's entry further
/// away. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_devotional_done(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    day: u32,
    today: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let (Some(id), Some(today)) = (opt_str(id), opt_str(today)) else {
            return out_string("null or invalid argument".into());
        };
        let Some(days) = engine.devotionals().iter().find(|d| d.id == id).map(|d| d.days) else {
            return out_string(format!("unknown devotional: {id}"));
        };
        if day < 1 || day > days {
            return out_string(format!("{id} has no day {day}"));
        }
        let Some(mut run) = devotional::load_runs(&home).0.into_iter().find(|r| r.id == id) else {
            return out_string(format!("no running devotional: {id}"));
        };
        if !devotional::mark_done(&mut run, day, today) {
            return ptr::null_mut(); // already banked: not an error, and no re-stamp
        }
        match devotional::write_run(&home, &run) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Pause or resume a devotional — set aside, kept whole: its file and its
/// banked days stay put, and it stops asking (no chip) while `paused`. An
/// absent id is an error: pausing one that is not running means the shell's
/// list is stale, and saying so beats a silent no-op (the `plan_set_paused`
/// stance). Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; `id` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_devotional_set_paused(
    engine: *mut PlumblineEngine,
    id: *const c_char,
    paused: bool,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else { return out_string("null engine".into()) };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".into());
        };
        let Some(id) = opt_str(id) else { return out_string("null or invalid argument".into()) };
        let Some(mut run) = devotional::load_runs(&home).0.into_iter().find(|r| r.id == id) else {
            return out_string(format!("no running devotional: {id}"));
        };
        if run.paused == paused {
            return ptr::null_mut(); // already there: a double-tap is not an error
        }
        run.paused = paused;
        match devotional::write_run(&home, &run) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}
