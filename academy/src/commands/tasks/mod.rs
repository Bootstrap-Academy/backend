use academy_config::Config;
use academy_core_premium_contracts::premium::PremiumService;
use academy_di::Provide;
use academy_persistence_contracts::{
    Database, Transaction, premium::PremiumRepository, session::SessionRepository,
};
use academy_persistence_postgres::session::PostgresSessionRepository;
use anyhow::Context;
use chrono::Utc;
use clap::Subcommand;
use tracing::info;

use crate::{
    cache, database, email,
    environment::{ConfigProvider, Provider, types},
};

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Remove expired records from the database.
    PruneDatabase,
    /// Refresh premium subscriptions.
    RefreshPremium,
}

impl TaskCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            TaskCommand::PruneDatabase => prune_database(config).await,
            TaskCommand::RefreshPremium => refresh_premium(config).await,
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
    let database = database::connect(&config.database).await?;
    let cache = cache::connect(&config.cache).await?;
    let email_service = email::connect(&config.email).await?;
    let config_provider = ConfigProvider::new(&config)?;
    let mut provider = Provider::new(config_provider, database, cache, email_service);

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
