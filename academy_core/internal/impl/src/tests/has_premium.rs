use academy_auth_contracts::internal::{AuthInternalAuthenticateError, MockAuthInternalService};
use academy_core_internal_contracts::{InternalHasPremiumError, InternalService};
use academy_core_premium_contracts::premium::MockPremiumService;
use academy_demo::{UUID1, user::FOO};
use academy_models::premium::Premium;
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{InternalServiceImpl, tests::Sut};

#[tokio::test]
async fn ok_true() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let premium = MockPremiumService::new().with_get_active(
        FOO.user.id,
        Some(Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: FOO.user.created_at,
            until: FOO.user.last_login.unwrap(),
        }),
    );

    let sut = InternalServiceImpl {
        auth_internal,
        db,
        user_repo,
        premium,
        ..Sut::default()
    };

    // Act
    let result = sut.has_premium(&"internal token".into(), FOO.user.id).await;

    // Assert
    assert!(result.unwrap());
}

#[tokio::test]
async fn ok_false() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let premium = MockPremiumService::new().with_get_active(FOO.user.id, None);

    let sut = InternalServiceImpl {
        auth_internal,
        db,
        user_repo,
        premium,
        ..Sut::default()
    };

    // Act
    let result = sut.has_premium(&"internal token".into(), FOO.user.id).await;

    // Assert
    assert!(!result.unwrap());
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
    let result = sut.has_premium(&"internal token".into(), FOO.user.id).await;

    // Assert
    assert_matches!(
        result,
        Err(InternalHasPremiumError::Auth(
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
    let result = sut.has_premium(&"internal token".into(), FOO.user.id).await;

    // Assert
    assert_matches!(result, Err(InternalHasPremiumError::UserNotFound));
}
