use academy_auth_contracts::MockAuthService;
use academy_core_session_contracts::{SessionFeatureService, SessionListByUserError};
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
async fn ok_self() {
    // Arrange
    let expected = vec![FOO_1.clone(), FOO_2.clone()];

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let session_repo =
        MockSessionRepository::new().with_list_by_user(FOO.user.id, expected.clone());

    let sut = SessionFeatureServiceImpl {
        auth,
        db,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.list_by_user(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn ok_admin() {
    // Arrange
    let expected = vec![FOO_1.clone(), FOO_2.clone()];

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let session_repo =
        MockSessionRepository::new().with_list_by_user(FOO.user.id, expected.clone());

    let sut = SessionFeatureServiceImpl {
        auth,
        db,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .list_by_user(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
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
        .list_by_user(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    assert_matches!(
        result,
        Err(SessionListByUserError::Auth(AuthError::Authenticate(
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
        .list_by_user(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    assert_matches!(
        result,
        Err(SessionListByUserError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}
