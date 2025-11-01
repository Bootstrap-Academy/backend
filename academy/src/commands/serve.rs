use academy_cache_contracts::CacheService;
use academy_config::Config;
use academy_di::Provide;
use academy_email_contracts::EmailService;
use academy_persistence_contracts::Database;
use academy_persistence_postgres::MigrationStatus;
use tracing::{info, warn};

use crate::{
    cache, database, email,
    environment::{
        ConfigProvider, Provider,
        types::{DailyRewardActivity, RestServer},
    },
};

pub async fn serve(config: Config) -> anyhow::Result<()> {
    info!("Connecting to database");
    let database = database::connect(&config.database).await?;
    database.ping().await?;

    if config.database.run_migrations {
        info!("Applying pending migrations");
        let mut applied = false;
        for name in database.run_migrations(None).await? {
            info!("Applied {name}");
            applied = true;
        }
        if !applied {
            info!("No migrations pending");
        }
    } else {
        info!("Checking for pending migrations");
        let pending = database
            .list_migrations()
            .await?
            .into_iter()
            .filter_map(|MigrationStatus { migration, applied }| (!applied).then_some(migration))
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            for migration in pending {
                warn!("Migration {} is pending", migration.name);
            }
            anyhow::bail!(
                "Some database migrations are pending. Run `academy migrate up` to apply them."
            );
        } else {
            info!("No migrations pending");
        }
    }

    info!("Connecting to valkey cache");
    let cache = cache::connect(&config.cache).await?;
    cache.ping().await?;

    info!("Connecting to smtp server");
    let email = email::connect(&config.email).await?;
    email.ping().await?;

    let config_provider = ConfigProvider::new(&config)?;
    let activity_configs = config_provider.daily_reward_activity_configs();
    let daily_reward_activity =
        DailyRewardActivity::new(activity_configs.skills, activity_configs.challenges).await?;

    let mut provider = Provider::new(
        config_provider,
        database,
        cache,
        email,
        daily_reward_activity,
    );

    let server: RestServer = provider.provide();
    server.serve().await
}
