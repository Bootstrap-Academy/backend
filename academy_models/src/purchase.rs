//! Accepted offers and consumer artifacts are independent of mutable catalog/account state.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Customer-visible hearts use two of the integer units used by task debits.
/// Keep integer arithmetic so odd quantities and large configured values stay exact.
pub fn display_hearts(units: u64) -> String {
    if units.is_multiple_of(2) {
        (units / 2).to_string()
    } else {
        format!("{},5", units / 2)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseProduct {
    pub kind: String,
    pub reference: String,
    pub title: String,
    pub description: String,
    pub coins: u64,
    /// Authoritative product data (period, instructor, scope, access conditions).
    pub facts: Value,
    pub revision: String,
    pub service_starts_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOffer {
    pub id: Uuid,
    pub user_id: Uuid,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub recipient: String,
    pub product: PurchaseProduct,
    pub document_hash: String,
    pub hash: String,
    pub text: String,
    pub declaration: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provision_window_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PurchaseAcceptance {
    pub order_id: Uuid,
    pub offer_hash: String,
    pub accepted: bool,
    pub early_performance_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseStatus {
    pub offer: PurchaseOffer,
    pub state: String,
    pub accepted_at: Option<DateTime<Utc>>,
    pub confirmation_smtp_accepted_at: Option<DateTime<Utc>>,
    pub fulfillment: Option<Value>,
    pub financial_evidence: Option<Value>,
    pub review_reason: Option<String>,
    #[serde(default)]
    pub provision_deadline: Option<DateTime<Utc>>,
    #[serde(default)]
    pub provision_timing: Option<Value>,
    #[serde(default)]
    pub document_corrections: Vec<String>,
}

impl PurchaseStatus {
    pub fn provision_deadline(&self) -> Option<DateTime<Utc>> {
        let elapsed = self.offer.provision_window_seconds.and_then(|seconds| {
            self.accepted_at?
                .checked_add_signed(chrono::TimeDelta::seconds(seconds.try_into().ok()?))
        });
        match (elapsed, self.offer.product.service_starts_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PurchaseRecord {
    pub status: PurchaseStatus,
    pub terms_pdf: Vec<u8>,
    pub withdrawal_pdf: Vec<u8>,
    pub confirmation_body: Option<String>,
    pub delivery_generation: i64,
    pub submission: Option<PurchaseAcceptance>,
    pub message_metadata: Option<Value>,
}
