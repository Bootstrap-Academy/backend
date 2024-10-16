use std::{future::Future, time::Duration};

use serde::{de::DeserializeOwned, Serialize};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait CacheService: Sized + Send + Sync + 'static {
    /// Reads a cache item.
    fn get<T: DeserializeOwned + 'static>(
        &self,
        key: &str,
    ) -> impl Future<Output = anyhow::Result<Option<T>>> + Send;

    /// Creates a new or updates an existing cache item.
    ///
    /// If `ttl` is set, the item is automatically removed after this timeout.
    fn set<T: Serialize + Sync + 'static>(
        &self,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Removes an existing cache item.
    ///
    /// Does nothing if the cache item does not exist.
    fn remove(&self, key: &str) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Verify the connection to the cache.
    fn ping(&self) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl MockCacheService {
    pub fn with_get<T: DeserializeOwned + Send + 'static>(
        mut self,
        key: String,
        result: Option<T>,
    ) -> Self {
        self.expect_get()
            .once()
            .with(mockall::predicate::eq(key))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_set<T: std::fmt::Debug + PartialEq + Serialize + Send + Sync + 'static>(
        mut self,
        key: String,
        value: T,
        ttl: Option<Duration>,
    ) -> Self {
        self.expect_set()
            .once()
            .with(
                mockall::predicate::eq(key),
                mockall::predicate::eq(value),
                mockall::predicate::eq(ttl),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_remove(mut self, key: String) -> Self {
        self.expect_remove()
            .once()
            .with(mockall::predicate::eq(key))
            .return_once(|_| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
