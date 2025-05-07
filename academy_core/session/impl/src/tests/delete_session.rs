use academy_auth_contracts::MockAuthService;
use academy_core_session_contracts::{
    SessionDeleteError, SessionFeatureService, session::MockSessionService,
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1, FOO_2},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{MockDatabase, session::MockSessionRepository};
use academy_utils::assert_matches;

use crate::{SessionFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok_current() {
    // Arrange
    let db = MockDatabase::build(true);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let session_repo = MockSessionRepository::new().with_get(FOO_1.id, Some(FOO_1.clone()));

    let session = MockSessionService::new().with_delete(FOO_1.id, true);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session_repo,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_session(&"token".into(), UserIdOrSelf::Slf, FOO_1.id)
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn ok_self() {
    // Arrange
    let db = MockDatabase::build(true);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let session_repo = MockSessionRepository::new().with_get(FOO_2.id, Some(FOO_2.clone()));

    let session = MockSessionService::new().with_delete(FOO_2.id, true);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session_repo,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_session(&"token".into(), UserIdOrSelf::Slf, FOO_2.id)
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn ok_admin() {
    // Arrange
    let db = MockDatabase::build(true);

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let session_repo = MockSessionRepository::new().with_get(FOO_2.id, Some(FOO_2.clone()));

    let session = MockSessionService::new().with_delete(FOO_2.id, true);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session_repo,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_session(&"token".into(), UserIdOrSelf::UserId(FOO.user.id), FOO_2.id)
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
        .delete_session(&"token".into(), UserIdOrSelf::UserId(FOO.user.id), FOO_1.id)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(SessionDeleteError::Auth(AuthError::Authenticate(
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
        .delete_session(&"token".into(), UserIdOrSelf::UserId(FOO.user.id), FOO_1.id)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(SessionDeleteError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let db = MockDatabase::build(false);

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let session_repo = MockSessionRepository::new().with_get(FOO_1.id, None);

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_session(&"token".into(), UserIdOrSelf::UserId(FOO.user.id), FOO_1.id)
        .await;

    // Assert
    assert_matches!(result, Err(SessionDeleteError::NotFound));
}

#[tokio::test]
async fn different_user() {
    // Arrange
    let db = MockDatabase::build(false);

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let session_repo = MockSessionRepository::new().with_get(FOO_1.id, Some(FOO_1.clone()));

    let sut = SessionFeatureServiceImpl {
        db,
        auth,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .delete_session(&"token".into(), UserIdOrSelf::UserId(BAR.user.id), FOO_1.id)
        .await;

    // Assert
    assert_matches!(result, Err(SessionDeleteError::NotFound));
}
