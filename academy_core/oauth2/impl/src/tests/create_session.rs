use academy_core_oauth2_contracts::{
    OAuth2CreateSessionError, OAuth2CreateSessionResponse, OAuth2FeatureService,
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
    oauth2::{OAuth2Login, OAuth2RegistrationToken},
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::{Apply, assert_matches};

use crate::{OAuth2FeatureServiceImpl, OAuth2Registration, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };
    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login.clone(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_oauth2_provider_id_and_remote_user_id(
            login.provider_id.clone(),
            FOO_OAUTH2_LINK_1.remote_user.id.clone(),
            Some(FOO.clone()),
        );

    let session = MockSessionService::new().with_create(FOO.clone(), None, true, expected.clone());

    let sut = OAuth2FeatureServiceImpl {
        db,
        oauth2_login,
        user_repo,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(login, None).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        OAuth2CreateSessionResponse::Login(expected.into())
    );
}

#[tokio::test]
async fn not_linked() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };
    let expected = OAuth2RegistrationToken::try_new(
        "kvyhRRjn83JC223MwAbqhFTW09J8a75VIBMyLaxhiLtSl0Mddhyr7qctXcqKBINC",
    )
    .unwrap();

    let db = MockDatabase::build(false);

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login.clone(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_oauth2_provider_id_and_remote_user_id(
            login.provider_id.clone(),
            FOO_OAUTH2_LINK_1.remote_user.id.clone(),
            None,
        );

    let oauth2_registration = MockOAuth2RegistrationService::new().with_save(
        OAuth2Registration {
            provider_id: login.provider_id.clone(),
            remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
        },
        expected.clone(),
    );

    let sut = OAuth2FeatureServiceImpl {
        db,
        oauth2_login,
        oauth2_registration,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(login, None).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        OAuth2CreateSessionResponse::RegistrationToken(expected)
    );
}

#[tokio::test]
async fn invalid_provider() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login.clone(), Err(OAuth2LoginServiceError::InvalidProvider));

    let sut = OAuth2FeatureServiceImpl {
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(login, None).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateSessionError::InvalidProvider));
}

#[tokio::test]
async fn invalid_code() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login.clone(), Err(OAuth2LoginServiceError::InvalidCode));

    let sut = OAuth2FeatureServiceImpl {
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(login, None).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateSessionError::InvalidCode));
}

#[tokio::test]
async fn user_disabled() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let db = MockDatabase::build(false);

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login.clone(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_oauth2_provider_id_and_remote_user_id(
            login.provider_id.clone(),
            FOO_OAUTH2_LINK_1.remote_user.id.clone(),
            Some(FOO.clone().with(|u| u.user.enabled = false)),
        );

    let sut = OAuth2FeatureServiceImpl {
        db,
        oauth2_login,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(login, None).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateSessionError::UserDisabled));
}
