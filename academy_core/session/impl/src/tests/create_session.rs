use academy_auth_contracts::MockAuthService;
use academy_core_mfa_contracts::authenticate::{
    MfaAuthenticateError, MfaAuthenticateResult, MockMfaAuthenticateService,
};
use academy_core_session_contracts::{
    SessionCreateCommand, SessionCreateError, SessionFeatureService,
    failed_auth_count::MockSessionFailedAuthCountService, session::MockSessionService,
};
use academy_demo::{
    session::{BAR_1, FOO_1},
    user::{BAR, BAR_PASSWORD, FOO, FOO_PASSWORD},
};
use academy_models::{auth::Login, mfa::MfaAuthentication, user::UserNameOrEmailAddress};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_shared_contracts::captcha::{CaptchaCheckError, MockCaptchaService};
use academy_utils::{Apply, assert_matches};

use crate::{SessionFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: FOO_1.device_name.clone(),
        mfa: MfaAuthentication::default(),
    };

    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 1)
        .with_reset(UserNameOrEmailAddress::Name(FOO.user.name.clone()))
        .with_reset(UserNameOrEmailAddress::Email(
            FOO.user.email.clone().unwrap(),
        ));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_name_or_email(cmd.name_or_email.clone(), Some(FOO.clone()));

    let auth = MockAuthService::new().with_authenticate_by_password(
        FOO.user.id,
        cmd.password.clone(),
        true,
    );

    let session = MockSessionService::new().with_create(
        FOO.clone(),
        cmd.device_name.clone(),
        true,
        expected.clone(),
    );

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        auth,
        session,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(cmd, None).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn ok_mfa() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: FOO_1.device_name.clone(),
        mfa: MfaAuthentication {
            totp_code: Some("123456".try_into().unwrap()),
            recovery_code: None,
        },
    };

    let expected = Login {
        user_composite: FOO.clone().with(|u| u.details.mfa_enabled = true),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 1)
        .with_reset(UserNameOrEmailAddress::Name(FOO.user.name.clone()))
        .with_reset(UserNameOrEmailAddress::Email(
            FOO.user.email.clone().unwrap(),
        ));

    let user_repo = MockUserRepository::new().with_get_composite_by_name_or_email(
        cmd.name_or_email.clone(),
        Some(expected.user_composite.clone()),
    );

    let auth = MockAuthService::new().with_authenticate_by_password(
        FOO.user.id,
        cmd.password.clone(),
        true,
    );

    let mfa_authenticate = MockMfaAuthenticateService::new().with_authenticate(
        FOO.user.id,
        cmd.mfa.clone(),
        Ok(MfaAuthenticateResult::Ok),
    );

    let session = MockSessionService::new().with_create(
        expected.user_composite.clone(),
        cmd.device_name.clone(),
        true,
        expected.clone(),
    );

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        auth,
        session,
        mfa_authenticate,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(cmd, None).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn ok_mfa_reset() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: FOO_1.device_name.clone(),
        mfa: MfaAuthentication {
            totp_code: Some("123456".try_into().unwrap()),
            recovery_code: None,
        },
    };

    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 1)
        .with_reset(UserNameOrEmailAddress::Name(FOO.user.name.clone()))
        .with_reset(UserNameOrEmailAddress::Email(
            FOO.user.email.clone().unwrap(),
        ));

    let user_repo = MockUserRepository::new().with_get_composite_by_name_or_email(
        cmd.name_or_email.clone(),
        Some(
            expected
                .user_composite
                .clone()
                .with(|u| u.details.mfa_enabled = true),
        ),
    );

    let auth = MockAuthService::new().with_authenticate_by_password(
        FOO.user.id,
        cmd.password.clone(),
        true,
    );

    let mfa_authenticate = MockMfaAuthenticateService::new().with_authenticate(
        FOO.user.id,
        cmd.mfa.clone(),
        Ok(MfaAuthenticateResult::Reset),
    );

    let session = MockSessionService::new().with_create(
        expected.user_composite.clone(),
        cmd.device_name.clone(),
        true,
        expected.clone(),
    );

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        auth,
        session,
        mfa_authenticate,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(cmd, None).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn ok_captcha() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: FOO_1.device_name.clone(),
        mfa: MfaAuthentication::default(),
    };

    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 3)
        .with_reset(UserNameOrEmailAddress::Name(FOO.user.name.clone()))
        .with_reset(UserNameOrEmailAddress::Email(
            FOO.user.email.clone().unwrap(),
        ));

    let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_name_or_email(cmd.name_or_email.clone(), Some(FOO.clone()));

    let auth = MockAuthService::new().with_authenticate_by_password(
        FOO.user.id,
        cmd.password.clone(),
        true,
    );

    let session = MockSessionService::new().with_create(
        FOO.clone(),
        cmd.device_name.clone(),
        true,
        expected.clone(),
    );

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        captcha,
        auth,
        session,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_session(cmd, Some("resp".try_into().unwrap()))
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn invalid_recaptcha_response() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: FOO_1.device_name.clone(),
        mfa: MfaAuthentication::default(),
    };

    let session_failed_auth_count =
        MockSessionFailedAuthCountService::new().with_get(cmd.name_or_email.clone(), 4);

    let captcha =
        MockCaptchaService::new().with_check(Some("resp"), Err(CaptchaCheckError::Failed));

    let sut = SessionFeatureServiceImpl {
        session_failed_auth_count,
        captcha,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_session(cmd, Some("resp".try_into().unwrap()))
        .await;

    // Assert
    assert_matches!(result, Err(SessionCreateError::Recaptcha));
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: FOO_1.device_name.clone(),
        mfa: MfaAuthentication::default(),
    };

    let db = MockDatabase::build(false);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 1)
        .with_increment(cmd.name_or_email.clone());

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_name_or_email(cmd.name_or_email.clone(), None);

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(cmd, None).await;

    // Assert
    assert_matches!(result, Err(SessionCreateError::InvalidCredentials));
}

#[tokio::test]
async fn wrong_password() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: FOO_1.device_name.clone(),
        mfa: MfaAuthentication::default(),
    };

    let db = MockDatabase::build(false);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 1)
        .with_increment(UserNameOrEmailAddress::Name(FOO.user.name.clone()))
        .with_increment(UserNameOrEmailAddress::Email(
            FOO.user.email.clone().unwrap(),
        ));

    let user_repo = MockUserRepository::new().with_get_composite_by_name_or_email(
        cmd.name_or_email.clone(),
        Some(FOO.clone().with(|u| u.details.mfa_enabled = true)),
    );

    let auth = MockAuthService::new().with_authenticate_by_password(
        FOO.user.id,
        cmd.password.clone(),
        false,
    );

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        auth,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(cmd, None).await;

    // Assert
    assert_matches!(result, Err(SessionCreateError::InvalidCredentials));
}

#[tokio::test]
async fn mfa_failed() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(FOO.user.name.clone()),
        password: FOO_PASSWORD.clone(),
        device_name: BAR_1.device_name.clone(),
        mfa: MfaAuthentication {
            totp_code: Some("123456".try_into().unwrap()),
            recovery_code: None,
        },
    };

    let db = MockDatabase::build(false);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 1)
        .with_increment(UserNameOrEmailAddress::Name(FOO.user.name.clone()))
        .with_increment(UserNameOrEmailAddress::Email(
            FOO.user.email.clone().unwrap(),
        ));

    let user_repo = MockUserRepository::new().with_get_composite_by_name_or_email(
        cmd.name_or_email.clone(),
        Some(FOO.clone().with(|u| u.details.mfa_enabled = true)),
    );

    let auth = MockAuthService::new().with_authenticate_by_password(
        FOO.user.id,
        cmd.password.clone(),
        true,
    );

    let mfa_authenticate = MockMfaAuthenticateService::new().with_authenticate(
        FOO.user.id,
        cmd.mfa.clone(),
        Err(MfaAuthenticateError::Failed),
    );

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        auth,
        user_repo,
        mfa_authenticate,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(cmd, None).await;

    // Assert
    assert_matches!(result, Err(SessionCreateError::MfaFailed));
}

#[tokio::test]
async fn user_disabled() {
    // Arrange
    let cmd = SessionCreateCommand {
        name_or_email: UserNameOrEmailAddress::Name(BAR.user.name.clone()),
        password: BAR_PASSWORD.clone(),
        device_name: BAR_1.device_name.clone(),
        mfa: MfaAuthentication::default(),
    };

    let db = MockDatabase::build(false);

    let session_failed_auth_count = MockSessionFailedAuthCountService::new()
        .with_get(cmd.name_or_email.clone(), 1)
        .with_reset(UserNameOrEmailAddress::Name(BAR.user.name.clone()));

    let user_repo = MockUserRepository::new()
        .with_get_composite_by_name_or_email(cmd.name_or_email.clone(), Some(BAR.clone()));

    let auth = MockAuthService::new().with_authenticate_by_password(
        BAR.user.id,
        cmd.password.clone(),
        true,
    );

    let sut = SessionFeatureServiceImpl {
        db,
        session_failed_auth_count,
        auth,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_session(cmd, None).await;

    // Assert
    assert_matches!(result, Err(SessionCreateError::UserDisabled));
}
