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

use chrono::{DateTime, TimeZone, Utc};

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
    /// When the claim the document records was closed out, or `None` while it
    /// is still open.
    ///
    /// Only a final statement records a claim: the unused share of the
    /// purchased Morphcoins, which is refunded by hand on request. The
    /// timestamp is therefore set by hand as well, with
    /// `academy admin finance settle <number>`, so that the same statement
    /// cannot be paid out twice.
    pub settled_at: Option<DateTime<Utc>>,
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
}
