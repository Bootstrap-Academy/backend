use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::{
    OAuth2BeginAuthorizationError, OAuth2FeatureService,
    authorization::{MockOAuth2AuthorizationService, OAuth2AuthorizationServiceError},
};
use academy_demo::{
    oauth2::TEST_OAUTH2_PROVIDER_ID,
    session::FOO_1,
    user::{ADMIN, FOO},
};
use academy_models::{oauth2::OAuth2AuthorizationUrl, url::Url};
use academy_utils::assert_matches;

use super::{STATE, Sut};
use crate::OAuth2FeatureServiceImpl;

/// A flow started without a token is anonymous and can only be redeemed as a
/// login.
#[tokio::test]
async fn ok_without_a_token() {
    // Arrange
    let redirect_uri: Url = "http://test/redirect".parse().unwrap();
    let expected = OAuth2AuthorizationUrl {
        state: STATE.try_into().unwrap(),
        authorize_url: "http://test/auth?state=x".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(None);

    let oauth2_authorization = MockOAuth2AuthorizationService::new().with_begin(
        TEST_OAUTH2_PROVIDER_ID.clone(),
        redirect_uri.clone(),
        None,
        Ok(expected.clone()),
    );

    let sut = OAuth2FeatureServiceImpl {
        auth,
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut
        .begin_authorization(
            &"token".into(),
            TEST_OAUTH2_PROVIDER_ID.clone(),
            redirect_uri,
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

/// A flow started while signed in belongs to that account and can only be
/// redeemed as a link for it.
#[tokio::test]
async fn ok_with_a_token() {
    // Arrange
    let redirect_uri: Url = "http://test/redirect".parse().unwrap();
    let expected = OAuth2AuthorizationUrl {
        state: STATE.try_into().unwrap(),
        authorize_url: "http://test/auth?state=x".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let oauth2_authorization = MockOAuth2AuthorizationService::new().with_begin(
        TEST_OAUTH2_PROVIDER_ID.clone(),
        redirect_uri.clone(),
        Some(FOO.user.id),
        Ok(expected.clone()),
    );

    let sut = OAuth2FeatureServiceImpl {
        auth,
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut
        .begin_authorization(
            &"token".into(),
            TEST_OAUTH2_PROVIDER_ID.clone(),
            redirect_uri,
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
    assert_ne!(ADMIN.user.id, FOO.user.id);
}

#[tokio::test]
async fn invalid_provider() {
    // Arrange
    let redirect_uri: Url = "http://test/redirect".parse().unwrap();

    let auth = MockAuthService::new().with_authenticate(None);

    let oauth2_authorization = MockOAuth2AuthorizationService::new().with_begin(
        "invalid-provider".into(),
        redirect_uri.clone(),
        None,
        Err(OAuth2AuthorizationServiceError::InvalidProvider),
    );

    let sut = OAuth2FeatureServiceImpl {
        auth,
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut
        .begin_authorization(&"token".into(), "invalid-provider".into(), redirect_uri)
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2BeginAuthorizationError::InvalidProvider));
}

#[tokio::test]
async fn invalid_redirect_uri() {
    // Arrange
    let redirect_uri: Url = "https://attacker.example/redirect".parse().unwrap();

    let auth = MockAuthService::new().with_authenticate(None);

    let oauth2_authorization = MockOAuth2AuthorizationService::new().with_begin(
        TEST_OAUTH2_PROVIDER_ID.clone(),
        redirect_uri.clone(),
        None,
        Err(OAuth2AuthorizationServiceError::InvalidRedirectUri),
    );

    let sut = OAuth2FeatureServiceImpl {
        auth,
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut
        .begin_authorization(
            &"token".into(),
            TEST_OAUTH2_PROVIDER_ID.clone(),
            redirect_uri,
        )
        .await;

    // Assert
    assert_matches!(
        result,
        Err(OAuth2BeginAuthorizationError::InvalidRedirectUri)
    );
}
