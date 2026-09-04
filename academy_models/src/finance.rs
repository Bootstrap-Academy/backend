//! Invoices and credit notes as documents that have to be kept.
//!
//! Invoices and credit notes are retained for eight years, counted from the
//! end of the calendar year in which they were issued (§ 147 Abs. 3 Satz 1 und
//! Abs. 4 AO, § 257 Abs. 4 HGB, § 14b Abs. 1 UStG). The record of an issued
//! document therefore outlives the account it was issued for: the account
//! reference is dropped and the customer details are replaced by
//! [`RETENTION_MARKER`], while number, amounts and dates are kept.

use chrono::{DateTime, Datelike, TimeZone, Utc};

use crate::{macros::nutype_string, user::UserId};

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
}

impl FinancialDocumentKind {
    /// Identifier of this kind in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invoice => "invoice",
            Self::CreditNote => "credit_note",
        }
    }
}

impl std::str::FromStr for FinancialDocumentKind {
    type Err = InvalidFinancialDocumentKindError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "invoice" => Ok(Self::Invoice),
            "credit_note" => Ok(Self::CreditNote),
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

/// Return the timestamp before which documents may be deleted at `now`.
///
/// The retention period of `years` years begins at the end of the calendar
/// year in which the document was issued (§ 147 Abs. 3 Satz 1 und Abs. 4 AO),
/// so a document issued in 2024 may be deleted from the beginning of 2033.
///
/// Returns `None` if the resulting year is outside the representable range.
pub fn retention_cutoff(now: DateTime<Utc>, years: u32) -> Option<DateTime<Utc>> {
    let year = now.year().checked_sub(i32::try_from(years).ok()?)?;
    Utc.with_ymd_and_hms(year, 1, 1, 0, 0, 0).single()
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
        for kind in [
            FinancialDocumentKind::Invoice,
            FinancialDocumentKind::CreditNote,
        ] {
            assert_eq!(
                kind.as_str().parse::<FinancialDocumentKind>().unwrap(),
                kind
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
    fn cutoff_out_of_range() {
        assert_eq!(retention_cutoff(date(2026, 9, 3), u32::MAX), None);
    }
}
