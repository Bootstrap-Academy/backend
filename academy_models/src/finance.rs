//! Invoices, credit notes and final statements as documents that have to be
//! kept.
//!
//! They are retained for eight years, counted from the end of the calendar
//! year in which they were issued (§ 147 Abs. 3 Satz 1 und Abs. 4 AO,
//! § 257 Abs. 4 HGB, § 14b Abs. 1 UStG). The record of an issued document
//! therefore outlives the account it was issued for: the account reference is
//! dropped and the customer details are replaced by [`RETENTION_MARKER`],
//! while number, amounts and dates are kept. The final statement of a deleted
//! account keeps its customer details, because the unused share of the
//! purchased Morphcoins it records can only be refunded to somebody it still
//! names (AGB Ziffer 6.7).

use chrono::{DateTime, Datelike, TimeZone, Utc};
use chrono_tz::Europe::Berlin;

use crate::{macros::nutype_string, user::UserId};

/// Time zone the calendar year of a document is determined in.
///
/// The retention period is a German one and the documents are issued by a
/// German company, so the calendar year of a document is the year it carries
/// in `Europe/Berlin`, which is also the date printed on it.
pub const DOCUMENT_TIME_ZONE: chrono_tz::Tz = Berlin;

nutype_string!(FinancialDocumentNumber(validate(
    len_char_min = 1,
    len_char_max = 64
)));

/// Replaces the customer details of a document once the account it was issued
/// for has been deleted.
pub const RETENTION_MARKER: &str = "Gelöschtes Konto (Aufbewahrung nach § 147 Abs. 3 AO)";

/// Kind of a financial document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FinancialDocumentKind {
    /// Invoice for a Morphcoin purchase.
    Invoice,
    /// Monthly credit note for Morphcoins the user has earned.
    CreditNote,
    /// Final statement of the unused share of the purchased Morphcoins, issued
    /// when an account is deleted.
    FinalStatement,
}

impl FinancialDocumentKind {
    /// All kinds, in the order they are listed in.
    pub const ALL: &'static [Self] = &[Self::Invoice, Self::CreditNote, Self::FinalStatement];

    /// Identifier of this kind in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::CreditNote => "credit_note",
            Self::FinalStatement => "final_statement",
        }
    }

    /// Whether documents of this kind keep the customer details they were
    /// issued with after the account has been deleted.
    ///
    /// A final statement records the unused share of the purchased Morphcoins
    /// so that it can still be refunded on request after the deletion, which
    /// is only possible if it still names the person to refund it to.
    pub fn keeps_customer_details(self) -> bool {
        matches!(self, Self::FinalStatement)
    }
}

impl std::str::FromStr for FinancialDocumentKind {
    type Err = InvalidFinancialDocumentKindError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "invoice" => Ok(Self::Invoice),
            "credit_note" => Ok(Self::CreditNote),
            "final_statement" => Ok(Self::FinalStatement),
            _ => Err(InvalidFinancialDocumentKindError),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("The financial document kind is invalid.")]
pub struct InvalidFinancialDocumentKindError;

/// A financial document that has been issued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinancialDocument {
    /// Number of the document, which is also the name of its pdf file.
    pub number: FinancialDocumentNumber,
    pub kind: FinancialDocumentKind,
    /// The account the document was issued for, or `None` if that account has
    /// been deleted.
    pub user_id: Option<UserId>,
    pub issued_at: DateTime<Utc>,
    /// Address block as printed on the document, one line per entry.
    ///
    /// [`RETENTION_MARKER`] once the account has been deleted, and `None` for
    /// documents that were issued before the address block was recorded.
    pub customer_details: Option<Vec<String>>,
    pub coins: Option<u64>,
    /// Totals in euro cents, rounded exactly as they are printed.
    pub net_total_cents: Option<i64>,
    pub vat_total_cents: Option<i64>,
    pub gross_total_cents: Option<i64>,
}

/// Number of the final statement that is issued for the account with the given
/// customer number.
///
/// An account is deleted once, so one final statement exists per customer
/// number.
pub fn final_statement_number(user_number: u64) -> String {
    format!("S{user_number}")
}

/// Unused share of the purchased Morphcoins (AGB Ziffer 6.1 and 6.7).
///
/// Reward coins count as consumed first, so the unused share is the part of
/// the current balance that does not exceed the Morphcoins the account has
/// bought. Morphcoins that have already been refunded were taken off the
/// balance when the refund was made and are therefore already deducted, unless
/// the balance was topped up with reward coins afterwards.
pub fn unused_purchased_coins(balance: u64, purchased: u64) -> u64 {
    balance.min(purchased)
}

/// Return the timestamp before which documents may be deleted at `now`.
///
/// The retention period of `years` years begins at the end of the calendar
/// year in which the document was issued (§ 147 Abs. 3 Satz 1 und Abs. 4 AO),
/// so a document issued in 2024 may be deleted from the beginning of 2033.
///
/// Both the current year and the beginning of the year are taken in
/// [`DOCUMENT_TIME_ZONE`]. In UTC, a document issued between 23:00 and
/// midnight on 31 December would be filed under the previous year and pruned
/// a year before the German period ends.
///
/// Returns `None` if the resulting year is outside the representable range.
pub fn retention_cutoff(now: DateTime<Utc>, years: u32) -> Option<DateTime<Utc>> {
    let year = now
        .with_timezone(&DOCUMENT_TIME_ZONE)
        .year()
        .checked_sub(i32::try_from(years).ok()?)?;

    DOCUMENT_TIME_ZONE
        .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .map(|start_of_year| start_of_year.with_timezone(&Utc))
}

/// Return the time at which the credit note with the given number was issued.
///
/// A credit note is numbered `G<year><month>-<user number>` and covers one
/// calendar month, so it is issued at the beginning of the following month.
/// Returns `None` if the number is not a credit note number.
pub fn credit_note_issued_at(number: &str) -> Option<DateTime<Utc>> {
    let (month, user_number) = number.strip_prefix('G')?.split_once('-')?;
    if month.len() != 6
        || !month.bytes().all(|byte| byte.is_ascii_digit())
        || user_number.is_empty()
        || !user_number.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    let year = month[..4].parse::<i32>().ok()?;
    let month = month[4..].parse::<u32>().ok()?;
    let (year, month) = match month {
        1..=11 => (year, month + 1),
        12 => (year.checked_add(1)?, 1),
        _ => return None,
    };

    Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap()
    }

    #[test]
    fn kind_round_trip() {
        for &kind in FinancialDocumentKind::ALL {
            assert_eq!(
                kind.as_str().parse::<FinancialDocumentKind>().unwrap(),
                kind
            );
        }
    }

    #[test]
    fn only_the_final_statement_keeps_its_customer_details() {
        for &kind in FinancialDocumentKind::ALL {
            assert_eq!(
                kind.keeps_customer_details(),
                kind == FinancialDocumentKind::FinalStatement,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn cutoff_starts_at_the_end_of_the_year_of_issue() {
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

    /// A document issued between 23:00 UTC and midnight on 31 December already
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
    fn credit_note_issue_date() {
        assert_eq!(
            credit_note_issued_at("G202402-7"),
            Some(Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap())
        );
        assert_eq!(
            credit_note_issued_at("G202412-1337"),
            Some(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap())
        );

        for number in [
            "R0000042",
            "G202402",
            "G20240-7",
            "G2024021-7",
            "G2024ab-7",
            "G202413-7",
            "G202400-7",
            "Gäöüßx-7",
            "",
        ] {
            assert_eq!(credit_note_issued_at(number), None, "{number}");
        }
    }

    #[test]
    fn unused_purchased_share() {
        // Nothing bought, only reward coins in the balance.
        assert_eq!(unused_purchased_coins(1337, 0), 0);
        // Bought and not spent.
        assert_eq!(unused_purchased_coins(1000, 1000), 1000);
        // Reward coins on top of the purchased ones are not refundable.
        assert_eq!(unused_purchased_coins(1500, 1000), 1000);
        // Spending is taken from the reward coins first.
        assert_eq!(unused_purchased_coins(1200, 1000), 1000);
        // Once the reward coins are used up, the purchased share shrinks.
        assert_eq!(unused_purchased_coins(800, 1000), 800);
        assert_eq!(unused_purchased_coins(0, 1000), 0);
    }

    #[test]
    fn final_statement_numbers_are_not_credit_note_numbers() {
        assert_eq!(final_statement_number(1337), "S1337");
        assert_eq!(credit_note_issued_at(&final_statement_number(1337)), None);
    }

    #[test]
    fn cutoff_out_of_range() {
        assert_eq!(retention_cutoff(date(2026, 9, 3), u32::MAX), None);
    }
}
