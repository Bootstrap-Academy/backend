use academy_auth_contracts::internal::{AuthInternalAuthenticateError, MockAuthInternalService};
use academy_core_internal_contracts::{InternalGetUserError, InternalService};
use academy_demo::user::FOO;
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::assert_matches;

use crate::{tests::Sut, InternalServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("auth", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let sut = InternalServiceImpl {
        db,
        auth_internal,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get_user(&"internal token".into(), FOO.user.id).await;

    // Assert
    assert_eq!(result.unwrap(), *FOO);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("auth", false);

    let sut = InternalServiceImpl {
        auth_internal,
        ..Sut::default()
    };

    // Act
    let result = sut.get_user(&"internal token".into(), FOO.user.id).await;

    // Assert
    assert_matches!(
        result,
        Err(InternalGetUserError::Auth(
            AuthInternalAuthenticateError::InvalidToken
        ))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("auth", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, None);

    let sut = InternalServiceImpl {
        db,
        auth_internal,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get_user(&"internal token".into(), FOO.user.id).await;

    // Assert
    assert_matches!(result, Err(InternalGetUserError::NotFound));
}
