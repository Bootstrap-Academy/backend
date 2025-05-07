use academy_core_user_contracts::{
    UserFeatureService, UserVerifyEmailError,
    email_confirmation::{MockUserEmailConfirmationService, UserEmailConfirmationVerifyEmailError},
};
use academy_demo::{VERIFICATION_CODE_1, user::FOO};
use academy_persistence_contracts::MockDatabase;
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let db = MockDatabase::build(true);

    let user_email_confirmation = MockUserEmailConfirmationService::new()
        .with_verify_email(VERIFICATION_CODE_1.clone(), Ok(FOO.clone()));

    let sut = UserFeatureServiceImpl {
        db,
        user_email_confirmation,
        ..Sut::default()
    };

    // Act
    let result = sut.verify_email(VERIFICATION_CODE_1.clone()).await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn invalid_code() {
    // Arrange
    let db = MockDatabase::build(false);

    let user_email_confirmation = MockUserEmailConfirmationService::new().with_verify_email(
        VERIFICATION_CODE_1.clone(),
        Err(UserEmailConfirmationVerifyEmailError::InvalidCode),
    );

    let sut = UserFeatureServiceImpl {
        db,
        user_email_confirmation,
        ..Sut::default()
    };

    // Act
    let result = sut.verify_email(VERIFICATION_CODE_1.clone()).await;

    // Assert
    assert_matches!(result, Err(UserVerifyEmailError::InvalidCode));
}

#[tokio::test]
async fn already_verified() {
    // Arrange
    let db = MockDatabase::build(false);

    let user_email_confirmation = MockUserEmailConfirmationService::new().with_verify_email(
        VERIFICATION_CODE_1.clone(),
        Err(UserEmailConfirmationVerifyEmailError::AlreadyVerified),
    );

    let sut = UserFeatureServiceImpl {
        db,
        user_email_confirmation,
        ..Sut::default()
    };

    // Act
    let result = sut.verify_email(VERIFICATION_CODE_1.clone()).await;

    // Assert
    result.unwrap();
}
