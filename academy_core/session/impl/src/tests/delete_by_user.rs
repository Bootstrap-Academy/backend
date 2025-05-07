use academy_auth_contracts::MockAuthService;
use academy_core_session_contracts::{
    SessionDeleteByUserError, SessionFeatureService, session::MockSessionService,
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::MockDatabase;
use academy_utils::assert_matches;

use super::Sut;
use crate::SessionFeatureServiceImpl;

#[tokio::test]
async fn ok_self() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let session = MockSessionService::new().with_delete_by_user(FOO.user.id);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_by_user(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn ok_admin() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let session = MockSessionService::new().with_delete_by_user(FOO.user.id);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_by_user(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = SessionFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_by_user(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    assert_matches!(
        result,
        Err(SessionDeleteByUserError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = SessionFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_by_user(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    assert_matches!(
        result,
        Err(SessionDeleteByUserError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}
