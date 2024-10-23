use academy_auth_contracts::internal::{AuthInternalAuthenticateError, MockAuthInternalService};
use academy_core_internal_contracts::{InternalGetUserByEmailError, InternalService};
use academy_demo::user::FOO;
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::assert_matches;

use crate::{tests::Sut, InternalServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("auth", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), Some(FOO.clone()));

    let sut = InternalServiceImpl {
        db,
        auth_internal,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .get_user_by_email(&"internal token".into(), FOO.user.email.clone().unwrap())
        .await;

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
    let result = sut
        .get_user_by_email(&"internal token".into(), FOO.user.email.clone().unwrap())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(InternalGetUserByEmailError::Auth(
            AuthInternalAuthenticateError::InvalidToken
        ))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("auth", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), None);

    let sut = InternalServiceImpl {
        db,
        auth_internal,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .get_user_by_email(&"internal token".into(), FOO.user.email.clone().unwrap())
        .await;

    // Assert
    assert_matches!(result, Err(InternalGetUserByEmailError::NotFound));
}
