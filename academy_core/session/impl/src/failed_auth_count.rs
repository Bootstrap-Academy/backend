use academy_cache_contracts::CacheService;
use academy_core_session_contracts::failed_auth_count::SessionFailedAuthCountService;
use academy_di::Build;
use academy_models::user::UserNameOrEmailAddress;
use academy_shared_contracts::hash::HashService;
use academy_utils::trace_instrument;
use anyhow::Context;

#[derive(Debug, Clone, Build)]
pub struct SessionFailedAuthCountServiceImpl<Hash, Cache> {
    hash: Hash,
    cache: Cache,
}

impl<Hash, Cache> SessionFailedAuthCountService for SessionFailedAuthCountServiceImpl<Hash, Cache>
where
    Hash: HashService,
    Cache: CacheService,
{
    #[trace_instrument(skip(self))]
    async fn get(&self, name_or_email: &UserNameOrEmailAddress) -> anyhow::Result<u64> {
        self.cache
            .get(&self.cache_key(name_or_email))
            .await
            .map(|x| x.unwrap_or(0))
            .context("Failed to get failed auth count from cache")
    }

    #[trace_instrument(skip(self))]
    async fn increment(&self, name_or_email: &UserNameOrEmailAddress) -> anyhow::Result<()> {
        let cache_key = self.cache_key(name_or_email);

        let count = self
            .cache
            .get(&cache_key)
            .await
            .context("Failed to get failed auth count from cache")?
            .unwrap_or(0u64);

        self.cache
            .set(&cache_key, &(count + 1), None)
            .await
            .context("Failed to save failed auth count in cache")
    }

    #[trace_instrument(skip(self))]
    async fn reset(&self, name_or_email: &UserNameOrEmailAddress) -> anyhow::Result<()> {
        self.cache
            .remove(&self.cache_key(name_or_email))
            .await
            .context("Failed to reset failed auth count in cache")
    }
}

impl<Hash, Cache> SessionFailedAuthCountServiceImpl<Hash, Cache>
where
    Hash: HashService,
{
    fn cache_key(&self, name_or_email: &UserNameOrEmailAddress) -> String {
        let hash = self.hash.sha256(
            &match name_or_email {
                UserNameOrEmailAddress::Name(name) => name,
                UserNameOrEmailAddress::Email(email) => email.as_str(),
            }
            .to_lowercase(),
        );
        format!("failed_auth_attempts:{}", hex::encode(hash.0))
    }
}

#[cfg(test)]
mod tests {
    use academy_cache_contracts::MockCacheService;
    use academy_demo::{SHA256HASH1, SHA256HASH1_HEX, user::FOO};
    use academy_shared_contracts::hash::MockHashService;

    use super::*;

    #[tokio::test]
    async fn get() {
        // Arrange
        let hash =
            MockHashService::new().with_sha256(FOO.user.name.clone().into_inner(), *SHA256HASH1);

        let cache = MockCacheService::new().with_get(
            format!("failed_auth_attempts:{SHA256HASH1_HEX}"),
            Some(3u64),
        );

        let sut = SessionFailedAuthCountServiceImpl { hash, cache };

        // Act
        let result = sut
            .get(&UserNameOrEmailAddress::Name(FOO.user.name.clone()))
            .await;

        // Assert
        assert_eq!(result.unwrap(), 3);
    }

    #[tokio::test]
    async fn increment() {
        // Arrange
        let hash = MockHashService::new().with_sha256(
            FOO.user.email.as_ref().unwrap().as_str().to_owned(),
            *SHA256HASH1,
        );

        let cache_key = format!("failed_auth_attempts:{SHA256HASH1_HEX}");
        let cache = MockCacheService::new()
            .with_get(cache_key.clone(), Some(3u64))
            .with_set(cache_key, 4u64, None);

        let sut = SessionFailedAuthCountServiceImpl { hash, cache };

        // Act
        let result = sut
            .increment(&UserNameOrEmailAddress::Email(
                FOO.user.email.clone().unwrap(),
            ))
            .await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn reset() {
        // Arrange
        let hash =
            MockHashService::new().with_sha256(FOO.user.name.clone().into_inner(), *SHA256HASH1);

        let cache =
            MockCacheService::new().with_remove(format!("failed_auth_attempts:{SHA256HASH1_HEX}"));

        let sut = SessionFailedAuthCountServiceImpl { hash, cache };

        // Act
        let result = sut
            .reset(&UserNameOrEmailAddress::Name(FOO.user.name.clone()))
            .await;

        // Assert
        result.unwrap();
    }
}
