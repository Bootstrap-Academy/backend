use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::{
    OAuth2CreateLinkError, OAuth2FeatureService,
    authorization::MockOAuth2AuthorizationService,
    link::{MockOAuth2LinkService, OAuth2LinkServiceError},
    login::{MockOAuth2LoginService, OAuth2LoginServiceError},
};
use academy_demo::{
    oauth2::{FOO_OAUTH2_LINK_1, TEST_OAUTH2_PROVIDER_ID},
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    oauth2::OAuth2PendingAuthorization,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{
    OAuth2FeatureServiceImpl,
    tests::{STATE, Sut, callback, login, pending_authorization},
};

#[tokio::test]
async fn ok() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending_authorization()));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let oauth2_create_link = MockOAuth2LinkService::new().with_create(
        FOO.user.id,
        TEST_OAUTH2_PROVIDER_ID.clone(),
        FOO_OAUTH2_LINK_1.remote_user.clone(),
        Ok(FOO_OAUTH2_LINK_1.clone()),
    );

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_authorization,
        oauth2_login,
        oauth2_create_link,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), UserIdOrSelf::Slf, callback())
        .await;

    // Assert
    assert_eq!(result.unwrap(), *FOO_OAUTH2_LINK_1);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = OAuth2FeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), FOO.user.id.into(), callback())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(OAuth2CreateLinkError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = OAuth2FeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), FOO.user.id.into(), callback())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(OAuth2CreateLinkError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, false);

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), FOO.user.id.into(), callback())
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::NotFound));
}

/// A `state` that was never issued, has expired or has already been redeemed
/// is rejected before the authorization code is exchanged.
#[tokio::test]
async fn invalid_state() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_authorization =
        MockOAuth2AuthorizationService::new().with_consume(STATE.try_into().unwrap(), None);

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_authorization,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), UserIdOrSelf::Slf, callback())
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::InvalidState));
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

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login, Err(OAuth2LoginServiceError::InvalidProvider));

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_authorization,
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), UserIdOrSelf::Slf, callback())
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::InvalidProvider));
}

#[tokio::test]
async fn invalid_code() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending_authorization()));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login(), Err(OAuth2LoginServiceError::InvalidCode));

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_authorization,
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), UserIdOrSelf::Slf, callback())
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::InvalidCode));
}

#[tokio::test]
async fn remote_already_linked() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_authorization = MockOAuth2AuthorizationService::new()
        .with_consume(STATE.try_into().unwrap(), Some(pending_authorization()));

    let oauth2_login = MockOAuth2LoginService::new()
        .with_login(login(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let oauth2_create_link = MockOAuth2LinkService::new().with_create(
        FOO.user.id,
        TEST_OAUTH2_PROVIDER_ID.clone(),
        FOO_OAUTH2_LINK_1.remote_user.clone(),
        Err(OAuth2LinkServiceError::RemoteAlreadyLinked),
    );

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_authorization,
        oauth2_login,
        oauth2_create_link,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_link(&"token".into(), UserIdOrSelf::Slf, callback())
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::RemoteAlreadyLinked));
}
