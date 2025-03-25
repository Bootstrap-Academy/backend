use std::{future::Future, sync::Arc, time::Duration};

use academy_cache_contracts::CacheService;
use academy_core_health_contracts::{HealthFeatureService, HealthStatus};
use academy_di::Build;
use academy_email_contracts::EmailService;
use academy_persistence_contracts::Database;
use academy_shared_contracts::time::TimeService;
use academy_utils::trace_instrument;
use chrono::{DateTime, TimeDelta, Utc};
use tokio::sync::RwLock;
use tracing::{error, trace};

#[derive(Debug, Clone, Build)]
pub struct HealthFeatureServiceImpl<Time, Db, Cache, Email> {
    time: Time,
    db: Db,
    cache: Cache,
    email: Email,
    config: HealthFeatureConfig,
    #[di(default)]
    state: Arc<State>,
}

#[derive(Debug, Clone)]
pub struct HealthFeatureConfig {
    pub database_cache_ttl: Duration,
    pub cache_cache_ttl: Duration,
    pub email_cache_ttl: Duration,
}

#[derive(Debug, Default)]
struct State {
    database_cache: RwLock<Option<CachedStatusItem>>,
    cache_cache: RwLock<Option<CachedStatusItem>>,
    email_cache: RwLock<Option<CachedStatusItem>>,
}

#[derive(Debug, Clone, Copy)]
struct CachedStatusItem {
    status: bool,
    timestamp: DateTime<Utc>,
}

impl<Time, Db, Cache, Email> HealthFeatureService
    for HealthFeatureServiceImpl<Time, Db, Cache, Email>
where
    Time: TimeService,
    Db: Database,
    Cache: CacheService,
    Email: EmailService,
{
    #[trace_instrument(skip(self))]
    async fn get_status(&self) -> HealthStatus {
        let database = self.ping_cached(
            "database",
            &self.state.database_cache,
            self.config.database_cache_ttl,
            || async {
                self.db
                    .ping()
                    .await
                    .inspect_err(|err| error!("Failed to ping database: {err:#}"))
                    .is_ok()
            },
        );

        let cache = self.ping_cached(
            "cache",
            &self.state.cache_cache,
            self.config.cache_cache_ttl,
            || async {
                self.cache
                    .ping()
                    .await
                    .inspect_err(|err| error!("Failed to ping cache: {err:#}"))
                    .is_ok()
            },
        );

        let email = self.ping_cached(
            "email",
            &self.state.email_cache,
            self.config.email_cache_ttl,
            || async {
                self.email
                    .ping()
                    .await
                    .inspect_err(|err| error!("Failed to ping smtp server: {err:#}"))
                    .is_ok()
            },
        );

        let (database, cache, email) = tokio::join!(database, cache, email);

        HealthStatus {
            database,
            cache,
            email,
        }
    }
}

impl<Time, Db, Cache, Email> HealthFeatureServiceImpl<Time, Db, Cache, Email>
where
    Time: TimeService,
{
    #[trace_instrument(skip(self, f))]
    async fn ping_cached<F>(
        &self,
        _item: &'static str,
        cache: &RwLock<Option<CachedStatusItem>>,
        ttl: Duration,
        f: impl FnOnce() -> F,
    ) -> bool
    where
        F: Future<Output = bool>,
    {
        let now = self.time.now();

        let status_if_not_expired = |cached: Option<CachedStatusItem>| {
            let CachedStatusItem { status, timestamp } = cached?;
            let ttl = timestamp + ttl - now;
            (ttl > TimeDelta::zero())
                .then_some(status)
                .inspect(|_| trace!(%ttl, "use cache"))
        };

        if let Some(status) = status_if_not_expired(*cache.read().await) {
            return status;
        }

        trace!("cache miss, acquire write lock");
        let mut cache_guard = cache.write().await;
        if let Some(status) = status_if_not_expired(*cache_guard) {
            return status;
        }

        trace!("ping");
        let status = f().await;

        *cache_guard = Some(CachedStatusItem {
            status,
            timestamp: now,
        });

        status
    }
}
