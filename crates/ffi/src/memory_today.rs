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

use crate::{guard, opt_str, out_json, PlumblineEngine};

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
