use academy_core_oauth2_contracts::{
    OAuth2BeginAuthorizationError, OAuth2FeatureService,
    authorization::{MockOAuth2AuthorizationService, OAuth2AuthorizationServiceError},
};
use academy_demo::oauth2::TEST_OAUTH2_PROVIDER_ID;
use academy_models::{oauth2::OAuth2AuthorizationUrl, url::Url};
use academy_utils::assert_matches;

use super::{STATE, Sut};
use crate::OAuth2FeatureServiceImpl;

#[tokio::test]
async fn ok() {
    // Arrange
    let redirect_uri: Url = "http://test/redirect".parse().unwrap();
    let expected = OAuth2AuthorizationUrl {
        state: STATE.try_into().unwrap(),
        authorize_url: "http://test/auth?state=x".parse().unwrap(),
    };

    let oauth2_authorization = MockOAuth2AuthorizationService::new().with_begin(
        TEST_OAUTH2_PROVIDER_ID.clone(),
        redirect_uri.clone(),
        Ok(expected.clone()),
    );

    let sut = OAuth2FeatureServiceImpl {
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut
        .begin_authorization(TEST_OAUTH2_PROVIDER_ID.clone(), redirect_uri)
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn invalid_provider() {
    // Arrange
    let redirect_uri: Url = "http://test/redirect".parse().unwrap();

    let oauth2_authorization = MockOAuth2AuthorizationService::new().with_begin(
        "invalid-provider".into(),
        redirect_uri.clone(),
        Err(OAuth2AuthorizationServiceError::InvalidProvider),
    );

    let sut = OAuth2FeatureServiceImpl {
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut
        .begin_authorization("invalid-provider".into(), redirect_uri)
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2BeginAuthorizationError::InvalidProvider));
}
