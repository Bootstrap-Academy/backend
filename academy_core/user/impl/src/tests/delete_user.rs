use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{UserDeleteError, UserFeatureService};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok_self() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        .with_invalidate_access_tokens(FOO.user.id);

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_delete(FOO.user.id, true);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn ok_admin() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())))
        .with_invalidate_access_tokens(FOO.user.id);

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_delete(FOO.user.id, true);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), FOO.user.id.into()).await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(UserDeleteError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(UserDeleteError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())))
        .with_invalidate_access_tokens(FOO.user.id);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_delete(FOO.user.id, false);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.delete_user(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(result, Err(UserDeleteError::NotFound));
}
