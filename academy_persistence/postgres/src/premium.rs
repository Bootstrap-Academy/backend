use academy_di::Build;
use academy_models::{
    premium::{Premium, PremiumId, PremiumPlan},
    user::UserId,
};
use academy_persistence_contracts::premium::PremiumRepository;
use academy_utils::trace_instrument;
use bb8_postgres::tokio_postgres::Row;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{arg_indices, columns, ColumnCounter, PostgresTransaction};

#[derive(Debug, Clone, Build)]
pub struct PostgresPremiumRepository;

columns!(premium as "p": "id", "user_id", "since", "until");

impl PremiumRepository<PostgresTransaction> for PostgresPremiumRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get_latest_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<Premium>> {
        txn.txn()
            .query_opt(
                &format!(
                    "select {PREMIUM_COLS} from premium p where user_id=$1 order by until desc \
                     limit 1"
                ),
                &[&*user_id],
            )
            .await
            .map_err(Into::into)
            .map(|row| row.map(|row| decode_premium(&row, &mut Default::default())))
    }

    #[trace_instrument(skip(self, txn))]
    async fn create(&self, txn: &mut PostgresTransaction, premium: Premium) -> anyhow::Result<()> {
        txn.txn()
            .execute(
                &format!(
                    "insert into premium ({PREMIUM_COL_NAMES}) values ({})",
                    arg_indices(1..=PREMIUM_CNT)
                ),
                &[
                    &*premium.id,
                    &*premium.user_id,
                    &premium.since,
                    &premium.until,
                ],
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn extend(
        &self,
        txn: &mut PostgresTransaction,
        id: PremiumId,
        until: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        txn.txn()
            .execute("update premium set until=$2 where id=$1", &[&*id, &until])
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_subscription_users(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<UserId>> {
        txn.txn()
            .query("select user_id from premium_subscriptions", &[])
            .await
            .map_err(Into::into)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| row.get::<_, Uuid>(0).into())
                    .collect()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_subscription(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<PremiumPlan>> {
        txn.txn()
            .query_opt(
                "select plan from premium_subscriptions where user_id=$1",
                &[&*user_id],
            )
            .await
            .map_err(Into::into)
            .and_then(|row| row.map(|row| decode_plan(row.get(0))).transpose())
    }

    #[trace_instrument(skip(self, txn))]
    async fn set_subscription(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        plan: Option<PremiumPlan>,
    ) -> anyhow::Result<()> {
        if let Some(plan) = plan {
            txn.txn()
                .execute(
                    "insert into premium_subscriptions (user_id, plan) values ($1, $2) on \
                     conflict (user_id) do update set plan=$2",
                    &[&*user_id, &encode_plan(plan)],
                )
                .await
        } else {
            txn.txn()
                .execute(
                    "delete from premium_subscriptions where user_id=$1",
                    &[&*user_id],
                )
                .await
        }
        .map(|_| ())
        .map_err(Into::into)
    }
}

fn decode_premium(row: &Row, cnt: &mut ColumnCounter) -> Premium {
    Premium {
        id: row.get::<_, Uuid>(cnt.idx()).into(),
        user_id: row.get::<_, Uuid>(cnt.idx()).into(),
        since: row.get(cnt.idx()),
        until: row.get(cnt.idx()),
    }
}

fn encode_plan(plan: PremiumPlan) -> i16 {
    match plan {
        PremiumPlan::Monthly => 0,
        PremiumPlan::Yearly => 1,
    }
}

fn decode_plan(code: i16) -> anyhow::Result<PremiumPlan> {
    Ok(match code {
        0 => PremiumPlan::Monthly,
        1 => PremiumPlan::Yearly,
        _ => anyhow::bail!("Invalid premium plan code {code}"),
    })
}
