use academy_core_oauth2_contracts::login::{OAuth2LoginService, OAuth2LoginServiceError};
use academy_di::Build;
use academy_extern_contracts::oauth2::{OAuth2ApiService, OAuth2ResolveCodeError};
use academy_models::oauth2::{OAuth2Login, OAuth2UserInfo};

use crate::OAuth2FeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct OAuth2LoginServiceImpl<OAuth2Api> {
    oauth2_api: OAuth2Api,
    config: OAuth2FeatureConfig,
}

impl<OAuth2Api> OAuth2LoginService for OAuth2LoginServiceImpl<OAuth2Api>
where
    OAuth2Api: OAuth2ApiService,
{
    async fn login(&self, login: OAuth2Login) -> Result<OAuth2UserInfo, OAuth2LoginServiceError> {
        let provider = self
            .config
            .providers
            .get(&login.provider_id)
            .ok_or(OAuth2LoginServiceError::InvalidProvider)?;

        let user_info = self
            .oauth2_api
            .resolve_code(provider.clone(), login.code, login.redirect_uri)
            .await
            .map_err(|err| match err {
                OAuth2ResolveCodeError::InvalidCode => OAuth2LoginServiceError::InvalidCode,
                OAuth2ResolveCodeError::Other(err) => err.into(),
            })?;

        Ok(user_info)
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::oauth2::{FOO_OAUTH2_LINK_1, TEST_OAUTH2_PROVIDER, TEST_OAUTH2_PROVIDER_ID};
    use academy_extern_contracts::oauth2::MockOAuth2ApiService;
    use academy_utils::assert_matches;

    use super::*;

    type Sut = OAuth2LoginServiceImpl<MockOAuth2ApiService>;

    #[tokio::test]
    async fn ok() {
        // Arrange
        let login = OAuth2Login {
            provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
            code: "code".try_into().unwrap(),
            redirect_uri: "http://test/redirect".parse().unwrap(),
        };

        let oauth2_api = MockOAuth2ApiService::new().with_resolve_code(
            TEST_OAUTH2_PROVIDER.clone(),
            login.code.clone(),
            login.redirect_uri.clone(),
            Ok(FOO_OAUTH2_LINK_1.remote_user.clone()),
        );

        let sut = OAuth2LoginServiceImpl {
            oauth2_api,
            ..Sut::default()
        };

        // Act
        let result = sut.login(login).await;

        // Assert
        assert_eq!(result.unwrap(), FOO_OAUTH2_LINK_1.remote_user);
    }

    #[tokio::test]
    async fn invalid_provider() {
        // Arrange
        let login = OAuth2Login {
            provider_id: "invalid-provider".into(),
            code: "code".try_into().unwrap(),
            redirect_uri: "http://test/redirect".parse().unwrap(),
        };

        let sut = Sut::default();

        // Act
        let result = sut.login(login).await;

        // Assert
        assert_matches!(result, Err(OAuth2LoginServiceError::InvalidProvider));
    }

    #[tokio::test]
    async fn invalid_code() {
        // Arrange
        let login = OAuth2Login {
            provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
            code: "code".try_into().unwrap(),
            redirect_uri: "http://test/redirect".parse().unwrap(),
        };

        let oauth2_api = MockOAuth2ApiService::new().with_resolve_code(
            TEST_OAUTH2_PROVIDER.clone(),
            login.code.clone(),
            login.redirect_uri.clone(),
            Err(OAuth2ResolveCodeError::InvalidCode),
        );

        let sut = OAuth2LoginServiceImpl {
            oauth2_api,
            ..Sut::default()
        };

        // Act
        let result = sut.login(login).await;

        // Assert
        assert_matches!(result, Err(OAuth2LoginServiceError::InvalidCode));
    }
}
