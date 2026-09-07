use std::{fmt::Debug, sync::LazyLock};

use academy_assets::templates;
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

pub mod format;

/// The logo as it is embedded into every template, so that no mail has to load
/// an image from a remote host.
pub static LOGO_BASE64: LazyLock<String> =
    LazyLock::new(|| BASE64_STANDARD.encode(academy_assets::email::LOGO_TEXT_PNG));

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait TemplateService: Send + Sync + 'static {
    /// Render the given template.
    fn render<T: Template + 'static>(&self, template: &T) -> anyhow::Result<String>;
}

#[cfg(feature = "mock")]
impl MockTemplateService {
    pub fn with_render<T: Template + Send + PartialEq + std::fmt::Debug + 'static>(
        mut self,
        template: T,
        result: String,
    ) -> Self {
        self.expect_render()
            .once()
            .with(mockall::predicate::eq(template))
            .return_once(|_| Ok(result));
        self
    }
}

pub trait Template: Serialize + Debug {
    const NAME: &'static str;
    const TEMPLATE: &'static str;
}

macro_rules! templates {
    ($( $ident:ident ( $template:expr ), )* ) => {
        $(
            impl Template for $ident {
                const NAME: &'static str = stringify!($ident);
                const TEMPLATE: &'static str = $template;
            }
        )*

        pub const TEMPLATES: &[(&str, &str)] = &[
            $( ($ident::NAME, $ident::TEMPLATE) ),*
        ];
    };
}

templates! {
    ResetPasswordTemplate(templates::RESET_PASSWORD_HTML),
    VerifyEmailTemplate(templates::VERIFY_EMAIL_HTML),
    PurchaseConfirmationTemplate(templates::PURCHASE_CONFIRMATION_HTML),
    InvoiceTemplate(templates::INVOICE_HTML),
    FinalStatementTemplate(templates::FINAL_STATEMENT_HTML),
    ContractCancellationConfirmationTemplate(templates::CONTRACT_CANCELLATION_CONFIRMATION_HTML),
    ContractWithdrawalConfirmationTemplate(templates::CONTRACT_WITHDRAWAL_CONFIRMATION_HTML),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResetPasswordTemplate {
    pub code: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyEmailTemplate {
    pub code: String,
    pub url: String,
}

/// Confirmation of a contract cancellation (§ 312k BGB).
///
/// All timestamps are pre-formatted strings in the `Europe/Berlin` time zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContractCancellationConfirmationTemplate {
    pub received_at: String,
    pub name: String,
    pub email: String,
    pub contract: String,
    /// The contract as the declarant named it, if they named it.
    pub contract_designation: Option<String>,
    pub cancellation_type: String,
    /// Whether the cancellation is an extraordinary one, for which no end date
    /// is determined automatically.
    pub extraordinary: bool,
    pub details: Option<String>,
    pub requested_end: Option<String>,
    pub effective_end: Option<String>,
}

/// Confirmation of a withdrawal from a contract (§ 356a BGB).
///
/// All timestamps are pre-formatted strings in the `Europe/Berlin` time zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContractWithdrawalConfirmationTemplate {
    pub received_at: String,
    pub name: String,
    pub email: String,
    pub contract: String,
    /// The contract or order as the declarant named it, if they named it.
    pub contract_designation: Option<String>,
    pub details: Option<String>,
}

/// Confirmation of a Morphcoin purchase.
///
/// Every number is serialized as an already formatted German string (see
/// [`format`]), so the mail reads the way the checkout does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurchaseConfirmationTemplate {
    #[serde(serialize_with = "format::serialize_count")]
    pub coins: u64,
    #[serde(serialize_with = "format::serialize_percent")]
    pub vat_percent: Decimal,
    #[serde(serialize_with = "format::serialize_amount")]
    pub vat_total: Decimal,
    #[serde(serialize_with = "format::serialize_amount")]
    pub gross_total: Decimal,
    /// The declarations the consumer gave at checkout, repeated in the
    /// confirmation of the contract (§ 312f Abs. 3 BGB).
    pub withdrawal_consent: Option<WithdrawalConsentConfirmation>,
}

/// The declarations under § 356 Abs. 5 Nr. 2 / Abs. 6 Nr. 2 BGB as they are
/// repeated in a confirmation email.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WithdrawalConsentConfirmation {
    /// Wording of the declarations, verbatim.
    pub text: String,
    /// Version of the withdrawal instruction the wording was taken from.
    pub version: String,
    /// Time at which the declarations were given, already formatted.
    pub timestamp: String,
}

/// An invoice or, with a different title, a credit note.
///
/// Every number is serialized as an already formatted German string (see
/// [`format`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvoiceTemplate {
    pub title: &'static str,
    pub customer_details: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub invoice_number: String,
    pub items: Vec<InvoiceItem>,
    #[serde(serialize_with = "format::serialize_percent")]
    pub vat_percent: Decimal,
    #[serde(serialize_with = "format::serialize_amount")]
    pub net_total: Decimal,
    #[serde(serialize_with = "format::serialize_amount")]
    pub vat_total: Decimal,
    #[serde(serialize_with = "format::serialize_amount")]
    pub gross_total: Decimal,
}

/// Final statement of the unused share of the purchased Morphcoins, issued
/// when an account is deleted (AGB Ziffer 6.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinalStatementTemplate {
    pub title: &'static str,
    /// Address block, including the email address a later refund request is
    /// answered to.
    pub customer_details: Vec<String>,
    /// Point in time at which the account was deleted.
    pub timestamp: DateTime<Utc>,
    pub statement_number: String,
    /// Morphcoins the account has bought, from its invoices.
    #[serde(serialize_with = "format::serialize_count")]
    pub purchased_coins: u64,
    /// Morphcoin balance at the moment of the deletion.
    #[serde(serialize_with = "format::serialize_count")]
    pub balance_coins: u64,
    /// Unused share of the purchased Morphcoins.
    #[serde(serialize_with = "format::serialize_count")]
    pub unused_coins: u64,
    /// Number of Morphcoins that correspond to one Euro.
    #[serde(serialize_with = "format::serialize_count")]
    pub coins_per_euro: u64,
    /// Euro value of `unused_coins`.
    #[serde(serialize_with = "format::serialize_amount")]
    pub refund_amount: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvoiceDetail {
    pub name: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvoiceItem {
    pub description: String,
    /// Net price of a single unit. Kept at four decimal places, because it is
    /// the value that has to multiply out to `net_total`.
    #[serde(serialize_with = "format::serialize_unit_price")]
    pub net_unit: Decimal,
    #[serde(serialize_with = "format::serialize_count")]
    pub count: u64,
    #[serde(serialize_with = "format::serialize_amount")]
    pub net_total: Decimal,
}
