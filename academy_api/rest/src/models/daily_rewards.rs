use std::str::FromStr;

use academy_core_daily_rewards_contracts::{
    DailyRewardCategory, DailyRewardClaimAllResponse as CoreClaimAllResponse,
    DailyRewardClaimResponse as CoreClaimResponse, DailyRewardClaimSkip as CoreClaimSkip,
    DailyRewardClaimSkipReason as CoreSkipReason, DailyRewardClaimSuccess as CoreClaimSuccess,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiDailyRewardClaimResponse {
    pub status: &'static str,
    pub success: ApiDailyRewardClaimSuccess,
}

impl ApiDailyRewardClaimResponse {
    pub const STATUS_OK: &'static str = "ok";
}

impl From<CoreClaimResponse> for ApiDailyRewardClaimResponse {
    fn from(response: CoreClaimResponse) -> Self {
        Self {
            status: Self::STATUS_OK,
            success: ApiDailyRewardClaimSuccess::from(response.success),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiDailyRewardClaimSuccess {
    pub category: DailyRewardCategory,
    pub coins: i32,
    pub claimed_at: DateTime<Utc>,
}

impl From<CoreClaimSuccess> for ApiDailyRewardClaimSuccess {
    fn from(success: CoreClaimSuccess) -> Self {
        Self {
            category: success.category,
            coins: success.coins,
            claimed_at: success.claimed_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiDailyRewardClaimAllResponse {
    pub status: &'static str,
    pub claimed: Vec<ApiDailyRewardClaimSuccess>,
    pub skipped_categories: Vec<ApiDailyRewardClaimSkip>,
}

impl ApiDailyRewardClaimAllResponse {
    pub const STATUS_OK: &'static str = "ok";
}

impl From<CoreClaimAllResponse> for ApiDailyRewardClaimAllResponse {
    fn from(response: CoreClaimAllResponse) -> Self {
        let claimed = response
            .claimed
            .into_iter()
            .map(ApiDailyRewardClaimSuccess::from)
            .collect();
        let skipped = response
            .skipped
            .into_iter()
            .map(ApiDailyRewardClaimSkip::from)
            .collect();

        Self {
            status: Self::STATUS_OK,
            claimed,
            skipped_categories: skipped,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiDailyRewardClaimSkip {
    pub category: DailyRewardCategory,
    #[serde(rename = "reason")]
    pub reason: ApiDailyRewardClaimSkipReason,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApiDailyRewardClaimSkipReason {
    Pending,
    Unavailable,
    AlreadyClaimed,
    Error,
}

impl From<CoreClaimSkip> for ApiDailyRewardClaimSkip {
    fn from(skip: CoreClaimSkip) -> Self {
        Self {
            category: skip.category,
            reason: map_skip_reason(skip.reason),
        }
    }
}

fn map_skip_reason(reason: CoreSkipReason) -> ApiDailyRewardClaimSkipReason {
    match reason {
        CoreSkipReason::Pending => ApiDailyRewardClaimSkipReason::Pending,
        CoreSkipReason::Unavailable => ApiDailyRewardClaimSkipReason::Unavailable,
        CoreSkipReason::AlreadyClaimed => ApiDailyRewardClaimSkipReason::AlreadyClaimed,
        CoreSkipReason::Error => ApiDailyRewardClaimSkipReason::Error,
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathDailyRewardCategory {
    #[serde(deserialize_with = "deserialize_category")]
    pub category: DailyRewardCategory,
}

fn deserialize_category<'de, D>(deserializer: D) -> Result<DailyRewardCategory, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    DailyRewardCategory::from_str(&raw).map_err(serde::de::Error::custom)
}
