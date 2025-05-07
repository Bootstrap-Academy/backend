use academy_core_user_contracts::{
    UserFeatureService, UserRequestPasswordResetError,
    email_confirmation::MockUserEmailConfirmationService,
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
        .with_get_composite_by_email(FOO.user.email.clone().unwrap(), Some(FOO.clone()));

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

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_password_reset(FOO.user.email.clone().unwrap(), None)
        .await;

    // Assert
    result.unwrap();
}
