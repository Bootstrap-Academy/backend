//! Calendar arithmetic for the premium periods.
//!
//! The terms sell „1 Monat" and „12 Monate", so a period is a calendar period
//! and not a fixed number of days: it ends on the same day of the month as it
//! began, and on the last day of the month if that day does not exist there
//! (§ 188 Abs. 2 and Abs. 3 BGB). Twelve of those months are a year, so a
//! yearly period ends on the same date one year later.
//!
//! The calendar the consumer reads is the German one, so the day of the month
//! is taken in `Europe/Berlin` rather than in UTC. Without that, a membership
//! bought on the 31st of a month at a time that is still the 30th in UTC would
//! be extended from the wrong day.

use chrono::{DateTime, Months, TimeDelta, Utc};
use chrono_tz::Europe::Berlin;

/// Return the point in time `months` calendar months after `from`.
pub fn add_months(from: DateTime<Utc>, months: u32) -> DateTime<Utc> {
    let local = from.with_timezone(&Berlin).naive_local();

    let Some(shifted) = local.checked_add_months(Months::new(months)) else {
        // Only reachable beyond the year 262143; keeping the start is the
        // safest thing left to do.
        return from;
    };

    resolve(shifted).unwrap_or(from + TimeDelta::days(30 * i64::from(months)))
}

/// Turn a local time back into a point in time.
///
/// The hour a period would end in can be missing (the night the clocks go
/// forward) or exist twice (the night they go back). Both are resolved in
/// favour of the consumer: the end of the missing hour, and the later of the
/// two possible ones.
fn resolve(local: chrono::NaiveDateTime) -> Option<DateTime<Utc>> {
    use chrono::{LocalResult, TimeZone};

    match Berlin.from_local_datetime(&local) {
        LocalResult::Single(value) => Some(value.with_timezone(&Utc)),
        LocalResult::Ambiguous(_, latest) => Some(latest.with_timezone(&Utc)),
        LocalResult::None => match Berlin.from_local_datetime(&(local + TimeDelta::hours(1))) {
            LocalResult::Single(value) => Some(value.with_timezone(&Utc)),
            LocalResult::Ambiguous(_, latest) => Some(latest.with_timezone(&Utc)),
            LocalResult::None => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn berlin(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
    ) -> chrono::DateTime<chrono_tz::Tz> {
        Berlin
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .unwrap()
    }

    fn one_month(from: chrono::DateTime<chrono_tz::Tz>) -> chrono::DateTime<chrono_tz::Tz> {
        add_months(from.with_timezone(&Utc), 1).with_timezone(&Berlin)
    }

    fn one_year(from: chrono::DateTime<chrono_tz::Tz>) -> chrono::DateTime<chrono_tz::Tz> {
        add_months(from.with_timezone(&Utc), 12).with_timezone(&Berlin)
    }

    /// The ordinary case: the same day of the next month, at the same time.
    #[test]
    fn month_keeps_the_day() {
        assert_eq!(
            one_month(berlin(2026, 9, 7, 14, 30)),
            berlin(2026, 10, 7, 14, 30)
        );
        assert_eq!(
            one_month(berlin(2026, 1, 1, 0, 0)),
            berlin(2026, 2, 1, 0, 0)
        );
    }

    /// § 188 Abs. 3 BGB: a day that the next month does not have becomes its
    /// last day.
    #[test]
    fn month_falls_back_to_the_last_day() {
        assert_eq!(
            one_month(berlin(2026, 1, 31, 9, 0)),
            berlin(2026, 2, 28, 9, 0)
        );
        assert_eq!(
            one_month(berlin(2028, 1, 31, 9, 0)),
            berlin(2028, 2, 29, 9, 0)
        );
        assert_eq!(
            one_month(berlin(2026, 1, 30, 9, 0)),
            berlin(2026, 2, 28, 9, 0)
        );
        assert_eq!(
            one_month(berlin(2026, 3, 31, 9, 0)),
            berlin(2026, 4, 30, 9, 0)
        );
        assert_eq!(
            one_month(berlin(2026, 5, 31, 9, 0)),
            berlin(2026, 6, 30, 9, 0)
        );
    }

    /// The end of February is not carried over to the end of March.
    #[test]
    fn month_from_the_end_of_february() {
        assert_eq!(
            one_month(berlin(2026, 2, 28, 9, 0)),
            berlin(2026, 3, 28, 9, 0)
        );
        assert_eq!(
            one_month(berlin(2028, 2, 29, 9, 0)),
            berlin(2028, 3, 29, 9, 0)
        );
    }

    /// Twelve months are one year, on the same date.
    #[test]
    fn a_year_is_the_same_date_one_year_later() {
        assert_eq!(
            one_year(berlin(2026, 9, 7, 14, 30)),
            berlin(2027, 9, 7, 14, 30)
        );
        assert_eq!(one_year(berlin(2024, 1, 1, 0, 0)), berlin(2025, 1, 1, 0, 0));
    }

    /// A year starting on 29 February ends on 28 February, because 2027 has no
    /// 29 February.
    #[test]
    fn a_year_from_a_leap_day() {
        assert_eq!(
            one_year(berlin(2028, 2, 29, 12, 0)),
            berlin(2029, 2, 28, 12, 0)
        );
        // and a leap year reached from a leap year keeps the day
        assert_eq!(
            add_months(berlin(2028, 2, 29, 12, 0).with_timezone(&Utc), 48).with_timezone(&Berlin),
            berlin(2032, 2, 29, 12, 0)
        );
    }

    /// A month is a calendar month, not 30.44 days: the length of the period
    /// depends on the month it runs through.
    #[test]
    fn the_length_depends_on_the_month() {
        let january = berlin(2026, 1, 15, 12, 0);
        assert_eq!(
            (one_month(january) - january).num_days(),
            31,
            "January has 31 days"
        );

        let february = berlin(2026, 2, 15, 12, 0);
        assert_eq!(
            (one_month(february) - february).num_days(),
            28,
            "February 2026 has 28 days"
        );
    }

    /// A period ending in the hour that the clocks skip is moved to the end of
    /// the gap rather than being refused.
    #[test]
    fn an_end_in_a_missing_hour() {
        // 29 March 2026, 02:30 local time does not exist in Europe/Berlin
        let start = berlin(2025, 3, 29, 2, 30);
        let end = one_year(start);
        assert_eq!(end, berlin(2026, 3, 29, 3, 30));
        assert!(end > start);
    }

    /// The clocks going back give the hour twice; the later one is used, so the
    /// period is never shortened.
    #[test]
    fn an_end_in_a_repeated_hour() {
        // 25 October 2026, 02:30 local time exists twice in Europe/Berlin
        let start = berlin(2026, 9, 25, 2, 30);
        let end = one_month(start).with_timezone(&Utc);
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 10, 25, 1, 30, 0).unwrap());
    }
}
