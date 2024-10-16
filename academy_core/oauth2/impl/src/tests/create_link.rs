use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::{
    create_link::{MockOAuth2CreateLinkService, OAuth2CreateLinkServiceError},
    login::{MockOAuth2LoginService, OAuth2LoginServiceError},
    OAuth2CreateLinkError, OAuth2Service,
};
use academy_demo::{
    oauth2::{FOO_OAUTH2_LINK_1, TEST_OAUTH2_PROVIDER_ID},
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    oauth2::OAuth2Login,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::assert_matches;

use crate::{tests::Sut, OAuth2ServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_login = MockOAuth2LoginService::new()
        .with_invoke(login.clone(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let oauth2_create_link = MockOAuth2CreateLinkService::new().with_invoke(
        FOO.user.id,
        TEST_OAUTH2_PROVIDER_ID.clone(),
        FOO_OAUTH2_LINK_1.remote_user.clone(),
        Ok(FOO_OAUTH2_LINK_1.clone()),
    );

    let sut = OAuth2ServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_login,
        oauth2_create_link,
        ..Sut::default()
    };

    // Act
    let result = sut.create_link("token", UserIdOrSelf::Slf, login).await;

    // Assert
    assert_eq!(result.unwrap(), *FOO_OAUTH2_LINK_1);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(None);

    let sut = OAuth2ServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.create_link("token", FOO.user.id.into(), login).await;

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
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = OAuth2ServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.create_link("token", FOO.user.id.into(), login).await;

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
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, false);

    let sut = OAuth2ServiceImpl {
        db,
        auth,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_link("token", FOO.user.id.into(), login).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::NotFound));
}

#[tokio::test]
async fn invalid_provider() {
    // Arrange
    let login = OAuth2Login {
        provider_id: "invalid-provider".into(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_login = MockOAuth2LoginService::new()
        .with_invoke(login.clone(), Err(OAuth2LoginServiceError::InvalidProvider));

    let sut = OAuth2ServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut.create_link("token", UserIdOrSelf::Slf, login).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::InvalidProvider));
}

#[tokio::test]
async fn invalid_code() {
    // Arrange
    let login = OAuth2Login {
        provider_id: "invalid-provider".into(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_login = MockOAuth2LoginService::new()
        .with_invoke(login.clone(), Err(OAuth2LoginServiceError::InvalidCode));

    let sut = OAuth2ServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_login,
        ..Sut::default()
    };

    // Act
    let result = sut.create_link("token", UserIdOrSelf::Slf, login).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::InvalidCode));
}

#[tokio::test]
async fn remote_already_linked() {
    // Arrange
    let login = OAuth2Login {
        provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
        code: "code".try_into().unwrap(),
        redirect_uri: "http://test/redirect".parse().unwrap(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let oauth2_login = MockOAuth2LoginService::new()
        .with_invoke(login.clone(), Ok(FOO_OAUTH2_LINK_1.remote_user.clone()));

    let oauth2_create_link = MockOAuth2CreateLinkService::new().with_invoke(
        FOO.user.id,
        TEST_OAUTH2_PROVIDER_ID.clone(),
        FOO_OAUTH2_LINK_1.remote_user.clone(),
        Err(OAuth2CreateLinkServiceError::RemoteAlreadyLinked),
    );

    let sut = OAuth2ServiceImpl {
        db,
        auth,
        user_repo,
        oauth2_login,
        oauth2_create_link,
        ..Sut::default()
    };

    // Act
    let result = sut.create_link("token", UserIdOrSelf::Slf, login).await;

    // Assert
    assert_matches!(result, Err(OAuth2CreateLinkError::RemoteAlreadyLinked));
}
