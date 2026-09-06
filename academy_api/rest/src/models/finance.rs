use academy_models::{
    finance::{FinancialDocument, FinancialDocumentKind, FinancialDocumentNumber},
    user::UserId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::contract::ApiTimestamp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiFinancialDocumentKind {
    /// Invoice for a Morphcoin purchase
    Invoice,
    /// Monthly credit note for Morphcoins the user has earned
    CreditNote,
    /// Final statement of the unused share of the purchased Morphcoins, issued
    /// when an account is deleted
    FinalStatement,
}

impl From<ApiFinancialDocumentKind> for FinancialDocumentKind {
    fn from(value: ApiFinancialDocumentKind) -> Self {
        match value {
            ApiFinancialDocumentKind::Invoice => Self::Invoice,
            ApiFinancialDocumentKind::CreditNote => Self::CreditNote,
            ApiFinancialDocumentKind::FinalStatement => Self::FinalStatement,
        }
    }
}

impl From<FinancialDocumentKind> for ApiFinancialDocumentKind {
    fn from(value: FinancialDocumentKind) -> Self {
        match value {
            FinancialDocumentKind::Invoice => Self::Invoice,
            FinancialDocumentKind::CreditNote => Self::CreditNote,
            FinancialDocumentKind::FinalStatement => Self::FinalStatement,
        }
    }
}

/// An invoice, credit note or final statement that has been issued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiFinancialDocument {
    /// Number of the document, which is also the name of its pdf file
    pub number: FinancialDocumentNumber,
    pub kind: ApiFinancialDocumentKind,
    /// The account the document was issued for, null once that account has
    /// been deleted
    pub user_id: Option<UserId>,
    pub issued_at: ApiTimestamp,
    /// Address block as printed on the document, one line per entry. Replaced
    /// by a retention marker once the account has been deleted, except on a
    /// final statement, which keeps it so that a refund can still be offered.
    /// Null for documents issued before the address block was recorded.
    pub customer_details: Option<Vec<String>>,
    /// Number of Morphcoins the document is about
    pub coins: Option<u64>,
    /// Net total in euro cents, as printed
    pub net_total_cents: Option<i64>,
    /// Vat total in euro cents, as printed
    pub vat_total_cents: Option<i64>,
    /// Gross total in euro cents, as printed. On a final statement this is the
    /// amount that can still be refunded.
    pub gross_total_cents: Option<i64>,
}

impl From<FinancialDocument> for ApiFinancialDocument {
    fn from(value: FinancialDocument) -> Self {
        Self {
            number: value.number,
            kind: value.kind.into(),
            user_id: value.user_id,
            issued_at: value.issued_at.into(),
            customer_details: value.customer_details,
            coins: value.coins,
            net_total_cents: value.net_total_cents,
            vat_total_cents: value.vat_total_cents,
            gross_total_cents: value.gross_total_cents,
        }
    }
}
