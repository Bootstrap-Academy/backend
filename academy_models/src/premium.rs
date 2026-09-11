use chrono::{DateTime, Utc};

use crate::{macros::id, user::UserId};

id!(PremiumId);
id!(PremiumRenewalId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumRenewalOffer {
    pub id: String,
    pub monthly_price: u64,
    pub terms_version: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumRenewalConsent {
    pub request_id: PremiumRenewalId,
    pub offer_id: String,
    pub accepted: bool,
    pub withdrawal_consent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PremiumRenewalStatus {
    pub id: PremiumRenewalId,
    pub monthly_price: u64,
    pub confirmation_sent: bool,
}

/// Immutable declaration and confirmation content. Delivery state lives separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PremiumRenewalAgreement {
    pub id: PremiumRenewalId,
    pub user_id: UserId,
    pub received_at: DateTime<Utc>,
    /// Paid-period snapshot at activation, never advanced by a later purchase.
    /// Absent only on retained agreements from before deadline recording.
    pub paid_period_id: Option<PremiumId>,
    pub confirmation_deadline: Option<DateTime<Utc>>,
    pub offer_id: String,
    pub monthly_price: u64,
    pub recipient: String,
    pub document: String,
    pub terms_pdf: Vec<u8>,
    pub withdrawal_pdf: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PremiumPlan {
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PremiumPlanDetails {
    pub price: u64,
    /// Length of the period in calendar months (§ 188 Abs. 2 and Abs. 3 BGB),
    /// not in days.
    pub months: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Premium {
    pub id: PremiumId,
    pub user_id: UserId,
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PremiumStatus {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    pub subscription: Option<PremiumPlan>,
    pub renewal: Option<PremiumRenewalStatus>,
}
