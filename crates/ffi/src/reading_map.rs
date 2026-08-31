//! The reading map's C ABI — where the reader has been, and how long ago.
//!
//! All of these tolerate an engine with no home (opened from bytes): the map
//! reads as "nothing recorded" rather than failing, so the navigator still opens.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::{canon, reading};

use crate::{guard, guard_err, opt_str, out_json, out_string, wire, PlumblineEngine};

impl PlumblineEngine {
    /// Chapter word counts, built once. `pub(crate)` because the plans module
    /// derives day chapters against the same table.
    pub(crate) fn reading_words(&self) -> &reading::ChapterWords {
        self.reading_words.get_or_init(|| reading::ChapterWords::build(&self.corpus))
    }

    /// The reader's start date, creating it at `now` on the first call that needs it. It anchors
    /// the unread glow, so `record` stamps it too, not only the navigator. With no home there is
    /// nothing to persist: `now` stands in and every unread chapter reads as brand new.
    fn reading_since(&self, now: &str) -> String {
        match self.home.as_ref() {
            Some(h) => reading::ensure_since(h, now).unwrap_or_else(|_| now.to_string()),
            None => now.to_string(),
        }
    }

    fn reading_store(&self) -> reading::Store {
        self.home.as_ref().map(|h| reading::load(h).0).unwrap_or_default()
    }
}

/// Every book's reading standing at `now` (RFC3339), canon order, as
/// `{books:[…],since,spec}`. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; `now` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_reading_books_json(
    engine: *const PlumblineEngine,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(now)) = (engine.as_ref(), opt_str(now)) else {
            return ptr::null_mut();
        };
        let since = e.reading_since(now);
        let store = e.reading_store();
        out_json(&wire::WireReadingBooks {
            books: reading::books(&e.corpus, e.reading_words(), &store, now),
            since,
            spec: reading::spec(),
        })
    })
}

/// One book's chapters at `now`, chapter order, as `{book,chapters:[…],since,spec}`.
/// Null for an unknown book id.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_reading_chapters_json(
    engine: *const PlumblineEngine,
    book: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(book), Some(now)) = (engine.as_ref(), opt_str(book), opt_str(now)) else {
            return ptr::null_mut();
        };
        if canon::book_by_id(book).is_none() {
            return ptr::null_mut();
        }
        let since = e.reading_since(now);
        let store = e.reading_store();
        out_json(&wire::WireReadingChapters {
            book: book.to_string(),
            chapters: reading::book_chapters(&e.corpus, e.reading_words(), &store, book, now),
            since,
            spec: reading::spec(),
        })
    })
}

/// Credit reading time to a chapter and persist it.
///
/// `reached` is the furthest verse number the reader has had on screen and `seconds` the dwell
/// since the last call, reported on the cadence in `spec.tickSeconds` plus on leaving the chapter
/// and on going to the background.
///
/// Returns the resulting `{book,chapter,pct,completed,lastRead?}`, or null when
/// the engine has no home to write to (reading is simply not tracked then).
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_reading_record_json(
    engine: *mut PlumblineEngine,
    book: *const c_char,
    chapter: u32,
    reached: u32,
    seconds: f32,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(book), Some(now)) = (engine.as_mut(), opt_str(book), opt_str(now)) else {
            return ptr::null_mut();
        };
        let Some(home) = e.home.clone() else { return ptr::null_mut() };
        if canon::book_by_id(book).is_none() || !seconds.is_finite() {
            return ptr::null_mut();
        }
        // Stamp the start date here too: reading happens long before anyone opens the
        // navigator, and the anchor should be the earlier of the two.
        let _ = e.reading_since(now);
        let words = e.reading_words();
        let (chapter, reached) = (chapter as u16, reached.min(u16::MAX as u32) as u16);
        match reading::record(&home, &e.corpus, words, book, chapter, reached, seconds, now) {
            Ok(recorded) => out_json(&wire::WireReadingRecorded { recorded }),
            Err(_) => ptr::null_mut(),
        }
    })
}

/// Log a chapter as read on `date` (`YYYY-MM-DD`, or any RFC3339 stamp — only
/// the day is kept) — the by-hand affordance for reading done in a paper Bible.
/// Full credit. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_reading_mark_read(
    engine: *mut PlumblineEngine,
    book: *const c_char,
    chapter: u32,
    date: *const c_char,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let (Some(book), Some(date)) = (opt_str(book), opt_str(date)) else {
            return out_string("null or invalid argument".to_string());
        };
        if canon::book_by_id(book).is_none() {
            return out_string(format!("unknown book: {book}"));
        }
        match reading::mark_read(&home, book, chapter as u16, date) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}

/// Drop a chapter's reading record — the way back out of a date set by mistake;
/// the chapter returns to unread. Null on success, else an owned error string.
///
/// # Safety
/// `engine` is valid; `book` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_reading_forget(
    engine: *mut PlumblineEngine,
    book: *const c_char,
    chapter: u32,
) -> *mut c_char {
    guard_err(|| {
        let Some(engine) = engine.as_mut() else {
            return out_string("null engine".to_string());
        };
        let Some(home) = engine.home.clone() else {
            return out_string("engine has no home directory (opened from bytes); cannot author".to_string());
        };
        let Some(book) = opt_str(book) else {
            return out_string("null or invalid argument".to_string());
        };
        match reading::forget(&home, book, chapter as u16) {
            Ok(()) => ptr::null_mut(),
            Err(e) => out_string(e.to_string()),
        }
    })
}
