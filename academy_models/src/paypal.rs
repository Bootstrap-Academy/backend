use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{coin::Balance, email_address::EmailAddressWithName};

use crate::{macros::nutype_string, user::UserId, withdrawal::WithdrawalTextVersion};

nutype_string!(PaypalOrderId(validate(len_char_max = 256)));

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaypalCoinOrder {
    pub id: PaypalOrderId,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub captured_at: Option<DateTime<Utc>>,
    pub coins: u64,
    pub invoice_number: u64,
    /// Historical field name. New single orders store the neutral request's
    /// acceptance time; the exact declaration remains in the original contract.
    /// This timestamp alone proves no statutory withdrawal-expiry acknowledgment.
    pub withdrawal_consent_at: Option<DateTime<Utc>>,
    /// Version of the withdrawal instruction the declarations were taken from.
    pub withdrawal_text_version: Option<WithdrawalTextVersion>,
}

/// Immutable commercial facts recorded before the browser can approve payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaypalPaymentSnapshot {
    pub order: PaypalCoinOrder,
    pub request_id: Uuid,
    pub merchant_id: String,
    pub currency: String,
    pub gross_total: Decimal,
    pub net_unit: Decimal,
    pub net_total: Decimal,
    pub vat_total: Decimal,
    pub vat_percent: Decimal,
    pub customer_details: Vec<String>,
    pub recipient: EmailAddressWithName,
    pub consent_text: String,
    #[serde(default)]
    pub contract_order_id: Option<Uuid>,
    /// Accepted prospective bound, absent for historical snapshots. Never reset
    /// by provider approval/capture or populated from a later live configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision_deadline: Option<DateTime<Utc>>,
}

/// Operational progress lives separately from the immutable snapshot. No cascading user FK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaypalPayment {
    pub snapshot: PaypalPaymentSnapshot,
    pub started_at: Option<DateTime<Utc>>,
    pub attempts: i64,
    pub capture: Option<PaypalCapture>,
    pub balance: Option<Balance>,
    pub fulfilled_at: Option<DateTime<Utc>>,
    pub receipt_sent_at: Option<DateTime<Utc>>,
    pub receipt_attempts: i64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaypalCapture {
    pub id: String,
    pub status: String,
    pub currency: String,
    pub amount: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaypalRemoteOrder {
    pub id: PaypalOrderId,
    pub intent: String,
    pub status: String,
    pub merchant_id: String,
    pub currency: String,
    pub amount: Decimal,
    pub captures: Vec<PaypalCapture>,
}

impl PaypalPaymentSnapshot {
    pub fn matches_remote(&self, remote: &PaypalRemoteOrder) -> bool {
        remote.id == self.order.id
            && remote.intent == "CAPTURE"
            && remote.merchant_id == self.merchant_id
            && !self.merchant_id.is_empty()
            && remote.currency == self.currency
            && remote.amount == self.gross_total
            && remote.captures.iter().all(|capture| {
                !capture.id.is_empty()
                    && capture.currency == self.currency
                    && capture.amount == self.gross_total
            })
    }
}
