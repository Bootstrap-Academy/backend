use academy_cache_contracts::CacheService;
use academy_core_oauth2_contracts::registration::OAuth2RegistrationService;
use academy_di::Build;
use academy_models::oauth2::{OAuth2Registration, OAuth2RegistrationToken};
use academy_shared_contracts::secret::SecretService;
use academy_utils::trace_instrument;
use anyhow::Context;

use crate::OAuth2FeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct OAuth2RegistrationServiceImpl<Secret, Cache> {
    secret: Secret,
    cache: Cache,
    config: OAuth2FeatureConfig,
}

impl<Secret, Cache> OAuth2RegistrationService for OAuth2RegistrationServiceImpl<Secret, Cache>
where
    Secret: SecretService,
    Cache: CacheService,
{
    #[trace_instrument(skip(self))]
    async fn save(
        &self,
        registration: &OAuth2Registration,
    ) -> anyhow::Result<OAuth2RegistrationToken> {
        let registration_token =
            OAuth2RegistrationToken::try_new(self.secret.generate(OAuth2RegistrationToken::LEN).0)
                .unwrap();

        self.cache
            .set(
                &oauth2_registration_cache_key(&registration_token),
                registration,
                Some(self.config.registration_token_ttl),
            )
            .await
            .context("Failed to save OAuth2 registration in cache")?;

        Ok(registration_token)
    }

    #[trace_instrument(skip(self))]
    async fn get(
        &self,
        registration_token: &OAuth2RegistrationToken,
    ) -> anyhow::Result<Option<OAuth2Registration>> {
        self.cache
            .get(&oauth2_registration_cache_key(registration_token))
            .await
            .context("Failed to get OAuth2 registration from cache")
    }

    #[trace_instrument(skip(self))]
    async fn consume(&self, registration_token: &OAuth2RegistrationToken) -> anyhow::Result<bool> {
        self.cache
            .pop::<OAuth2Registration>(&oauth2_registration_cache_key(registration_token))
            .await
            .context("Failed to remove OAuth2 registration from cache")
            .map(|registration| registration.is_some())
    }
}

fn oauth2_registration_cache_key(registration_token: &OAuth2RegistrationToken) -> String {
    format!("oauth2_registration:{}", **registration_token)
}

#[cfg(test)]
mod tests {
    use academy_cache_contracts::MockCacheService;
    use academy_demo::oauth2::FOO_OAUTH2_LINK_1;
    use academy_shared_contracts::secret::MockSecretService;

    use super::*;

    type Sut = OAuth2RegistrationServiceImpl<MockSecretService, MockCacheService>;

    #[tokio::test]
    async fn save() {
        // Arrange
        let config = OAuth2FeatureConfig::default();
        let expected = token();
        let registration = registration();

        let secret = MockSecretService::new()
            .with_generate(OAuth2RegistrationToken::LEN, expected.clone().into_inner());

        let cache = MockCacheService::new().with_set(
            format!("oauth2_registration:{}", *expected),
            registration.clone(),
            Some(config.registration_token_ttl),
        );

        let sut = OAuth2RegistrationServiceImpl {
            secret,
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.save(&registration).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn get_some() {
        // Arrange
        let expected = registration();
        let token = token();

        let cache = MockCacheService::new().with_get(
            format!("oauth2_registration:{}", *token),
            Some(expected.clone()),
        );

        let sut = OAuth2RegistrationServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.get(&token).await;

        // Assert
        assert_eq!(result.unwrap(), Some(expected));
    }

    #[tokio::test]
    async fn get_none() {
        // Arrange
        let token = token();

        let cache = MockCacheService::new().with_get(
            format!("oauth2_registration:{}", *token),
            None::<OAuth2Registration>,
        );

        let sut = OAuth2RegistrationServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.get(&token).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn consume() {
        // Arrange
        let token = token();

        let cache = MockCacheService::new().with_pop(
            format!("oauth2_registration:{}", *token),
            Some(registration()),
        );

        let sut = OAuth2RegistrationServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.consume(&token).await;

        // Assert
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn consume_already_used() {
        // Arrange
        let token = token();

        let cache = MockCacheService::new().with_pop(
            format!("oauth2_registration:{}", *token),
            None::<OAuth2Registration>,
        );

        let sut = OAuth2RegistrationServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.consume(&token).await;

        // Assert
        assert!(!result.unwrap());
    }

    fn token() -> OAuth2RegistrationToken {
        "PCtuwzD5Xo1zwOMyRzJ5jEAvSrxxSwSLy16nur0l8SwPia4jwOtkxvCEsborR2d4"
            .try_into()
            .unwrap()
    }

    fn registration() -> OAuth2Registration {
        OAuth2Registration {
            provider_id: "test-provider".into(),
            remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
        }
    }
}
