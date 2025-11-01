use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    user::UserId,
};
use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DailyRewardStatus {
    Pending,
    Ready,
    Claimed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DailyRewardUnavailableReason {
    NoRecommendation,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardItem {
    pub category: DailyRewardCategory,
    pub coins: i32,
    pub status: DailyRewardStatus,
    pub claimable_since: Option<DateTime<Utc>>,
    pub last_detected_at: Option<DateTime<Utc>>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub activity_sample: Option<Value>,
    pub unavailable_reason: Option<DailyRewardUnavailableReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardClaimTotals {
    pub available_coins: i32,
    pub claimed_today: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardsSnapshot {
    pub date_utc: NaiveDate,
    pub feature_enabled: bool,
    pub rewards: Vec<DailyRewardItem>,
    pub claim_totals: DailyRewardClaimTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardClaimSuccess {
    pub category: DailyRewardCategory,
    pub coins: i32,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DailyRewardClaimSkipReason {
    Pending,
    Unavailable,
    AlreadyClaimed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardClaimSkip {
    pub category: DailyRewardCategory,
    pub reason: DailyRewardClaimSkipReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardClaimAllResponse {
    pub claimed: Vec<DailyRewardClaimSuccess>,
    pub skipped: Vec<DailyRewardClaimSkip>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardClaimResponse {
    pub success: DailyRewardClaimSuccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardGetResponse {
    pub snapshot: DailyRewardsSnapshot,
}

#[derive(Debug, Error)]
pub enum DailyRewardGetError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("Daily rewards feature is disabled.")]
    FeatureDisabled,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum DailyRewardClaimError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("Daily rewards feature is disabled.")]
    FeatureDisabled,
    #[error("Reward is not ready to claim.")]
    NotReady,
    #[error("Reward has already been claimed.")]
    AlreadyClaimed,
    #[error("Reward is currently unavailable.")]
    Unavailable,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum DailyRewardClaimAllError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("Daily rewards feature is disabled.")]
    FeatureDisabled,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait DailyRewardFeatureService: Send + Sync + 'static {
    fn get_today(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<DailyRewardGetResponse, DailyRewardGetError>> + Send;

    fn claim(
        &self,
        token: &AccessToken,
        category: DailyRewardCategory,
    ) -> impl Future<Output = Result<DailyRewardClaimResponse, DailyRewardClaimError>> + Send;

    fn claim_all(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<DailyRewardClaimAllResponse, DailyRewardClaimAllError>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardActivity {
    pub first_detected_at: DateTime<Utc>,
    pub last_detected_at: DateTime<Utc>,
    pub activity_sample: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardActivityState {
    pub detected: Option<DailyRewardActivity>,
    pub unavailable_reason: Option<DailyRewardUnavailableReason>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct DailyRewardActivitySnapshot {
    pub lecture: DailyRewardActivityState,
    pub practice: DailyRewardActivityState,
    pub lab: DailyRewardActivityState,
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait DailyRewardActivityService: Send + Sync + 'static {
    fn detect(
        &self,
        user_id: UserId,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<DailyRewardActivitySnapshot>> + Send;
}

pub use academy_models::daily_rewards::{DailyRewardCategory, DailyRewardEntry};
