use academy_cache_contracts::CacheService;
use academy_core_oauth2_contracts::authorization::{
    OAuth2AuthorizationService, OAuth2AuthorizationServiceError,
};
use academy_di::Build;
use academy_extern_contracts::oauth2::OAuth2ApiService;
use academy_models::{
    oauth2::{
        OAuth2AuthorizationRequest, OAuth2AuthorizationUrl, OAuth2CodeVerifier,
        OAuth2PendingAuthorization, OAuth2ProviderId, OAuth2State,
    },
    url::Url,
};
use academy_shared_contracts::secret::SecretService;
use academy_utils::trace_instrument;
use anyhow::Context;

use crate::OAuth2FeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct OAuth2AuthorizationServiceImpl<Secret, Cache, OAuth2Api> {
    secret: Secret,
    cache: Cache,
    oauth2_api: OAuth2Api,
    config: OAuth2FeatureConfig,
}

impl<Secret, Cache, OAuth2Api> OAuth2AuthorizationService
    for OAuth2AuthorizationServiceImpl<Secret, Cache, OAuth2Api>
where
    Secret: SecretService,
    Cache: CacheService,
    OAuth2Api: OAuth2ApiService,
{
    #[trace_instrument(skip(self))]
    async fn begin(
        &self,
        provider_id: OAuth2ProviderId,
        redirect_uri: Url,
    ) -> Result<OAuth2AuthorizationUrl, OAuth2AuthorizationServiceError> {
        let provider = self
            .config
            .providers
            .get(&provider_id)
            .ok_or(OAuth2AuthorizationServiceError::InvalidProvider)?;

        let state = OAuth2State::try_new(self.secret.generate(OAuth2State::LEN).0).unwrap();
        let code_verifier = provider.pkce.then(|| {
            OAuth2CodeVerifier::try_new(self.secret.generate(OAuth2CodeVerifier::LEN).0).unwrap()
        });

        let request = OAuth2AuthorizationRequest {
            state,
            redirect_uri,
            code_verifier,
        };
        let authorize_url = self.oauth2_api.generate_auth_url(provider, &request);

        let OAuth2AuthorizationRequest {
            state,
            redirect_uri,
            code_verifier,
        } = request;

        self.cache
            .set(
                &oauth2_authorization_cache_key(&state),
                &OAuth2PendingAuthorization {
                    provider_id,
                    redirect_uri,
                    code_verifier,
                },
                Some(self.config.authorization_ttl),
            )
            .await
            .context("Failed to save OAuth2 authorization in cache")?;

        Ok(OAuth2AuthorizationUrl {
            state,
            authorize_url,
        })
    }

    #[trace_instrument(skip(self))]
    async fn consume(
        &self,
        state: &OAuth2State,
    ) -> anyhow::Result<Option<OAuth2PendingAuthorization>> {
        self.cache
            .pop(&oauth2_authorization_cache_key(state))
            .await
            .context("Failed to get OAuth2 authorization from cache")
    }
}

fn oauth2_authorization_cache_key(state: &OAuth2State) -> String {
    format!("oauth2_authorization:{}", **state)
}

#[cfg(test)]
mod tests {
    use academy_cache_contracts::MockCacheService;
    use academy_demo::oauth2::{TEST_OAUTH2_PROVIDER, TEST_OAUTH2_PROVIDER_ID};
    use academy_extern_contracts::oauth2::MockOAuth2ApiService;
    use academy_shared_contracts::secret::MockSecretService;
    use academy_utils::{Apply, assert_matches};

    use super::*;

    type Sut =
        OAuth2AuthorizationServiceImpl<MockSecretService, MockCacheService, MockOAuth2ApiService>;

    const STATE: &str = "vCOoUNBcvGwOJRLNSpxYlSHDNTRQSVROXbcpiWaBJcJLtcMBpMVvcMEcjNXaYPtb";
    const CODE_VERIFIER: &str = "OUwNdaqXHYTHTOSevzsGTuOOhpUZPTGGvcxSbNZWMcOJVoALLNMFwFtdrjEqRHtF";

    #[tokio::test]
    async fn begin_with_pkce() {
        // Arrange
        let config = OAuth2FeatureConfig::default();
        let redirect_uri: Url = "http://test/oauth/callback".parse().unwrap();

        let secret = MockSecretService::new()
            .with_generate(OAuth2State::LEN, STATE.into())
            .with_generate(OAuth2CodeVerifier::LEN, CODE_VERIFIER.into());

        let oauth2_api = MockOAuth2ApiService::new().with_generate_auth_url(
            TEST_OAUTH2_PROVIDER.clone(),
            OAuth2AuthorizationRequest {
                state: STATE.try_into().unwrap(),
                redirect_uri: redirect_uri.clone(),
                code_verifier: Some(CODE_VERIFIER.try_into().unwrap()),
            },
            "http://test/auth?state=x".parse().unwrap(),
        );

        let cache = MockCacheService::new().with_set(
            format!("oauth2_authorization:{STATE}"),
            OAuth2PendingAuthorization {
                provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
                redirect_uri: redirect_uri.clone(),
                code_verifier: Some(CODE_VERIFIER.try_into().unwrap()),
            },
            Some(config.authorization_ttl),
        );

        let sut = OAuth2AuthorizationServiceImpl {
            secret,
            cache,
            oauth2_api,
            ..Sut::default()
        };

        // Act
        let result = sut
            .begin(TEST_OAUTH2_PROVIDER_ID.clone(), redirect_uri)
            .await;

        // Assert
        assert_eq!(
            result.unwrap(),
            OAuth2AuthorizationUrl {
                state: STATE.try_into().unwrap(),
                authorize_url: "http://test/auth?state=x".parse().unwrap(),
            }
        );
    }

    #[tokio::test]
    async fn begin_without_pkce() {
        // Arrange
        let config = OAuth2FeatureConfig::default();
        let redirect_uri: Url = "http://test/oauth/callback".parse().unwrap();
        let provider = TEST_OAUTH2_PROVIDER.clone().with(|p| p.pkce = false);

        let secret = MockSecretService::new().with_generate(OAuth2State::LEN, STATE.into());

        let oauth2_api = MockOAuth2ApiService::new().with_generate_auth_url(
            provider.clone(),
            OAuth2AuthorizationRequest {
                state: STATE.try_into().unwrap(),
                redirect_uri: redirect_uri.clone(),
                code_verifier: None,
            },
            "http://test/auth?state=x".parse().unwrap(),
        );

        let cache = MockCacheService::new().with_set(
            format!("oauth2_authorization:{STATE}"),
            OAuth2PendingAuthorization {
                provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
                redirect_uri: redirect_uri.clone(),
                code_verifier: None,
            },
            Some(config.authorization_ttl),
        );

        let sut = OAuth2AuthorizationServiceImpl {
            secret,
            cache,
            oauth2_api,
            config: OAuth2FeatureConfig {
                providers: std::collections::HashMap::from([(
                    TEST_OAUTH2_PROVIDER_ID.clone(),
                    provider,
                )])
                .into(),
                ..OAuth2FeatureConfig::default()
            },
        };

        // Act
        let result = sut
            .begin(TEST_OAUTH2_PROVIDER_ID.clone(), redirect_uri)
            .await;

        // Assert
        assert_eq!(
            result.unwrap(),
            OAuth2AuthorizationUrl {
                state: STATE.try_into().unwrap(),
                authorize_url: "http://test/auth?state=x".parse().unwrap(),
            }
        );
    }

    #[tokio::test]
    async fn begin_invalid_provider() {
        // Arrange
        let sut = Sut::default();

        // Act
        let result = sut
            .begin(
                "invalid-provider".into(),
                "http://test/oauth/callback".parse().unwrap(),
            )
            .await;

        // Assert
        assert_matches!(
            result,
            Err(OAuth2AuthorizationServiceError::InvalidProvider)
        );
    }

    #[tokio::test]
    async fn consume_some() {
        // Arrange
        let expected = OAuth2PendingAuthorization {
            provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
            redirect_uri: "http://test/oauth/callback".parse().unwrap(),
            code_verifier: Some(CODE_VERIFIER.try_into().unwrap()),
        };

        let cache = MockCacheService::new().with_pop(
            format!("oauth2_authorization:{STATE}"),
            Some(expected.clone()),
        );

        let sut = OAuth2AuthorizationServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.consume(&STATE.try_into().unwrap()).await;

        // Assert
        assert_eq!(result.unwrap(), Some(expected));
    }

    #[tokio::test]
    async fn consume_none() {
        // Arrange
        let cache = MockCacheService::new().with_pop(
            format!("oauth2_authorization:{STATE}"),
            None::<OAuth2PendingAuthorization>,
        );

        let sut = OAuth2AuthorizationServiceImpl {
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.consume(&STATE.try_into().unwrap()).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }
}
