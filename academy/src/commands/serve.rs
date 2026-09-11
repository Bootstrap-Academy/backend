use academy_cache_contracts::CacheService;
use academy_config::{Config, MicroservicesConfig};
use academy_core_contract_contracts::ContractFeatureService;
use academy_core_paypal_contracts::PaypalFeatureService;
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
    if let Err(err) = email.ping().await {
        warn!(error=%err, "SMTP unavailable; durable payment receipts will retry");
    }

    let config_provider = ConfigProvider::new(&config)?;
    let mut provider = Provider::new(config_provider, database.clone(), cache, email);

    let deletion_services: crate::environment::types::MicroservicesApi = provider.provide();
    let deletion_recovery = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(err) = crate::deletions::retry(&database, &deletion_services).await {
                warn!(error=%err, "Account erasure recovery failed; durable work retained");
            }
        }
    });
    let contracts: crate::environment::types::ContractFeature = provider.provide();
    let contract_recovery = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(err) = contracts.retry_confirmations().await {
                warn!(error=%err,"Declaration confirmation retry failed; durable work retained");
            }
        }
    });
    let purchases: crate::environment::types::PurchaseFeature = provider.provide();
    let purchase_recovery = tokio::spawn(async move {
        use academy_core_purchase_contracts::PurchaseFeatureService;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = purchases.retry().await {
                warn!(%error,"Purchase confirmation recovery remains pending");
            }
        }
    });
    let moderation: crate::environment::types::ModerationFeature = provider.provide();
    let moderation_delivery = tokio::spawn(async move {
        use academy_core_moderation_contracts::ModerationFeatureService;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(error) = moderation.retry().await {
                warn!(%error,"Moderation delivery remains pending; owning statements retained");
            }
        }
    });
    let server: RestServer = provider.provide();
    let payments: crate::environment::types::PaypalFeature = provider.provide();
    let recovery = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(err) = payments.retry_payments().await {
                warn!(error=%err, "PayPal recovery unavailable; pending work retained");
            }
        }
    });
    let result = server.serve().await;
    moderation_delivery.abort();
    purchase_recovery.abort();
    contract_recovery.abort();
    recovery.abort();
    deletion_recovery.abort();
    result
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
