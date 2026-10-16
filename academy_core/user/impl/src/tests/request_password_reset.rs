use std::time::Duration;

use academy_core_user_contracts::{
    UserFeatureService, UserRequestPasswordResetError,
    email_confirmation::MockUserEmailConfirmationService,
    email_limit::{MockUserEmailLimitService, UserEmailCodeKind},
};
use academy_demo::user::FOO;
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_shared_contracts::captcha::{CaptchaCheckError, MockCaptchaService};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let db = MockDatabase::build(false);

    let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), Some(FOO.clone()))
        .with_lock_account(FOO.user.id, true)
        .with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_email_limit = MockUserEmailLimitService::new()
        .with_reserve_address(
            UserEmailCodeKind::PasswordReset,
            FOO.user.email.clone().unwrap(),
            Ok(()),
        )
        .with_reserve_account(UserEmailCodeKind::PasswordReset, FOO.user.id, Ok(()));

    let user_email_confirmation = MockUserEmailConfirmationService::new()
        .with_request_password_reset(
            FOO.user.id,
            FOO.user
                .email
                .clone()
                .unwrap()
                .with_name(FOO.profile.display_name.clone().into_inner()),
        );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user_repo,
        user_email_confirmation,
        user_email_limit,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_password_reset(
            FOO.user.email.clone().unwrap(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn invalid_captcha_response() {
    // Arrange
    let captcha =
        MockCaptchaService::new().with_check(Some("resp"), Err(CaptchaCheckError::Failed));

    let sut = UserFeatureServiceImpl {
        captcha,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_password_reset(
            FOO.user.email.clone().unwrap(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserRequestPasswordResetError::Recaptcha));
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let db = MockDatabase::build(false);

    let captcha = MockCaptchaService::new().with_check(None, Ok(()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), None);

    // An address without an account is counted like any other.
    let user_email_limit = MockUserEmailLimitService::new().with_reserve_address(
        UserEmailCodeKind::PasswordReset,
        FOO.user.email.clone().unwrap(),
        Ok(()),
    );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user_repo,
        user_email_limit,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_password_reset(FOO.user.email.clone().unwrap(), None)
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn address_limited() {
    // Arrange
    let captcha = MockCaptchaService::new().with_check(None, Ok(()));

    // The refusal comes before the address is looked up, so it is the same
    // whether or not an account has the address.
    let user_email_limit = MockUserEmailLimitService::new().with_reserve_address(
        UserEmailCodeKind::PasswordReset,
        FOO.user.email.clone().unwrap(),
        Err(Duration::from_secs(42)),
    );

    let sut = UserFeatureServiceImpl {
        captcha,
        user_email_limit,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_password_reset(FOO.user.email.clone().unwrap(), None)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserRequestPasswordResetError::RateLimit(retry_after))
            if *retry_after == Duration::from_secs(42)
    );
}

#[tokio::test]
async fn account_limited() {
    // Arrange
    let db = MockDatabase::build(false);

    let captcha = MockCaptchaService::new().with_check(None, Ok(()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), Some(FOO.clone()))
        .with_lock_account(FOO.user.id, true)
        .with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_email_limit = MockUserEmailLimitService::new()
        .with_reserve_address(
            UserEmailCodeKind::PasswordReset,
            FOO.user.email.clone().unwrap(),
            Ok(()),
        )
        .with_reserve_account(
            UserEmailCodeKind::PasswordReset,
            FOO.user.id,
            Err(Duration::from_secs(600)),
        );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user_repo,
        user_email_limit,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_password_reset(FOO.user.email.clone().unwrap(), None)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserRequestPasswordResetError::RateLimit(retry_after))
            if *retry_after == Duration::from_secs(600)
    );
}
