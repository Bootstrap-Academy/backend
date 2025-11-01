use academy_di::Build;
use academy_models::{
    daily_rewards::{DailyRewardCategory, DailyRewardEntry},
    user::UserId,
};
use academy_persistence_contracts::daily_rewards::{
    DailyRewardEntryUpsert, DailyRewardMarkClaimed, DailyRewardMarkClaimedError,
    DailyRewardMarkReady, DailyRewardRepository,
};
use academy_utils::trace_instrument;
use anyhow::anyhow;
use bb8_postgres::tokio_postgres::Row;
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use postgres_types::Json;
use serde_json::Value;

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresDailyRewardRepository;

impl PostgresDailyRewardRepository {
    fn map_entry(row: Row) -> anyhow::Result<DailyRewardEntry> {
        let map_ts = |value: Option<DateTime<FixedOffset>>| value.map(|dt| dt.with_timezone(&Utc));

        Ok(DailyRewardEntry {
            id: row.try_get("id")?,
            user_id: row.try_get::<_, uuid::Uuid>("user_id")?.into(),
            date_utc: row.try_get("date_utc")?,
            category: row.try_get("category")?,
            coins: row.try_get("coins")?,
            first_detected_at: map_ts(
                row.try_get::<_, Option<DateTime<FixedOffset>>>("first_detected_at")?,
            ),
            last_detected_at: map_ts(
                row.try_get::<_, Option<DateTime<FixedOffset>>>("last_detected_at")?,
            ),
            claimable_since: map_ts(
                row.try_get::<_, Option<DateTime<FixedOffset>>>("claimable_since")?,
            ),
            claimed_at: map_ts(row.try_get::<_, Option<DateTime<FixedOffset>>>("claimed_at")?),
            activity_sample: row
                .try_get::<_, Option<Json<Value>>>("activity_sample")?
                .map(|json| json.0),
            created_at: row
                .try_get::<_, DateTime<FixedOffset>>("created_at")?
                .with_timezone(&Utc),
            updated_at: row
                .try_get::<_, DateTime<FixedOffset>>("updated_at")?
                .with_timezone(&Utc),
        })
    }

    async fn get_entry(
        txn: &mut PostgresTransaction,
        user_id: UserId,
        date_utc: NaiveDate,
        category: DailyRewardCategory,
    ) -> anyhow::Result<Option<DailyRewardEntry>> {
        let row = txn
            .txn()
            .query_opt(
                r#"
select *
from daily_reward_entries
where user_id = $1
  and date_utc = $2
  and category = $3::daily_reward_category
"#,
                &[&*user_id, &date_utc, &category],
            )
            .await?;

        row.map(Self::map_entry).transpose()
    }
}

impl DailyRewardRepository<PostgresTransaction> for PostgresDailyRewardRepository {
    #[trace_instrument(skip(self, txn))]
    async fn list_by_user_and_date(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        date_utc: NaiveDate,
    ) -> anyhow::Result<Vec<DailyRewardEntry>> {
        let rows = txn
            .txn()
            .query(
                r#"
select *
from daily_reward_entries
where user_id = $1
  and date_utc = $2
order by category asc
"#,
                &[&*user_id, &date_utc],
            )
            .await?;

        rows.into_iter().map(Self::map_entry).collect()
    }

    #[trace_instrument(skip(self, txn))]
    async fn upsert_entry(
        &self,
        txn: &mut PostgresTransaction,
        params: DailyRewardEntryUpsert,
    ) -> anyhow::Result<DailyRewardEntry> {
        let row = txn
            .txn()
            .query_one(
                r#"
insert into daily_reward_entries (id, user_id, date_utc, category, coins)
values ($1, $2, $3, $4::daily_reward_category, $5)
on conflict (user_id, date_utc, category)
do update set coins = excluded.coins, updated_at = now()
returning *
"#,
                &[
                    &params.id,
                    &*params.user_id,
                    &params.date_utc,
                    &params.category,
                    &params.coins,
                ],
            )
            .await?;

        Self::map_entry(row)
    }

    #[trace_instrument(skip(self, txn))]
    async fn mark_ready(
        &self,
        txn: &mut PostgresTransaction,
        params: DailyRewardMarkReady,
    ) -> anyhow::Result<DailyRewardEntry> {
        let activity_sample: Option<Json<&Value>> = params.activity_sample.as_ref().map(Json);
        let row = txn
            .txn()
            .query_one(
                r#"
update daily_reward_entries
   set first_detected_at = coalesce(first_detected_at, $4::timestamptz),
       last_detected_at = case
           when $5::timestamptz is null then last_detected_at
           when last_detected_at is null then $5::timestamptz
           else greatest(last_detected_at, $5::timestamptz)
       end,
       claimable_since = coalesce(claimable_since, $6::timestamptz),
       activity_sample = coalesce($7::jsonb, activity_sample),
       updated_at = now()
where user_id = $1
  and date_utc = $2
  and category = $3::daily_reward_category
returning *
"#,
                &[
                    &*params.user_id,
                    &params.date_utc,
                    &params.category,
                    &params.first_detected_at,
                    &params.last_detected_at,
                    &params.claimable_since,
                    &activity_sample,
                ],
            )
            .await?;

        Self::map_entry(row)
    }

    #[trace_instrument(skip(self, txn))]
    async fn mark_claimed(
        &self,
        txn: &mut PostgresTransaction,
        params: DailyRewardMarkClaimed,
    ) -> Result<DailyRewardEntry, DailyRewardMarkClaimedError> {
        let row = txn
            .txn()
            .query_opt(
                r#"
update daily_reward_entries
   set claimed_at = $4,
       updated_at = $4
 where user_id = $1
   and date_utc = $2
   and category = $3::daily_reward_category
   and claimable_since is not null
  and claimed_at is null
returning *
"#,
                &[
                    &*params.user_id,
                    &params.date_utc,
                    &params.category,
                    &params.claimed_at,
                ],
            )
            .await
            .map_err(|err| DailyRewardMarkClaimedError::Other(err.into()))?;

        if let Some(row) = row {
            return Self::map_entry(row).map_err(DailyRewardMarkClaimedError::Other);
        }

        let existing = Self::get_entry(txn, params.user_id, params.date_utc, params.category)
            .await
            .map_err(DailyRewardMarkClaimedError::Other)?;

        match existing {
            None => Err(DailyRewardMarkClaimedError::NotFound),
            Some(entry) => {
                if entry.claimable_since.is_none() {
                    Err(DailyRewardMarkClaimedError::NotReady)
                } else if entry.claimed_at.is_some() {
                    Err(DailyRewardMarkClaimedError::AlreadyClaimed)
                } else {
                    Err(DailyRewardMarkClaimedError::Other(anyhow!(
                        "Failed to claim reward due to unknown row state"
                    )))
                }
            }
        }
    }
}
