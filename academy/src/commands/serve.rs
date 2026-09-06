use academy_cache_contracts::CacheService;
use academy_config::{Config, MicroservicesConfig};
use academy_di::Provide;
use academy_email_contracts::EmailService;
use academy_persistence_contracts::Database;
use academy_persistence_postgres::MigrationStatus;
use tracing::{info, warn};

use crate::{
    cache, database, email,
    environment::{ConfigProvider, Provider, types::RestServer},
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

    log_configured_microservices(&config.microservices);

    info!("Connecting to valkey cache");
    let cache = cache::connect(&config.cache).await?;
    cache.ping().await?;

    info!("Connecting to smtp server");
    let email = email::connect(&config.email).await?;
    email.ping().await?;

    let config_provider = ConfigProvider::new(&config)?;
    let mut provider = Provider::new(config_provider, database, cache, email);

    let server: RestServer = provider.provide();
    server.serve().await
}

/// Report which microservices this deployment talks to.
///
/// A microservice without a base url is not part of the deployment: account
/// deletions are not propagated to it, and its share of a data export is left
/// out without the export reporting anything as missing. That is intentional,
/// but it is invisible from the outside, so it is said once at startup.
fn log_configured_microservices(config: &MicroservicesConfig) {
    let services = [
        ("skills", config.skills_url.is_some()),
        ("challenges", config.challenges_url.is_some()),
        ("events", config.events_url.is_some()),
    ];

    let configured = names(&services, true);
    let missing = names(&services, false);

    if configured.is_empty() {
        warn!(
            "No microservice is configured. Account deletions are not propagated anywhere and a \
             data export only contains the data of the backend itself."
        );
    } else {
        info!(
            "Microservices configured for account deletion and data export: {}",
            configured.join(", ")
        );
    }

    if !missing.is_empty() {
        warn!(
            "No url configured for {}. Account deletions are not propagated to them and their \
             data is left out of every export without being reported as missing.",
            missing.join(", ")
        );
    }
}

fn names(services: &[(&'static str, bool)], configured: bool) -> Vec<&'static str> {
    services
        .iter()
        .filter(|(_, is_configured)| *is_configured == configured)
        .map(|(name, _)| *name)
        .collect()
}
