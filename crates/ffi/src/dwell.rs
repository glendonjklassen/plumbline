//! The reading-time tracker's C ABI — a shell reports elapsed seconds, the core
//! decides what they were worth.
//!
//! Grace, idle, tail-banking and the thresholds all live in the core; a shell
//! owns only its clock and its window.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::{canon, reading};

use crate::{guard, opt_str, out_json, wire, PlumblineEngine};

/// The reading map's tuning as JSON: `{wordsPerMinute, completeAt, freshDays,
/// staleDays, graceSeconds, tickSeconds, idleSeconds}`. Engine-independent and
/// free, unlike the same object riding on `reading_books_json`. Never null.
#[no_mangle]
pub extern "C" fn plumbline_reading_spec_json() -> *mut c_char {
    guard(ptr::null_mut(), || out_json(&reading::spec()))
}

/// One sample of reading time.
///
/// `book`/`chapter` are what is on screen; a NULL `book` means nothing is being
/// read right now (a dialog is up, the app is going to the background, the
/// reader left the chapter) and banks the tail. `reached` is the deepest verse
/// of that chapter the reader has scrolled to, `step_seconds` the seconds this
/// sample covers (a shell passes its own sample interval; the core clamps it),
/// and `interacted` whether anything was touched since the last sample.
///
/// Most calls answer null. When the core decides the banked seconds are worth
/// writing down it records them and answers the same
/// `{book,chapter,pct,completed,lastRead?}` `reading_record_json` does, so a
/// shell reacts to `completed` in exactly one place.
///
/// Null also when the engine has no home to write to — reading is simply not
/// tracked then.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_reading_tick_json(
    engine: *mut PlumblineEngine,
    book: *const c_char,
    chapter: u32,
    reached: u32,
    step_seconds: f32,
    interacted: bool,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(now)) = (engine.as_mut(), opt_str(now)) else {
            return ptr::null_mut();
        };
        // An unknown book is "nothing is being read": the tail still banks, so rubbish from a
        // shell loses the seconds rather than crediting a chapter that does not exist.
        let target =
            opt_str(book).filter(|b| canon::book_by_id(b).is_some()).map(|b| (b, chapter.min(u16::MAX as u32) as u16));
        let reached = reached.min(u16::MAX as u32) as u16;
        let Ok(mut tracker) = e.dwell.lock() else { return ptr::null_mut() };
        let Some(report) = tracker.tick(target, reached, interacted, step_seconds) else {
            return ptr::null_mut();
        };
        // Hold the lock no longer than the decision: the write below touches the disk, and a
        // shell may tick off its own thread.
        drop(tracker);

        let Some(home) = e.home.clone() else { return ptr::null_mut() };
        // Stamp the start date here too: reading happens long before anyone opens the
        // navigator, and the anchor should be the earlier of the two.
        let _ = reading::ensure_since(&home, now);
        let words = e.reading_words.get_or_init(|| reading::ChapterWords::build(&e.corpus));
        match reading::record(
            &home,
            &e.corpus,
            words,
            &report.book,
            report.chapter,
            report.reached,
            report.seconds,
            now,
        ) {
            Ok(recorded) => out_json(&wire::WireReadingRecorded { recorded }),
            Err(_) => ptr::null_mut(),
        }
    })
}
