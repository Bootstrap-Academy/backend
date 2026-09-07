//! Retention periods that are counted in whole calendar years.
//!
//! Both periods the platform enforces work the same way: the clock starts at
//! the end of the calendar year of the record, not at the record itself. That
//! is how § 147 Abs. 4 AO counts the retention period of an invoice and how
//! § 199 Abs. 1 BGB counts the regular limitation period of a claim.

use chrono::{DateTime, Datelike, TimeZone, Utc};
use chrono_tz::Europe::Berlin;

/// Time zone the calendar year of a record is determined in.
///
/// The periods are German ones and the records are kept by a German company,
/// so the calendar year of a record is the year it carries in `Europe/Berlin`.
/// For a document that is also the date printed on it.
pub const RETENTION_TIME_ZONE: chrono_tz::Tz = Berlin;

/// Return the timestamp before which records kept for `years` years may be
/// deleted at `now`.
///
/// The period begins at the end of the calendar year of the record, so a
/// record from 2024 that is kept for eight years may be deleted from the
/// beginning of 2033.
///
/// Both the current year and the beginning of the year are taken in
/// [`RETENTION_TIME_ZONE`]. In UTC, a record from between 23:00 and midnight
/// on 31 December would be filed under the previous year and pruned a year
/// before the German period ends.
///
/// Returns `None` if the resulting year is outside the representable range.
pub fn retention_cutoff(now: DateTime<Utc>, years: u32) -> Option<DateTime<Utc>> {
    let year = now
        .with_timezone(&RETENTION_TIME_ZONE)
        .year()
        .checked_sub(i32::try_from(years).ok()?)?;

    RETENTION_TIME_ZONE
        .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .map(|start_of_year| start_of_year.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn cutoff_starts_at_the_end_of_the_year_of_the_record() {
        // A document issued anywhere in 2024 has to be kept until the end of
        // 2032 and may be deleted from the beginning of 2033.
        let cutoff = retention_cutoff(date(2032, 12, 31), 8).unwrap();
        assert!(date(2024, 1, 1) >= cutoff);
        assert!(date(2024, 12, 31) >= cutoff);

        let cutoff = retention_cutoff(date(2033, 1, 1), 8).unwrap();
        assert!(date(2024, 1, 1) < cutoff);
        assert!(date(2024, 12, 31) < cutoff);
        assert!(date(2025, 1, 1) >= cutoff);
    }

    /// A declaration received anywhere in 2024 is kept for three years and may
    /// be deleted from the beginning of 2028.
    #[test]
    fn cutoff_of_the_limitation_period() {
        let cutoff = retention_cutoff(date(2027, 12, 31), 3).unwrap();
        assert!(date(2024, 12, 31) >= cutoff);

        let cutoff = retention_cutoff(date(2028, 1, 1), 3).unwrap();
        assert!(date(2024, 12, 31) < cutoff);
        assert!(date(2025, 1, 1) >= cutoff);
    }

    /// A record from between 23:00 UTC and midnight on 31 December already
    /// carries the next year in Berlin, so it belongs to the next calendar
    /// year and has to be kept a year longer.
    #[test]
    fn cutoff_uses_the_calendar_year_in_berlin() {
        let new_years_eve = Utc.with_ymd_and_hms(2024, 12, 31, 23, 30, 0).unwrap();
        let earlier_that_day = Utc.with_ymd_and_hms(2024, 12, 31, 22, 30, 0).unwrap();

        // 2033 in Berlin, still 2032 in UTC.
        let cutoff =
            retention_cutoff(Utc.with_ymd_and_hms(2032, 12, 31, 23, 30, 0).unwrap(), 8).unwrap();
        // The document is dated 1 January 2025 in Berlin and is kept.
        assert!(new_years_eve >= cutoff);
        // An hour earlier it is still 2024 and may be deleted.
        assert!(earlier_that_day < cutoff);

        // A year later the 2025 document may be deleted as well.
        let cutoff =
            retention_cutoff(Utc.with_ymd_and_hms(2033, 12, 31, 23, 30, 0).unwrap(), 8).unwrap();
        assert!(new_years_eve < cutoff);
    }

    /// The cutoff is the beginning of the year in Berlin, not in UTC.
    #[test]
    fn cutoff_is_the_start_of_the_berlin_year() {
        assert_eq!(
            retention_cutoff(date(2033, 6, 1), 8).unwrap(),
            Utc.with_ymd_and_hms(2024, 12, 31, 23, 0, 0).unwrap()
        );
    }

    #[test]
    fn cutoff_out_of_range() {
        assert_eq!(retention_cutoff(date(2026, 9, 3), u32::MAX), None);
    }
}
