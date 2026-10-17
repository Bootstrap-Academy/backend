//! Moments and periods of the legal texts, counted in calendar days in Germany.

use chrono::{DateTime, Datelike, Days, NaiveDate, TimeZone, Timelike, Utc, Weekday};
use chrono_tz::Europe::Berlin;

/// Whether `at` is 00:00 in Europe/Berlin. Terms take effect only at such a moment, so the
/// date the texts name and the moment the platform switches are the same day.
pub fn starts_a_berlin_day(at: DateTime<Utc>) -> bool {
    let local = at.with_timezone(&Berlin);
    local.hour() == 0 && local.minute() == 0 && local.second() == 0 && local.nanosecond() == 0
}

/// The end of the withdrawal period of a contract concluded at `concluded_at`: the end of the
/// calendar day in Berlin 14 days after the conclusion (§ 187 Abs. 1, § 188 Abs. 2 BGB). If
/// that day is a Saturday, a Sunday or a public holiday in all of Germany, the period ends with
/// the next working day (§ 193 BGB). The result is the first moment after the period.
///
/// Where the date cannot be computed, the end lies as far in the future as possible, so that a
/// declaration that depends on it is asked for rather than left out.
pub fn withdrawal_period_end(concluded_at: DateTime<Utc>) -> DateTime<Utc> {
    let end = || {
        let mut last = concluded_at
            .with_timezone(&Berlin)
            .date_naive()
            .checked_add_days(Days::new(14))?;
        while !is_working_day(last) {
            last = last.succ_opt()?;
        }
        let next = last.succ_opt()?.and_hms_opt(0, 0, 0)?;
        Some(
            Berlin
                .from_local_datetime(&next)
                .earliest()?
                .with_timezone(&Utc),
        )
    };
    end().unwrap_or(DateTime::<Utc>::MAX_UTC)
}

fn is_working_day(day: NaiveDate) -> bool {
    !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) && !is_nationwide_holiday(day)
}

/// The public holidays of every German state: New Year, Good Friday, Easter Monday, 1 May,
/// Ascension Day, Whit Monday, the Day of German Unity and both days of Christmas.
fn is_nationwide_holiday(day: NaiveDate) -> bool {
    if matches!(
        (day.month(), day.day()),
        (1, 1) | (5, 1) | (10, 3) | (12, 25) | (12, 26)
    ) {
        return true;
    }
    easter_sunday(day.year()).is_some_and(|easter| {
        [-2, 1, 39, 50].into_iter().any(|offset: i64| {
            easter.checked_add_signed(chrono::TimeDelta::days(offset)) == Some(day)
        })
    })
}

/// Easter Sunday of the Gregorian calendar (the anonymous algorithm of Meeus, Jones and
/// Butcher).
fn easter_sunday(year: i32) -> Option<NaiveDate> {
    let a = year.rem_euclid(19);
    let b = year.div_euclid(100);
    let c = year.rem_euclid(100);
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn berlin(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Berlin
            .with_ymd_and_hms(y, mo, d, h, mi, 0)
            .single()
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn a_berlin_day_starts_at_midnight_in_berlin_in_winter_and_summer() {
        assert!(starts_a_berlin_day(berlin(2026, 12, 9, 0, 0)));
        assert!(starts_a_berlin_day("2026-12-08T23:00:00Z".parse().unwrap()));
        assert!(starts_a_berlin_day("2027-06-30T22:00:00Z".parse().unwrap()));
        assert!(!starts_a_berlin_day(
            "2026-12-09T00:00:00Z".parse().unwrap()
        ));
        assert!(!starts_a_berlin_day(
            berlin(2026, 12, 9, 0, 0) + chrono::TimeDelta::nanoseconds(1)
        ));
    }

    #[test]
    fn easter_falls_on_its_known_dates() {
        for (year, month, day) in [
            (2024, 3, 31),
            (2025, 4, 20),
            (2026, 4, 5),
            (2027, 3, 28),
            (2038, 4, 25),
            (2285, 3, 22),
        ] {
            assert_eq!(
                easter_sunday(year),
                NaiveDate::from_ymd_opt(year, month, day),
                "{year}"
            );
        }
    }

    #[test]
    fn the_period_ends_with_the_berlin_day_fourteen_days_after_the_conclusion() {
        // Tuesday to Tuesday, at any hour of the Berlin day.
        for (h, mi) in [(0, 0), (10, 0), (23, 59)] {
            assert_eq!(
                withdrawal_period_end(berlin(2026, 10, 20, h, mi)),
                berlin(2026, 11, 4, 0, 0)
            );
        }
        // 23:30 UTC on 20 October is already 21 October in Berlin.
        assert_eq!(
            withdrawal_period_end("2026-10-20T23:30:00Z".parse().unwrap()),
            berlin(2026, 11, 5, 0, 0)
        );
    }

    #[test]
    fn a_last_day_on_a_weekend_or_a_holiday_moves_to_the_next_working_day() {
        // Saturday: the period runs to the end of Monday.
        assert_eq!(
            withdrawal_period_end(berlin(2026, 10, 10, 12, 0)),
            berlin(2026, 10, 27, 0, 0)
        );
        // Day of German Unity on a Saturday (2026), then Sunday: Monday 5 October.
        assert_eq!(
            withdrawal_period_end(berlin(2026, 9, 19, 12, 0)),
            berlin(2026, 10, 6, 0, 0)
        );
        // Christmas: Friday 25, Saturday 26, Sunday 27 December, so Monday 28.
        assert_eq!(
            withdrawal_period_end(berlin(2026, 12, 11, 12, 0)),
            berlin(2026, 12, 29, 0, 0)
        );
        // Easter 2027: Good Friday 26 March to Easter Monday 29 March, so Tuesday 30, across
        // the change to summer time.
        assert_eq!(
            withdrawal_period_end(berlin(2027, 3, 12, 12, 0)),
            berlin(2027, 3, 31, 0, 0)
        );
        // Ascension Day 2027 (Thursday 6 May), so Friday 7; Whit Monday (17 May), so Tuesday.
        assert_eq!(
            withdrawal_period_end(berlin(2027, 4, 22, 12, 0)),
            berlin(2027, 5, 8, 0, 0)
        );
        assert_eq!(
            withdrawal_period_end(berlin(2027, 5, 3, 12, 0)),
            berlin(2027, 5, 19, 0, 0)
        );
        // New Year 2027 on a Friday and 1 May 2026 on a Friday: both run to the next Monday.
        assert_eq!(
            withdrawal_period_end(berlin(2026, 12, 18, 12, 0)),
            berlin(2027, 1, 5, 0, 0)
        );
        assert_eq!(
            withdrawal_period_end(berlin(2026, 4, 17, 12, 0)),
            berlin(2026, 5, 5, 0, 0)
        );
        // A working day after the 14th day changes nothing: Wednesday 25 November 2026.
        assert_eq!(
            withdrawal_period_end(berlin(2026, 11, 11, 8, 0)),
            berlin(2026, 11, 26, 0, 0)
        );
    }
}
