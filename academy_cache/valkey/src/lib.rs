use std::{fmt::Debug, time::Duration};

use academy_cache_contracts::CacheService;
use academy_utils::trace_instrument;
use anyhow::Context;
use bb8_redis::{
    RedisConnectionManager,
    bb8::Pool,
    redis::{self, AsyncCommands},
};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, Clone)]
pub struct ValkeyCache {
    pool: Pool<RedisConnectionManager>,
}

#[derive(Debug)]
pub struct ValkeyCacheConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
}

impl ValkeyCache {
    pub async fn connect(config: &ValkeyCacheConfig) -> anyhow::Result<Self> {
        let manager = RedisConnectionManager::new(config.url.as_str())?;
        let pool = Pool::builder()
            .max_size(config.max_connections)
            .min_idle(config.min_connections)
            .connection_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .build(manager)
            .await?;

        Ok(Self { pool })
    }

    #[cfg(feature = "dummy")]
    pub async fn dummy() -> Self {
        let manager = RedisConnectionManager::new("redis://dummy").unwrap();
        Self {
            pool: Pool::builder().build_unchecked(manager),
        }
    }

    pub async fn clear(&self) -> anyhow::Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire cache connection")?;
        redis::cmd("FLUSHDB")
            .exec_async(&mut *conn)
            .await
            .context("Failed to execute FLUSHDB command")
    }
}

impl CacheService for ValkeyCache {
    #[trace_instrument(skip(self))]
    async fn get<T: DeserializeOwned + Debug + 'static>(
        &self,
        key: &str,
    ) -> anyhow::Result<Option<T>> {
        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire cache connection")?;

        let result = conn
            .get::<_, Option<Vec<u8>>>(key)
            .await
            .context("Failed to read value from cache")?;

        result
            .map(|data| rmp_serde::from_slice(&data))
            .transpose()
            .context("Failed to deserialize cached value")
    }

    #[trace_instrument(skip(self))]
    async fn set<T: Serialize + Debug + Sync + 'static>(
        &self,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
    ) -> anyhow::Result<()> {
        let value = rmp_serde::to_vec(&value).context("Failed to serialize value")?;

        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire cache connection")?;

        if let Some(ttl) = ttl {
            conn.pset_ex(key, value, ttl.as_millis().try_into()?).await
        } else {
            conn.set(key, value).await
        }
        .context("Failed to write value to cache")
    }

    #[trace_instrument(skip(self))]
    async fn pop<T: DeserializeOwned + Debug + 'static>(
        &self,
        key: &str,
    ) -> anyhow::Result<Option<T>> {
        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire cache connection")?;

        let result = conn
            .get_del::<_, Option<Vec<u8>>>(key)
            .await
            .context("Failed to read and remove value from cache")?;

        result
            .map(|data| rmp_serde::from_slice(&data))
            .transpose()
            .context("Failed to deserialize cached value")
    }

    #[trace_instrument(skip(self))]
    async fn remove(&self, key: &str) -> anyhow::Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire cache connection")?;

        conn.del(key)
            .await
            .context("Failed to remove item from cache")
    }

    #[trace_instrument(skip(self))]
    async fn ping(&self) -> anyhow::Result<()> {
        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire cache connection")?;

        redis::cmd("PING")
            .exec_async(&mut *conn)
            .await
            .context("Failed to ping cache")
    }
}
