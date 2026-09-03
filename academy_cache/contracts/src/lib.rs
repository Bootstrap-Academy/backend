use std::{fmt::Debug, future::Future, time::Duration};

use serde::{Serialize, de::DeserializeOwned};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait CacheService: Sized + Send + Sync + 'static {
    /// Read a cache item.
    fn get<T: DeserializeOwned + Debug + 'static>(
        &self,
        key: &str,
    ) -> impl Future<Output = anyhow::Result<Option<T>>> + Send;

    /// Create a new or update an existing cache item.
    ///
    /// If `ttl` is set, the item is automatically removed after this timeout.
    fn set<T: Serialize + Debug + Sync + 'static>(
        &self,
        key: &str,
        value: &T,
        ttl: Option<Duration>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Read a cache item and remove it in the same operation.
    ///
    /// Returns `None` if the cache item does not exist. Because reading and
    /// removing happen atomically, this can be used to consume single use
    /// secrets: exactly one of two concurrent callers gets the value.
    fn pop<T: DeserializeOwned + Debug + 'static>(
        &self,
        key: &str,
    ) -> impl Future<Output = anyhow::Result<Option<T>>> + Send;

    /// Remove an existing cache item.
    ///
    /// Does nothing if the cache item does not exist.
    fn remove(&self, key: &str) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Verify the connection to the cache.
    fn ping(&self) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl MockCacheService {
    pub fn with_get<T: DeserializeOwned + Debug + Send + 'static>(
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

    pub fn with_set<T: Debug + PartialEq + Serialize + Send + Sync + 'static>(
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

    pub fn with_pop<T: DeserializeOwned + Debug + Send + 'static>(
        mut self,
        key: String,
        result: Option<T>,
    ) -> Self {
        self.expect_pop()
            .once()
            .with(mockall::predicate::eq(key))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
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
