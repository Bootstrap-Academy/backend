use std::{fmt::Debug, sync::LazyLock};

use academy_assets::templates;
use academy_utils::static_value;
use base64::{Engine, prelude::BASE64_STANDARD};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Serialize, Serializer};

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
    SubscribeNewsletterTemplate(templates::SUBSCRIBE_NEWSLETTER_HTML),
    PurchaseConfirmationTemplate(templates::PURCHASE_CONFIRMATION_HTML),
    InvoiceTemplate(templates::INVOICE_HTML),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubscribeNewsletterTemplate {
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
    pub cancellation_type: String,
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
    pub details: Option<String>,
}

macro_rules! rounded {
    ($($ident:ident($digits:literal)),* $(,)?) => { $(
        fn $ident<S: Serializer>(num: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
            Serialize::serialize(&num.round_dp($digits), serializer)
        }
    )* };
}
rounded! {
    rounded_2(2),
    rounded_4(4),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PurchaseConfirmationTemplate {
    pub coins: u64,
    pub vat_percent: Decimal,
    #[serde(serialize_with = "rounded_2")]
    pub vat_total: Decimal,
    #[serde(serialize_with = "rounded_2")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvoiceTemplate {
    pub title: &'static str,
    pub customer_details: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub invoice_number: String,
    pub items: Vec<InvoiceItem>,
    pub vat_percent: Decimal,
    #[serde(serialize_with = "rounded_2")]
    pub net_total: Decimal,
    #[serde(serialize_with = "rounded_2")]
    pub vat_total: Decimal,
    #[serde(serialize_with = "rounded_2")]
    pub gross_total: Decimal,
    #[serde(flatten)]
    pub _static: InvoiceTemplateStatic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvoiceDetail {
    pub name: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InvoiceItem {
    pub description: String,
    #[serde(serialize_with = "rounded_4")]
    pub net_unit: Decimal,
    pub count: u64,
    #[serde(serialize_with = "rounded_2")]
    pub net_total: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct InvoiceTemplateStatic {
    logo_base64: LogoBase64,
}

static_value!(LogoBase64(LOGO_BASE64.as_str()));
