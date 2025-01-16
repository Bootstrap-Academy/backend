use academy_models::premium::{PremiumPlan, PremiumPlanDetails, PremiumStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiPremiumPlan {
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiPremiumPlanDetails {
    pub price: u64,
    pub months: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiPremiumStatus {
    pub premium: bool,
    pub since: Option<i64>,
    pub until: Option<i64>,
    #[serde(rename = "autopay")]
    pub subscription: Option<ApiPremiumPlan>,
}

impl From<ApiPremiumPlan> for PremiumPlan {
    fn from(value: ApiPremiumPlan) -> Self {
        match value {
            ApiPremiumPlan::Monthly => PremiumPlan::Monthly,
            ApiPremiumPlan::Yearly => PremiumPlan::Yearly,
        }
    }
}

impl From<PremiumPlan> for ApiPremiumPlan {
    fn from(value: PremiumPlan) -> Self {
        match value {
            PremiumPlan::Monthly => ApiPremiumPlan::Monthly,
            PremiumPlan::Yearly => ApiPremiumPlan::Yearly,
        }
    }
}

impl From<PremiumPlanDetails> for ApiPremiumPlanDetails {
    fn from(value: PremiumPlanDetails) -> Self {
        Self {
            price: value.price,
            months: value.months,
        }
    }
}

impl From<PremiumStatus> for ApiPremiumStatus {
    fn from(value: PremiumStatus) -> Self {
        Self {
            premium: true,
            since: Some(value.since.timestamp()),
            until: Some(value.until.timestamp()),
            subscription: value.subscription.map(Into::into),
        }
    }
}

impl From<Option<PremiumStatus>> for ApiPremiumStatus {
    fn from(value: Option<PremiumStatus>) -> Self {
        match value {
            Some(value) => value.into(),
            None => Self {
                premium: false,
                since: None,
                until: None,
                subscription: None,
            },
        }
    }
}
