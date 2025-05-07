use academy_auth_contracts::MockAuthService;
use academy_core_heart_contracts::{HeartFeatureService, HeartGetError, heart::MockHeartService};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    heart::Hearts,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{HeartFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Hearts {
        hearts: 4,
        last_refill: FOO.user.created_at,
    };

    let db = MockDatabase::build(false);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let heart = MockHeartService::new().with_get(FOO.user.id, expected);

    let sut = HeartFeatureServiceImpl {
        db,
        auth,
        user_repo,
        heart,
        ..Sut::default()
    };

    // Act
    let result = sut.get(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = HeartFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.get(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(HeartGetError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = HeartFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.get(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(HeartGetError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, false);

    let sut = HeartFeatureServiceImpl {
        db,
        auth,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(result, Err(HeartGetError::UserNotFound));
}
