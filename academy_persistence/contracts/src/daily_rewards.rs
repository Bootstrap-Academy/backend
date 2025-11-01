use std::future::Future;

use academy_models::{
    daily_rewards::{DailyRewardCategory, DailyRewardEntry},
    user::UserId,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DailyRewardEntryUpsert {
    pub id: Uuid,
    pub user_id: UserId,
    pub date_utc: NaiveDate,
    pub category: DailyRewardCategory,
    pub coins: i32,
}

#[derive(Debug, Clone)]
pub struct DailyRewardMarkReady {
    pub user_id: UserId,
    pub date_utc: NaiveDate,
    pub category: DailyRewardCategory,
    pub first_detected_at: Option<DateTime<Utc>>,
    pub last_detected_at: Option<DateTime<Utc>>,
    pub claimable_since: Option<DateTime<Utc>>,
    pub activity_sample: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct DailyRewardMarkClaimed {
    pub user_id: UserId,
    pub date_utc: NaiveDate,
    pub category: DailyRewardCategory,
    pub claimed_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum DailyRewardMarkClaimedError {
    #[error("Daily reward is not yet ready to claim.")]
    NotReady,
    #[error("Daily reward already claimed.")]
    AlreadyClaimed,
    #[error("Daily reward entry not found.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait DailyRewardRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    fn list_by_user_and_date(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        date_utc: NaiveDate,
    ) -> impl Future<Output = anyhow::Result<Vec<DailyRewardEntry>>> + Send;

    fn upsert_entry(
        &self,
        txn: &mut Txn,
        params: DailyRewardEntryUpsert,
    ) -> impl Future<Output = anyhow::Result<DailyRewardEntry>> + Send;

    fn mark_ready(
        &self,
        txn: &mut Txn,
        params: DailyRewardMarkReady,
    ) -> impl Future<Output = anyhow::Result<DailyRewardEntry>> + Send;

    fn mark_claimed(
        &self,
        txn: &mut Txn,
        params: DailyRewardMarkClaimed,
    ) -> impl Future<Output = Result<DailyRewardEntry, DailyRewardMarkClaimedError>> + Send;
}
