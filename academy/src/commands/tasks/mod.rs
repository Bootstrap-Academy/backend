use academy_config::Config;
use academy_core_premium_contracts::premium::PremiumService;
use academy_di::Provide;
use academy_models::user::UserId;
use academy_persistence_contracts::{
    Database, Transaction, premium::PremiumRepository, session::SessionRepository,
};
use academy_persistence_postgres::session::PostgresSessionRepository;
use anyhow::Context;
use chrono::{NaiveDate, Utc};
use clap::Subcommand;
use tracing::info;
use uuid::Uuid;

use crate::{
    database,
    environment::{Provider, types},
};

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Remove expired records from the database.
    PruneDatabase,
    /// Refresh premium subscriptions.
    RefreshPremium,
    /// Rebuild a user's daily rewards snapshot.
    DailyRewardsRebuild {
        /// Target user ID.
        #[clap(long = "user")]
        user: Uuid,
        /// UTC date (YYYY-MM-DD). Defaults to today.
        #[clap(long = "date")]
        date: Option<NaiveDate>,
    },
}

impl TaskCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            TaskCommand::PruneDatabase => prune_database(config).await,
            TaskCommand::RefreshPremium => refresh_premium(config).await,
            TaskCommand::DailyRewardsRebuild { user, date } => {
                daily_rewards_rebuild(config, user, date).await
            }
        }
    }
}

async fn prune_database(config: Config) -> anyhow::Result<()> {
    let db = database::connect(&config.database).await?;
    let mut txn = db.begin_transaction().await?;

    let session_repo = PostgresSessionRepository;
    let now = Utc::now();
    let pruned = session_repo
        .delete_by_updated_at(&mut txn, now - config.session.refresh_token_ttl.0)
        .await
        .context("Failed to prune sessions")?;
    info!("Pruned {pruned} expired sessions.");

    txn.commit().await?;

    Ok(())
}

async fn refresh_premium(config: Config) -> anyhow::Result<()> {
    let mut provider = Provider::from_config(&config).await?;

    let db: types::Database = provider.provide();
    let mut txn = db.begin_transaction().await?;

    let premium_repo: types::PremiumRepo = provider.provide();
    let premium: types::Premium = provider.provide();

    let user_ids = premium_repo.list_subscription_users(&mut txn).await?;
    for &user_id in &user_ids {
        premium.get_active(&mut txn, user_id).await?;
    }
    info!("Refreshed {} premium subscriptions", user_ids.len());

    txn.commit().await?;

    Ok(())
}

async fn daily_rewards_rebuild(
    config: Config,
    user: Uuid,
    date: Option<NaiveDate>,
) -> anyhow::Result<()> {
    let mut provider = Provider::from_config(&config).await?;

    let feature: types::DailyRewardFeature = provider.provide();

    let user_id = UserId::from(user);
    let target_date = date.unwrap_or_else(|| Utc::now().date_naive());

    let snapshot = feature
        .rebuild_snapshot(user_id, target_date)
        .await
        .context("Failed to rebuild daily rewards snapshot")?;

    let statuses: Vec<_> = snapshot
        .rewards
        .iter()
        .map(|reward| format!("{:?}:{:?}", reward.category, reward.status))
        .collect();

    info!(
        user_id = ?user_id,
        date = %target_date,
        available_coins = snapshot.claim_totals.available_coins,
        claimed_today = snapshot.claim_totals.claimed_today,
        rewards = ?statuses,
        "Rebuilt daily rewards snapshot"
    );

    Ok(())
}
