//! WHICH SEATING a reader is in — so opening the app at church resumes church,
//! and opening it on a Tuesday morning resumes Tuesday morning.
//!
//! A reader's last chapter is not one thing. Somebody who studies on weekday
//! mornings, sits in a Sunday service and goes to a Wednesday meeting has three
//! separate places they were, and one "last chapter" serves whichever they did
//! most recently — so arriving at church on Sunday reopens Saturday night's
//! study, and Monday morning reopens the sermon passage. Keeping a position per
//! SLOT means each of those threads is picked up where it was left
//! (maintainer, 2026-08-13).
//!
//! The boundaries are a judgement, stated here rather than buried in a shell:
//!
//! | slot               | when                        |
//! |--------------------|-----------------------------|
//! | `sunday-morning`   | Sunday before noon          |
//! | `sunday-evening`   | Sunday from noon            |
//! | `wednesday-evening`| Wednesday from 5pm          |
//! | `other`            | everything else             |
//!
//! Wednesday MORNING is deliberately `other`: the slot exists for the midweek
//! meeting, and a Wednesday morning is a weekday morning like any other.
//!
//! The shells pass their own LOCAL date and hour. The core has no clock and no
//! timezone, and a slot computed in UTC would put a Sunday-evening service in
//! Monday for half the world.

use serde::{Deserialize, Serialize};

use crate::civil::date_to_days;

/// A seating a reading position is remembered against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionSlot {
    SundayMorning,
    SundayEvening,
    WednesdayEvening,
    Other,
}

impl SessionSlot {
    /// The token stored in the config. Frozen once written into a reader's file.
    pub fn token(self) -> &'static str {
        match self {
            SessionSlot::SundayMorning => "sunday-morning",
            SessionSlot::SundayEvening => "sunday-evening",
            SessionSlot::WednesdayEvening => "wednesday-evening",
            SessionSlot::Other => "other",
        }
    }

    /// A token back to a slot; unknown tokens (a later build's) answer `None`
    /// rather than silently becoming `Other`, so a caller can tell "I do not
    /// know this slot" from "this is the everyday one".
    pub fn parse(t: &str) -> Option<SessionSlot> {
        match t {
            "sunday-morning" => Some(SessionSlot::SundayMorning),
            "sunday-evening" => Some(SessionSlot::SundayEvening),
            "wednesday-evening" => Some(SessionSlot::WednesdayEvening),
            "other" => Some(SessionSlot::Other),
            _ => None,
        }
    }

    /// Every slot, for a shell enumerating them.
    pub const ALL: [SessionSlot; 4] =
        [SessionSlot::SundayMorning, SessionSlot::SundayEvening, SessionSlot::WednesdayEvening, SessionSlot::Other];
}

/// Day of the week for a `YYYY-MM-DD` date: 0 = Sunday … 6 = Saturday.
///
/// 1970-01-01 was a THURSDAY, which is day 4, hence the offset. `date_to_days`
/// answers days since that epoch and can be negative for dates before it, so the
/// remainder is brought back into range rather than left as C's negative modulo.
pub fn weekday(date: &str) -> Option<u8> {
    let days = date_to_days(date)?;
    Some((((days + 4) % 7 + 7) % 7) as u8)
}

/// The slot a local date and hour fall in. `hour` is 0–23 local time.
pub fn slot_for(date: &str, hour: u32) -> SessionSlot {
    match weekday(date) {
        Some(0) if hour < 12 => SessionSlot::SundayMorning,
        Some(0) => SessionSlot::SundayEvening,
        Some(3) if hour >= 17 => SessionSlot::WednesdayEvening,
        // An unparseable date is the everyday slot, not a panic: a shell with a
        // broken clock should still open a Bible.
        _ => SessionSlot::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays_are_right_against_known_dates() {
        // The epoch itself, and a spread of dates whose weekday is checkable.
        assert_eq!(weekday("1970-01-01"), Some(4), "the epoch was a Thursday");
        assert_eq!(weekday("2026-08-13"), Some(4), "a Thursday");
        assert_eq!(weekday("2026-08-16"), Some(0), "a Sunday");
        assert_eq!(weekday("2026-08-19"), Some(3), "a Wednesday");
        assert_eq!(weekday("2000-01-01"), Some(6), "a Saturday");
        // Before the epoch, where a naive `%` would answer negative.
        assert_eq!(weekday("1969-12-31"), Some(3), "a Wednesday");
        assert_eq!(weekday("1900-01-01"), Some(1), "a Monday");
    }

    #[test]
    fn sunday_splits_at_noon() {
        assert_eq!(slot_for("2026-08-16", 0), SessionSlot::SundayMorning);
        assert_eq!(slot_for("2026-08-16", 11), SessionSlot::SundayMorning);
        assert_eq!(slot_for("2026-08-16", 12), SessionSlot::SundayEvening, "noon is the evening side");
        assert_eq!(slot_for("2026-08-16", 23), SessionSlot::SundayEvening);
    }

    #[test]
    fn wednesday_is_only_an_evening() {
        // The slot exists for the midweek meeting; a Wednesday morning is a
        // weekday morning like any other and belongs with them.
        assert_eq!(slot_for("2026-08-19", 9), SessionSlot::Other);
        assert_eq!(slot_for("2026-08-19", 16), SessionSlot::Other);
        assert_eq!(slot_for("2026-08-19", 17), SessionSlot::WednesdayEvening);
        assert_eq!(slot_for("2026-08-19", 22), SessionSlot::WednesdayEvening);
    }

    #[test]
    fn every_other_day_is_the_everyday_slot() {
        for date in ["2026-08-17", "2026-08-18", "2026-08-20", "2026-08-21", "2026-08-22"] {
            for hour in [0, 9, 12, 19, 23] {
                assert_eq!(slot_for(date, hour), SessionSlot::Other, "{date} {hour}h");
            }
        }
    }

    #[test]
    fn a_broken_clock_still_opens_a_bible() {
        assert_eq!(slot_for("not-a-date", 9), SessionSlot::Other);
        assert_eq!(slot_for("", 9), SessionSlot::Other);
    }

    #[test]
    fn tokens_round_trip_and_an_unknown_one_is_none() {
        for s in SessionSlot::ALL {
            assert_eq!(SessionSlot::parse(s.token()), Some(s));
        }
        // NOT `Other`: a caller must be able to tell an unknown slot from the
        // everyday one, or a later build's token would silently overwrite it.
        assert_eq!(SessionSlot::parse("friday-vigil"), None);
    }
}
