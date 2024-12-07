use academy_auth_contracts::internal::{AuthInternalAuthenticateError, MockAuthInternalService};
use academy_core_heart_contracts::heart::{HeartAddError, MockHeartService};
use academy_core_internal_contracts::{InternalAddHeartsError, InternalService};
use academy_demo::user::FOO;
use academy_models::heart::Hearts;
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::assert_matches;

use crate::{tests::Sut, InternalServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Hearts {
        hearts: 42,
        last_refill: FOO.user.created_at,
    };

    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let heart = MockHeartService::new().with_add(FOO.user.id, -42, Ok(expected));

    let sut = InternalServiceImpl {
        auth_internal,
        db,
        user_repo,
        heart,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_hearts(&"internal token".into(), FOO.user.id, -42)
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", false);

    let sut = InternalServiceImpl {
        auth_internal,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_hearts(&"internal token".into(), FOO.user.id, 42)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(InternalAddHeartsError::Auth(
            AuthInternalAuthenticateError::InvalidToken
        ))
    );
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, false);

    let sut = InternalServiceImpl {
        auth_internal,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_hearts(&"internal token".into(), FOO.user.id, 42)
        .await;

    // Assert
    assert_matches!(result, Err(InternalAddHeartsError::UserNotFound));
}

#[tokio::test]
async fn not_enough_hearts() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let heart =
        MockHeartService::new().with_add(FOO.user.id, -42, Err(HeartAddError::NotEnoughHearts));

    let sut = InternalServiceImpl {
        auth_internal,
        db,
        user_repo,
        heart,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_hearts(&"internal token".into(), FOO.user.id, -42)
        .await;

    // Assert
    assert_matches!(result, Err(InternalAddHeartsError::NotEnoughHearts));
}
