use academy_core_oauth2_contracts::registration::MockOAuth2RegistrationService;
use academy_core_session_contracts::session::MockSessionService;
use academy_core_user_contracts::{
    UserCreateError, UserCreateRequest, UserFeatureService,
    user::{MockUserService, UserCreateCommand},
};
use academy_demo::{
    oauth2::{FOO_OAUTH2_LINK_1, TEST_OAUTH2_PROVIDER_ID},
    session::FOO_1,
    user::FOO,
};
use academy_models::{
    auth::Login,
    oauth2::{OAuth2Registration, OAuth2RegistrationToken},
};
use academy_persistence_contracts::MockDatabase;
use academy_shared_contracts::captcha::{CaptchaCheckError, MockCaptchaService};
use academy_utils::assert_matches;

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
    };

    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

    let user = MockUserService::new().with_create(req_to_cmd(&request), Ok(FOO.clone()));

    let session = MockSessionService::new().with_create(
        FOO.clone(),
        FOO_1.device_name.clone(),
        true,
        expected.clone(),
    );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_user(
            request,
            FOO_1.device_name.clone(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn ok_oauth2() {
    // Arrange
    let token = OAuth2RegistrationToken::try_new(
        "K7oACiokVoyttnGgYxJwCc2VCvDbQI10Bewthc5exlyQly2JZCViycDereak92oB",
    )
    .unwrap();

    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: None,
        oauth2_registration_token: Some(token.clone()),
    };

    let expected = Login {
        user_composite: FOO.clone(),
        session: FOO_1.clone(),
        access_token: "the access token".into(),
        refresh_token: "some refresh token".into(),
    };

    let db = MockDatabase::build(true);

    let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

    let oauth2_registration = MockOAuth2RegistrationService::new()
        .with_get(
            token.clone(),
            Some(OAuth2Registration {
                provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
                remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
            }),
        )
        .with_remove(token);

    let user = MockUserService::new().with_create(req_to_cmd(&request), Ok(FOO.clone()));

    let session = MockSessionService::new().with_create(
        FOO.clone(),
        FOO_1.device_name.clone(),
        true,
        expected.clone(),
    );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user,
        oauth2_registration,
        session,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_user(
            request,
            FOO_1.device_name.clone(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn no_login_method() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: None,
        oauth2_registration_token: None,
    };

    let sut = Sut::default();

    // Act
    let result = sut
        .create_user(request, FOO_1.device_name.clone(), None)
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::NoLoginMethod));
}

#[tokio::test]
async fn invalid_recaptcha_response() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
    };

    let captcha =
        MockCaptchaService::new().with_check(Some("resp"), Err(CaptchaCheckError::Failed));

    let sut = UserFeatureServiceImpl {
        captcha,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_user(
            request,
            FOO_1.device_name.clone(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::Recaptcha));
}

#[tokio::test]
async fn name_conflict() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
    };

    let db = MockDatabase::build(false);

    let captcha = MockCaptchaService::new().with_check(None, Ok(()));

    let user = MockUserService::new().with_create(
        req_to_cmd(&request),
        Err(academy_core_user_contracts::user::UserCreateError::NameConflict),
    );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_user(request, FOO_1.device_name.clone(), None)
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::NameConflict));
}

#[tokio::test]
async fn email_conflict() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
    };

    let db = MockDatabase::build(false);

    let captcha = MockCaptchaService::new().with_check(None, Ok(()));

    let user = MockUserService::new().with_create(
        req_to_cmd(&request),
        Err(academy_core_user_contracts::user::UserCreateError::EmailConflict),
    );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_user(request, FOO_1.device_name.clone(), None)
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::EmailConflict));
}

#[tokio::test]
async fn oauth2_invalid_registration_token() {
    // Arrange
    let token = OAuth2RegistrationToken::try_new(
        "K7oACiokVoyttnGgYxJwCc2VCvDbQI10Bewthc5exlyQly2JZCViycDereak92oB",
    )
    .unwrap();
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: None,
        oauth2_registration_token: Some(token.clone()),
    };

    let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

    let oauth2_registration = MockOAuth2RegistrationService::new().with_get(token, None);

    let sut = UserFeatureServiceImpl {
        captcha,
        oauth2_registration,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_user(
            request,
            FOO_1.device_name.clone(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::InvalidOAuthRegistrationToken));
}

#[tokio::test]
async fn oauth2_remote_already_linked() {
    // Arrange
    let token = OAuth2RegistrationToken::try_new(
        "K7oACiokVoyttnGgYxJwCc2VCvDbQI10Bewthc5exlyQly2JZCViycDereak92oB",
    )
    .unwrap();
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: None,
        oauth2_registration_token: Some(
            "K7oACiokVoyttnGgYxJwCc2VCvDbQI10Bewthc5exlyQly2JZCViycDereak92oB"
                .try_into()
                .unwrap(),
        ),
    };

    let db = MockDatabase::build(false);

    let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

    let oauth2_registration = MockOAuth2RegistrationService::new().with_get(
        token,
        Some(OAuth2Registration {
            provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
            remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
        }),
    );

    let user = MockUserService::new().with_create(
        req_to_cmd(&request),
        Err(academy_core_user_contracts::user::UserCreateError::RemoteAlreadyLinked),
    );

    let sut = UserFeatureServiceImpl {
        db,
        captcha,
        user,
        oauth2_registration,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_user(
            request,
            FOO_1.device_name.clone(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::RemoteAlreadyLinked));
}

fn req_to_cmd(req: &UserCreateRequest) -> UserCreateCommand {
    UserCreateCommand {
        name: req.name.clone(),
        display_name: req.display_name.clone(),
        email: req.email.clone(),
        password: req.password.clone(),
        admin: false,
        enabled: true,
        email_verified: false,
        oauth2_registration: req
            .oauth2_registration_token
            .as_ref()
            .map(|_| OAuth2Registration {
                provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
                remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
            }),
    }
}
