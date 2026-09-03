use academy_core_oauth2_contracts::{
    OAuth2CreateSessionError, OAuth2CreateSessionResponse, OAuth2FeatureService,
    authorization::MockOAuth2AuthorizationService,
    login::{MockOAuth2LoginService, OAuth2LoginServiceError},
    registration::MockOAuth2RegistrationService,
};
use academy_core_session_contracts::session::MockSessionService;
use academy_demo::{
    oauth2::{FOO_OAUTH2_LINK_1, TEST_OAUTH2_PROVIDER_ID},
    session::FOO_1,
    user::FOO,
};
use academy_models::{
    auth::Login,
    oauth2::{OAuth2PendingAuthorization, OAuth2RegistrationToken},
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::{Apply, assert_matches};

use crate::{
    OAuth2FeatureServiceImpl, OAuth2Registration,
    tests::{STATE, Sut, callback, login, pending_authorization},
};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending_authorization()));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_oauth2_provider_id_and_remote_user_id(
            TEST_OAUTH2_PROVIDER_ID.clone(),
            FOO_OAUTH2_LINK_1.remote_user.id.clone(),
            Some(FOO.clone()),
        );

    let session = MockSessionService::new().with_create(FOO.clone(), None, true, expected.clone());

    let sut = OAuth2FeatureServiceImpl {
        db,
        oauth2_authorization,
        oauth2_login,
        user_repo,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(callback(), None).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        OAuth2CreateSessionResponse::Login(expected.into())
    );
}

#[tokio::test]
async fn not_linked() {
    // Arrange
    let expected = OAuth2RegistrationToken::try_new(
        "kvyhRRjn83JC223MwAbqhFTW09J8a75VIBMyLaxhiLtSl0Mddhyr7qctXcqKBINC",
    )
    .unwrap();

    let db = MockDatabase::build(false);

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending_authorization()));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_oauth2_provider_id_and_remote_user_id(
            TEST_OAUTH2_PROVIDER_ID.clone(),
            FOO_OAUTH2_LINK_1.remote_user.id.clone(),
            None,
        );

    let oauth2_registration = MockOAuth2RegistrationService::new().with_save(
        OAuth2Registration {
            provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
            remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
        },
        expected.clone(),
    );

    let sut = OAuth2FeatureServiceImpl {
        db,
        oauth2_authorization,
        oauth2_login,
        oauth2_registration,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(callback(), None).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        OAuth2CreateSessionResponse::RegistrationToken(expected)
    );
}

/// A callback whose `state` was never issued, has expired or has already been
/// redeemed is rejected: this is what stops an attacker from feeding their own
/// authorization code into someone else's browser.
#[tokio::test]
async fn invalid_state() {
    // Arrange
    let oauth2_authorization =
        MockOAuth2AuthorizationService::new().with_consume(STATE.try_into().unwrap(), None);

    let sut = OAuth2FeatureServiceImpl {
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(callback(), None).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateSessionError::InvalidState));
}

#[tokio::test]
async fn invalid_provider() {
    // Arrange
    let pending = OAuth2PendingAuthorization {
        provider_id: "invalid-provider".into(),
        ..pending_authorization()
    };
    let login = crate::OAuth2Login {
        provider_id: "invalid-provider".into(),
        ..login()
    };

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login, Err(OAuth2LoginServiceError::InvalidProvider));

    let sut = OAuth2FeatureServiceImpl {
        oauth2_authorization,
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(callback(), None).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateSessionError::InvalidProvider));
}

#[tokio::test]
async fn invalid_code() {
    // Arrange
    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending_authorization()));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login(), Err(OAuth2LoginServiceError::InvalidCode));

    let sut = OAuth2FeatureServiceImpl {
        oauth2_authorization,
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(callback(), None).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateSessionError::InvalidCode));
}

#[tokio::test]
async fn user_disabled() {
    // Arrange
    let db = MockDatabase::build(false);

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending_authorization()));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_oauth2_provider_id_and_remote_user_id(
            TEST_OAUTH2_PROVIDER_ID.clone(),
            FOO_OAUTH2_LINK_1.remote_user.id.clone(),
            Some(FOO.clone().with(|u| u.user.enabled = false)),
        );

    let sut = OAuth2FeatureServiceImpl {
        db,
        oauth2_authorization,
        oauth2_login,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(callback(), None).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateSessionError::UserDisabled));
}
