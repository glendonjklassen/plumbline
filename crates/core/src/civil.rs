//! Civil-date math at day granularity — no external time dependency.
//!
//! The core is pure and has no clock: every timestamp arrives from the shell as
//! an RFC3339 string. What the core *does* need is arithmetic over those
//! strings — how many days until an SRS card is due, how many days since a
//! chapter was last read. That is all this module.
//!
//! Lifted out of `memory.rs` (2026-07-28) when `reading.rs` needed the same
//! four functions; it was private there and duplicating it would have meant two
//! implementations of the leap-year rule.

/// Howard Hinnant's days-from-civil (proleptic Gregorian; 1970-01-01 == 0).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The inverse of [`days_from_civil`] → `(year, month, day)`.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse the `YYYY-MM-DD` date out of an RFC3339 stamp → days since epoch.
/// Tolerates a bare date (`"2026-07-28"`) as well as a full stamp.
pub fn date_to_days(stamp: &str) -> Option<i64> {
    let mut it = stamp.get(0..10)?.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    Some(days_from_civil(y, m, d))
}

/// Days since the epoch rendered back as `YYYY-MM-DD`.
pub fn days_to_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `stamp`'s date advanced by `n` days, as `YYYY-MM-DD`.
pub fn add_days(stamp: &str, n: i64) -> String {
    days_to_date(date_to_days(stamp).unwrap_or(0) + n)
}

/// Unix seconds as the frozen wire stamp, `YYYY-MM-DDThh:mm:ssZ` — the same
/// shape the shells send for `created` / `added`.
///
/// The core still has no clock: this only *formats* a number someone else read.
/// `crates/ffi` is where that number comes from, for the mutations whose shell
/// caller sends no stamp of its own (an `updated` bump on a note edit, say —
/// docs/STABLE-IDS.md). Keeping the arithmetic here keeps it testable at fixed
/// instants, which a clock never is.
///
/// UTC by construction, no leap seconds, no local zone: seconds since the epoch
/// divided into days and a remainder.
pub fn stamp_from_epoch_secs(secs: i64) -> String {
    // Floor division, so a pre-epoch instant lands on the right day rather than
    // rounding toward zero into the next one.
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    format!("{}T{:02}:{:02}:{:02}Z", days_to_date(days), rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// Whole days from `from` to `to` (negative when `to` precedes `from`).
/// `None` if either stamp is unparseable.
pub fn days_between(from: &str, to: &str) -> Option<i64> {
    Some(date_to_days(to)? - date_to_days(from)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_boundaries() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_to_date(0), "1970-01-01");
        // Leap day and the month rollovers either side of it.
        assert_eq!(add_days("2024-02-28T00:00:00Z", 1), "2024-02-29");
        assert_eq!(add_days("2024-02-29T00:00:00Z", 1), "2024-03-01");
        assert_eq!(add_days("2025-02-28T00:00:00Z", 1), "2025-03-01");
        assert_eq!(add_days("2026-12-31T23:59:59Z", 1), "2027-01-01");
        for z in [-100_000i64, -1, 0, 1, 20_000, 100_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z, "round trip at {z}");
        }
    }

    #[test]
    fn spans_days_in_both_directions() {
        assert_eq!(days_between("2026-01-01", "2026-01-01"), Some(0));
        assert_eq!(days_between("2026-01-01T09:00:00Z", "2026-12-31T01:00:00Z"), Some(364));
        assert_eq!(days_between("2026-12-31", "2026-01-01"), Some(-364));
        // A leap year is 366 days end to end.
        assert_eq!(days_between("2024-01-01", "2025-01-01"), Some(366));
        assert_eq!(days_between("nonsense", "2026-01-01"), None);
    }

    #[test]
    fn parses_bare_dates_and_full_stamps_alike() {
        assert_eq!(date_to_days("2026-07-28"), date_to_days("2026-07-28T13:45:01Z"));
    }

    /// The stamp `crates/ffi`'s clock formats, pinned at fixed instants — which
    /// is the point of keeping the arithmetic here and the clock there.
    #[test]
    fn formats_epoch_seconds_as_the_wire_stamp() {
        assert_eq!(stamp_from_epoch_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(stamp_from_epoch_secs(1), "1970-01-01T00:00:01Z");
        assert_eq!(stamp_from_epoch_secs(86_399), "1970-01-01T23:59:59Z");
        assert_eq!(stamp_from_epoch_secs(86_400), "1970-01-02T00:00:00Z");
        // 2026-07-30T12:34:56Z, the shape a save actually writes.
        let secs = days_from_civil(2026, 7, 30) * 86_400 + 12 * 3600 + 34 * 60 + 56;
        assert_eq!(stamp_from_epoch_secs(secs), "2026-07-30T12:34:56Z");
        // A clock behind the epoch still lands on the right day, rather than
        // rounding toward zero into the next one.
        assert_eq!(stamp_from_epoch_secs(-1), "1969-12-31T23:59:59Z");
    }

    /// Whatever the clock says, the stamp is one the rest of the core can read
    /// back — these strings are compared against `created` and each other.
    #[test]
    fn the_stamp_it_writes_is_one_date_to_days_can_parse() {
        for secs in [0i64, 1_000_000, 1_785_000_000, -86_400] {
            let stamp = stamp_from_epoch_secs(secs);
            assert_eq!(date_to_days(&stamp), Some(secs.div_euclid(86_400)), "{stamp}");
        }
    }
}
