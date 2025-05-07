use academy_auth_contracts::MockAuthService;
use academy_core_oauth2_contracts::{OAuth2DeleteLinkError, OAuth2FeatureService};
use academy_demo::{
    oauth2::FOO_OAUTH2_LINK_1,
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::auth::{AuthError, AuthenticateError, AuthorizeError};
use academy_persistence_contracts::{
    MockDatabase, oauth2::MockOAuth2Repository, user::MockUserRepository,
};
use academy_utils::{Apply, assert_matches};

use crate::{OAuth2FeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let oauth2_repo = MockOAuth2Repository::new()
        .with_get_link(FOO_OAUTH2_LINK_1.id, Some(FOO_OAUTH2_LINK_1.clone()))
        .with_delete_link(FOO_OAUTH2_LINK_1.id, true);

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(FOO.clone().with(|u| u.details.oauth2_login = false)),
    );

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        oauth2_repo,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_link(&"token".into(), FOO.user.id.into(), FOO_OAUTH2_LINK_1.id)
        .await;

    // Assert
    result.unwrap();
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
        .delete_link(&"token".into(), FOO.user.id.into(), FOO_OAUTH2_LINK_1.id)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(OAuth2DeleteLinkError::Auth(AuthError::Authenticate(
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
        .delete_link(&"token".into(), FOO.user.id.into(), FOO_OAUTH2_LINK_1.id)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(OAuth2DeleteLinkError::Auth(AuthError::Authorize(
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

    let oauth2_repo = MockOAuth2Repository::new().with_get_link(FOO_OAUTH2_LINK_1.id, None);

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        oauth2_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_link(&"token".into(), FOO.user.id.into(), FOO_OAUTH2_LINK_1.id)
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2DeleteLinkError::NotFound));
}

#[tokio::test]
async fn user_id_mismatch() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let oauth2_repo = MockOAuth2Repository::new()
        .with_get_link(FOO_OAUTH2_LINK_1.id, Some(FOO_OAUTH2_LINK_1.clone()));

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        oauth2_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_link(&"token".into(), BAR.user.id.into(), FOO_OAUTH2_LINK_1.id)
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2DeleteLinkError::NotFound));
}

#[tokio::test]
async fn last_login_method() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build_expect_rollback();

    let oauth2_repo = MockOAuth2Repository::new()
        .with_get_link(FOO_OAUTH2_LINK_1.id, Some(FOO_OAUTH2_LINK_1.clone()))
        .with_delete_link(FOO_OAUTH2_LINK_1.id, true);

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(FOO.clone().with(|u| {
            u.details.password_login = false;
            u.details.oauth2_login = false;
        })),
    );

    let sut = OAuth2FeatureServiceImpl {
        db,
        auth,
        oauth2_repo,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_link(&"token".into(), FOO.user.id.into(), FOO_OAUTH2_LINK_1.id)
        .await;

    // Assert
    assert_matches!(result, Err(OAuth2DeleteLinkError::CannotRemoveLink));
}
