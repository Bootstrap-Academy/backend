use chrono::{DateTime, Utc};

use crate::{macros::id, user::UserId};

id!(PremiumId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PremiumPlan {
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PremiumPlanDetails {
    pub price: u64,
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
}
