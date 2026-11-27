//! Today's memorization work — the C ABI behind the plans surface's memory
//! tile and the bookmark-row chip. One read: the due cards a daily cap still
//! has room for, beside the cap and what is already spent, so a shell can say
//! "3 of 10 today" and hand a review exactly that queue.
//!
//! Its own module rather than beside the other memory endpoints in `lib.rs`,
//! which is past the tree's file-size rule already.

use std::ffi::c_char;
use std::ptr;

use plumbline_core::memory;
use plumbline_core::VRef;
use serde::Serialize;

use crate::{guard, opt_str, out_json, wire, PlumblineEngine};

/// What working one row of the hub means now: see [`memory::passage::Run`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireMemoryRun {
    /// The row's key — its first verse.
    #[serde(rename = "ref")]
    reference: String,
    /// `"Ps 23:1–6"`, or `"Gen 1:7"`.
    label: String,
    /// Verses in the row.
    total: u32,
    /// Of those, the verses passed at least once.
    learned: u32,
    /// Learned verses due for review, in verse order: drilled first.
    due: Vec<String>,
    /// Verses not learned yet, in verse order. The first is offered after the
    /// reviews, and each next one only once the one before it is passed.
    fresh: Vec<String>,
    /// Every verse of the row, in order — what a drill of a row with nothing
    /// due and nothing new walks.
    all: Vec<String>,
}

/// The run for the row at `verse_ref` at `now` (RFC3339 UTC), as
/// `{ref, label, total, learned, due, fresh, all}`. A verse this Bible does not
/// have is left out of all three lists, so a shell never asks for a drill that
/// cannot be served. Null when nothing is being memorized at `verse_ref`.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_memory_run_json(
    engine: *const PlumblineEngine,
    verse_ref: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref), Some(now)) =
            (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key), opt_str(now))
        else {
            return ptr::null_mut();
        };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        let Some(run) = memory::passage::Deck::new(&cards).run(&vref, now) else { return ptr::null_mut() };
        let keys = |refs: &[VRef]| -> Vec<String> {
            refs.iter().filter(|r| e.corpus.verse(r).is_some()).map(VRef::ref_key).collect()
        };
        out_json(&WireMemoryRun {
            reference: run.head.ref_key(),
            total: run.verses.len() as u32,
            learned: run.learned,
            due: keys(&run.due),
            fresh: keys(&run.fresh),
            all: keys(&run.verses),
            label: run.label,
        })
    })
}

/// Days until a verse comes back for each grade: what a shell prints under its
/// four buttons.
#[derive(Serialize)]
struct WireMemoryNext {
    again: u32,
    hard: u32,
    good: u32,
    easy: u32,
}

/// What each grade would do to `verse_ref` if given at `now` (RFC3339 UTC), as
/// `{again, hard, good, easy}` in days until the verse comes back —
/// [`memory::next_intervals`] over the verse's card as a grade would find it:
/// its own, the schedule of a chunk it has yet to be split from, or a new one.
/// Nothing is written. Null on a bad argument.
///
/// # Safety
/// `engine` is a live engine; the string args are null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_memory_next_json(
    engine: *const PlumblineEngine,
    verse_ref: *const c_char,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(vref), Some(now)) =
            (engine.as_ref(), opt_str(verse_ref).and_then(VRef::parse_ref_key), opt_str(now))
        else {
            return ptr::null_mut();
        };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        let card = memory::passage::spread(&cards)
            .remove(&vref)
            .unwrap_or_else(|| memory::Card::new(vref, plumbline_core::canon::TOKENIZATION_VERSION, now));
        let [again, hard, good, easy] = memory::next_intervals(&card, now);
        out_json(&WireMemoryNext { again, hard, good, easy })
    })
}

/// The passage `vref` is being memorized in and the verse to show above its
/// drill: `(label, lead-in)`. The lead-in is the nearest earlier verse of that
/// passage this Bible has, so none for the passage's first verse. Both `None`
/// for a verse no passage covers.
pub(crate) fn passage_context(e: &PlumblineEngine, vref: &VRef) -> (Option<String>, Option<wire::WireMemoryLeadIn>) {
    let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
    let deck = memory::passage::Deck::new(&cards);
    // The passage that opens earliest: where two overlap, the longer run-up.
    let Some(slot) = deck.slots().iter().find(|s| s.verses.len() > 1 && s.verses.contains(vref)) else {
        return (None, None);
    };
    let at = slot.verses.iter().position(|v| v == vref).unwrap_or(0);
    let lead_in = slot.verses[..at].iter().rev().find_map(|v| {
        e.corpus.verse(v).map(|verse| wire::WireMemoryLeadIn { reference: v.ref_key(), text: verse.body() })
    });
    (Some(slot.label.clone()), lead_in)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireMemoryToday {
    /// The cards to review now, most overdue first, cut to the cap's room.
    refs: Vec<String>,
    /// Every due card, capped or not.
    due_total: u32,
    /// Cards reviewed on `now`'s calendar day.
    reviewed_today: u32,
    /// The cap as given; `0` is no limit.
    cap: u32,
}

#[derive(Serialize)]
struct WireMemoryPace {
    windows: Vec<memory::MemoryWindow>,
}

/// Memorization activity over the reading pace's windows, as
/// `{windows:[{id,days,reviews,added}]}` — [`memory::pace`] over the home's
/// cards at `now` (RFC3339 UTC). No cards, or no home, answers no windows.
/// Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; `now` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_memory_pace_json(
    engine: *const PlumblineEngine,
    now: *const c_char,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(now)) = (engine.as_ref(), opt_str(now)) else {
            return ptr::null_mut();
        };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        out_json(&WireMemoryPace { windows: memory::pace(&cards, now).unwrap_or_default() })
    })
}

/// Today's memorization queue under a daily `cap` (`0` = no limit), as
/// `{refs, dueTotal, reviewedToday, cap}` — [`memory::today_queue`] over the
/// home's cards at `now` (RFC3339 UTC; the shell passes its day stamp). An
/// engine with no home answers an empty queue. Never null on a live engine.
///
/// # Safety
/// `engine` is a live engine; `now` is null or valid NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn plumbline_engine_memory_today_json(
    engine: *const PlumblineEngine,
    now: *const c_char,
    cap: u32,
) -> *mut c_char {
    guard(ptr::null_mut(), || {
        let (Some(e), Some(now)) = (engine.as_ref(), opt_str(now)) else {
            return ptr::null_mut();
        };
        let cards = e.home.as_ref().map(|h| memory::load_cards(h).0).unwrap_or_default();
        let q = memory::today_queue(&cards, now, cap);
        out_json(&WireMemoryToday {
            refs: q.refs.iter().map(VRef::ref_key).collect(),
            due_total: q.due_total as u32,
            reviewed_today: q.reviewed_today as u32,
            cap: q.cap,
        })
    })
}
