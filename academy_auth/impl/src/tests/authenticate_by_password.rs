use academy_auth_contracts::{AuthService, AuthenticateByPasswordError};
use academy_demo::user::{FOO, FOO_PASSWORD};
use academy_persistence_contracts::user::MockUserRepository;
use academy_shared_contracts::password::MockPasswordService;
use academy_utils::assert_matches;

use crate::{AuthServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let password_hash = "hash of foo's password";

    let user_repo =
        MockUserRepository::new().with_get_password_hash(FOO.user.id, Some(password_hash.into()));

    let password = MockPasswordService::new().with_verify(
        FOO_PASSWORD.clone().into_inner(),
        password_hash.into(),
        true,
    );

    let sut = AuthServiceImpl {
        user_repo,
        password,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_password(&mut (), FOO.user.id, FOO_PASSWORD.clone())
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let user_repo = MockUserRepository::new().with_get_password_hash(FOO.user.id, None);

    let sut = AuthServiceImpl {
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_password(&mut (), FOO.user.id, FOO_PASSWORD.clone())
        .await;

    // Assert
    assert_matches!(result, Err(AuthenticateByPasswordError::InvalidCredentials));
}

#[tokio::test]
async fn wrong_password() {
    // Arrange
    let password_hash = "hash of foo's password";

    let user_repo =
        MockUserRepository::new().with_get_password_hash(FOO.user.id, Some(password_hash.into()));

    let password = MockPasswordService::new().with_verify(
        FOO_PASSWORD.clone().into_inner(),
        password_hash.into(),
        false,
    );

    let sut = AuthServiceImpl {
        user_repo,
        password,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_password(&mut (), FOO.user.id, FOO_PASSWORD.clone())
        .await;

    // Assert
    assert_matches!(result, Err(AuthenticateByPasswordError::InvalidCredentials));
}
