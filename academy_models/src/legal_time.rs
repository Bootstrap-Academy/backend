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
/// that day is a Saturday, a Sunday or a statutory holiday in any German state, the period ends
/// with the next working day (§ 193 BGB). The result is the first moment after the period.
///
/// § 193 BGB counts the holidays at the customer's place, which the platform does not know, so
/// a holiday of any state moves the end. The result is never earlier than the legal end
/// anywhere in Germany, and a declaration that depends on it is asked for whenever it could be
/// needed. Where the date cannot be computed, the end lies as far in the future as possible for
/// the same reason.
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
    !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) && !is_holiday_in_any_state(day)
}

/// Whether `day` is a statutory holiday in at least one German state, in the whole state or in
/// a part of it.
///
/// - Everywhere: New Year, Good Friday, Easter Monday, 1 May, Ascension Day, Whit Monday, the
///   Day of German Unity and both days of Christmas.
/// - Epiphany, 6 January (Baden-Württemberg, Bavaria, Saxony-Anhalt).
/// - International Women's Day, 8 March (Berlin, Mecklenburg-Western Pomerania).
/// - Corpus Christi, 60 days after Easter (Baden-Württemberg, Bavaria, Hesse, North
///   Rhine-Westphalia, Rhineland-Palatinate, Saarland, parts of Saxony and Thuringia).
/// - Peace Festival of Augsburg, 8 August (the city of Augsburg, Bavaria).
/// - Assumption Day, 15 August (Saarland, parts of Bavaria).
/// - World Children's Day, 20 September (Thuringia).
/// - Reformation Day, 31 October (Brandenburg, Bremen, Hamburg, Mecklenburg-Western
///   Pomerania, Lower Saxony, Saxony, Saxony-Anhalt, Schleswig-Holstein, Thuringia).
/// - All Saints' Day, 1 November (Baden-Württemberg, Bavaria, North Rhine-Westphalia,
///   Rhineland-Palatinate, Saarland).
/// - Day of Repentance and Prayer, the Wednesday before 23 November (Saxony).
///
/// Easter Sunday and Whit Sunday (Brandenburg) are Sundays and move the end anyway.
fn is_holiday_in_any_state(day: NaiveDate) -> bool {
    if matches!(
        (day.month(), day.day()),
        (1, 1)
            | (1, 6)
            | (3, 8)
            | (5, 1)
            | (8, 8)
            | (8, 15)
            | (9, 20)
            | (10, 3)
            | (10, 31)
            | (11, 1)
            | (12, 25)
            | (12, 26)
    ) {
        return true;
    }
    if day.month() == 11 && (16..=22).contains(&day.day()) && day.weekday() == Weekday::Wed {
        return true;
    }
    easter_sunday(day.year()).is_some_and(|easter| {
        [-2, 1, 39, 50, 60].into_iter().any(|offset: i64| {
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

    #[test]
    fn a_holiday_of_any_single_state_moves_the_end_too() {
        // The 14th day after the order is the holiday, a working day otherwise; the period then
        // runs to the end of the next working day. Last: Reformation Day on Tuesday 31 October
        // 2028 and All Saints' Day on the Wednesday.
        for (concluded, end, holiday) in [
            ("2026-12-23", "2027-01-08", "Epiphany, Wed 6 Jan 2027"),
            ("2027-02-22", "2027-03-10", "Women's Day, Mon 8 Mar 2027"),
            ("2027-05-13", "2027-05-29", "Corpus Christi, Thu 27 May"),
            ("2028-07-25", "2028-08-10", "Augsburg, Tue 8 Aug 2028"),
            ("2028-08-01", "2028-08-17", "Assumption, Tue 15 Aug 2028"),
            ("2027-09-06", "2027-09-22", "Children's Day, Mon 20 Sep"),
            ("2027-10-18", "2027-11-03", "All Saints, Mon 1 Nov 2027"),
            ("2026-11-04", "2026-11-20", "Repentance, Wed 18 Nov 2026"),
            ("2027-11-03", "2027-11-19", "Repentance, Wed 17 Nov 2027"),
            ("2028-10-17", "2028-11-03", "Reformation, All Saints"),
        ] {
            let day = |date: &str| NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
            let (from, end) = (day(concluded), day(end));
            assert_eq!(
                withdrawal_period_end(berlin(from.year(), from.month(), from.day(), 12, 0)),
                berlin(end.year(), end.month(), end.day(), 0, 0),
                "{holiday}"
            );
        }
    }

    #[test]
    fn the_day_of_repentance_is_the_wednesday_before_23_november_only() {
        // 23 November 2033 is a Wednesday itself; the holiday is the Wednesday before.
        assert!(is_holiday_in_any_state(
            NaiveDate::from_ymd_opt(2033, 11, 16).unwrap()
        ));
        assert!(!is_holiday_in_any_state(
            NaiveDate::from_ymd_opt(2033, 11, 23).unwrap()
        ));
        assert!(is_holiday_in_any_state(
            NaiveDate::from_ymd_opt(2028, 11, 22).unwrap()
        ));
        // Another Wednesday in November is a working day: 24 November 2027.
        assert!(!is_holiday_in_any_state(
            NaiveDate::from_ymd_opt(2027, 11, 24).unwrap()
        ));
        assert_eq!(
            withdrawal_period_end(berlin(2027, 11, 10, 12, 0)),
            berlin(2027, 11, 25, 0, 0)
        );
    }
}
