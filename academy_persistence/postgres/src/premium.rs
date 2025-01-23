use academy_di::Build;
use academy_models::{
    premium::{Premium, PremiumId, PremiumPlan},
    user::UserId,
};
use academy_persistence_contracts::premium::PremiumRepository;
use academy_utils::trace_instrument;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        premium::{CreateParams, ExtendParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresPremiumRepository;

impl PremiumRepository<PostgresTransaction> for PostgresPremiumRepository {
    #[trace_instrument(skip(self, txn))]
    async fn get_latest_by_user_id(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<Premium>> {
        queries::premium::get_latest_by_user_id()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .map(|row| row.map(decode_premium))
    }

    #[trace_instrument(skip(self, txn))]
    async fn create(&self, txn: &mut PostgresTransaction, premium: Premium) -> anyhow::Result<()> {
        let params = CreateParams {
            id: *premium.id,
            user_id: *premium.user_id,
            since: premium.since.into(),
            until: premium.until.into(),
        };

        queries::premium::create()
            .params(txn.txn(), &params)
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
        let params = ExtendParams {
            id: *id,
            until: until.into(),
        };

        queries::premium::extend()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_subscription_users(
        &self,
        txn: &mut PostgresTransaction,
    ) -> anyhow::Result<Vec<UserId>> {
        queries::premium::list_subscription_users()
            .bind(txn.txn())
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).map(UserId::from))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_subscription(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<PremiumPlan>> {
        queries::premium::get_subscription()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .map(|row| row.map(decode_plan))
    }

    #[trace_instrument(skip(self, txn))]
    async fn set_subscription(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        plan: Option<PremiumPlan>,
    ) -> anyhow::Result<()> {
        queries::premium::set_subscription()
            .bind(txn.txn(), &user_id, &plan.map(encode_plan))
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

fn decode_premium(value: queries::premium::Premium) -> Premium {
    Premium {
        id: value.id.into(),
        user_id: value.user_id.into(),
        since: value.since.into(),
        until: value.until.into(),
    }
}

fn encode_plan(plan: PremiumPlan) -> clorinde::types::PremiumPlan {
    match plan {
        PremiumPlan::Monthly => clorinde::types::PremiumPlan::monthly,
        PremiumPlan::Yearly => clorinde::types::PremiumPlan::yearly,
    }
}

fn decode_plan(value: clorinde::types::PremiumPlan) -> PremiumPlan {
    match value {
        clorinde::types::PremiumPlan::monthly => PremiumPlan::Monthly,
        clorinde::types::PremiumPlan::yearly => PremiumPlan::Yearly,
    }
}
