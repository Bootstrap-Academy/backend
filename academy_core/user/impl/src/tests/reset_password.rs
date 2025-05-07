use academy_core_user_contracts::{
    UserFeatureService, UserResetPasswordError,
    email_confirmation::{
        MockUserEmailConfirmationService, UserEmailConfirmationResetPasswordError,
    },
};
use academy_demo::{
    VERIFICATION_CODE_1,
    user::{FOO, FOO_PASSWORD},
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), Some(FOO.clone()));

    let user_email_confirmation = MockUserEmailConfirmationService::new().with_reset_password(
        FOO.user.id,
        VERIFICATION_CODE_1.clone(),
        FOO_PASSWORD.clone(),
        Ok(()),
    );

    let sut = UserFeatureServiceImpl {
        db,
        user_repo,
        user_email_confirmation,
        ..Sut::default()
    };

    // Act
    let result = sut
        .reset_password(
            FOO.user.email.clone().unwrap(),
            VERIFICATION_CODE_1.clone(),
            FOO_PASSWORD.clone(),
        )
        .await;

    // Act
    assert_eq!(result.unwrap(), *FOO);
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), None);

    let sut = UserFeatureServiceImpl {
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .reset_password(
            FOO.user.email.clone().unwrap(),
            VERIFICATION_CODE_1.clone(),
            FOO_PASSWORD.clone(),
        )
        .await;

    // Act
    assert_matches!(result, Err(UserResetPasswordError::Failed));
}

#[tokio::test]
async fn invalid_code() {
    // Arrange
    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), Some(FOO.clone()));

    let user_email_confirmation = MockUserEmailConfirmationService::new().with_reset_password(
        FOO.user.id,
        VERIFICATION_CODE_1.clone(),
        FOO_PASSWORD.clone(),
        Err(UserEmailConfirmationResetPasswordError::InvalidCode),
    );

    let sut = UserFeatureServiceImpl {
        db,
        user_repo,
        user_email_confirmation,
        ..Sut::default()
    };

    // Act
    let result = sut
        .reset_password(
            FOO.user.email.clone().unwrap(),
            VERIFICATION_CODE_1.clone(),
            FOO_PASSWORD.clone(),
        )
        .await;

    // Act
    assert_matches!(result, Err(UserResetPasswordError::Failed));
}
