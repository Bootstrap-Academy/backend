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
    session::SessionOrigin,
};
use academy_persistence_contracts::{MockDatabase, MockTransaction};
use academy_shared_contracts::captcha::{CaptchaCheckError, MockCaptchaService};
use academy_utils::assert_matches;

use crate::{
    UserFeatureServiceImpl,
    tests::{Sut, TERMS_VERSION},
};

#[tokio::test]
async fn ok() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
        false,
        SessionOrigin::SignIn,
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
async fn database_unavailable_returns_error_before_creating_user() {
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
    };

    let mut db = MockDatabase::new();
    db.expect_begin_transaction().once().return_once(|| {
        Box::pin(std::future::ready(Err(anyhow::anyhow!(
            "database is in recovery mode"
        ))))
    });

    let sut = UserFeatureServiceImpl {
        db,
        captcha: MockCaptchaService::new().with_check(None, Ok(())),
        // These mocks have no expectations: creating a user or session before
        // acquiring a transaction would fail this test.
        ..Sut::default()
    };

    let result = sut.create_user(request, None, None).await;

    let Err(UserCreateError::Other(error)) = result else {
        panic!("registration must return a database error");
    };
    assert_eq!(
        error.root_cause().to_string(),
        "database is in recovery mode"
    );
}

#[tokio::test]
async fn failed_commit_returns_error_instead_of_login() {
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
    };

    let mut txn = MockTransaction::new();
    txn.expect_commit().once().return_once(|| {
        Box::pin(std::future::ready(Err(anyhow::anyhow!(
            "connection lost during commit"
        ))))
    });
    let mut db = MockDatabase::new();
    db.expect_begin_transaction()
        .once()
        .return_once(|| Box::pin(std::future::ready(Ok(txn))));

    let sut = UserFeatureServiceImpl {
        db,
        captcha: MockCaptchaService::new().with_check(None, Ok(())),
        user: MockUserService::new().with_create(req_to_cmd(&request), Ok(FOO.clone())),
        session: MockSessionService::new().with_create(
            FOO.clone(),
            None,
            true,
            false,
            SessionOrigin::SignIn,
            Login {
                user_composite: FOO.clone(),
                session: FOO_1.clone(),
                access_token: "the access token".into(),
                refresh_token: "some refresh token".into(),
            },
        ),
        ..Sut::default()
    };

    let result = sut.create_user(request, None, None).await;

    let Err(UserCreateError::Other(error)) = result else {
        panic!("registration must not return a login before a successful commit");
    };
    assert_eq!(
        error.root_cause().to_string(),
        "connection lost during commit"
    );
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
        .with_consume(token, true);

    let user = MockUserService::new().with_create(req_to_cmd(&request), Ok(FOO.clone()));

    let session = MockSessionService::new().with_create(
        FOO.clone(),
        FOO_1.device_name.clone(),
        true,
        false,
        SessionOrigin::SignIn,
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

/// A registration token that a concurrent request has already redeemed does not
/// create a second account.
#[tokio::test]
async fn oauth2_registration_token_already_used() {
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
    };

    let db = MockDatabase::build_expect_rollback();

    let captcha = MockCaptchaService::new().with_check(Some("resp"), Ok(()));

    let oauth2_registration = MockOAuth2RegistrationService::new()
        .with_get(
            token.clone(),
            Some(OAuth2Registration {
                provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
                remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
            }),
        )
        .with_consume(token, false);

    let user = MockUserService::new().with_create(req_to_cmd(&request), Ok(FOO.clone()));

    let session = MockSessionService::new().with_create(
        FOO.clone(),
        FOO_1.device_name.clone(),
        true,
        false,
        SessionOrigin::SignIn,
        Login {
            user_composite: FOO.clone(),
            session: FOO_1.clone(),
            access_token: "the access token".into(),
            refresh_token: "some refresh token".into(),
        },
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
    assert_matches!(result, Err(UserCreateError::InvalidOAuthRegistrationToken));
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
async fn age_not_confirmed() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: false,
    };

    let sut = Sut::default();

    // Act
    let result = sut
        .create_user(request, FOO_1.device_name.clone(), None)
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::AgeNotConfirmed));
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
        terms_version: TERMS_VERSION.clone(),
        age_confirmed: true,
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
        // The recorded version is the server's, whatever the request carried.
        terms_version: Some(TERMS_VERSION.clone()),
        age_confirmed: req.age_confirmed,
    }
}

/// A version other than the one that is currently in force is refused, so a
/// consent record can never claim a version that was never published.
#[tokio::test]
async fn outdated_terms_version() {
    // Arrange
    let request = UserCreateRequest {
        name: FOO.user.name.clone(),
        display_name: FOO.profile.display_name.clone(),
        email: FOO.user.email.clone().unwrap(),
        password: Some("secure password".try_into().unwrap()),
        oauth2_registration_token: None,
        terms_version: "1999-01".try_into().unwrap(),
        age_confirmed: true,
    };

    let sut = Sut::default();

    // Act
    let result = sut
        .create_user(
            request,
            FOO_1.device_name.clone(),
            Some("resp".try_into().unwrap()),
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserCreateError::TermsVersionMismatch));
}

#[tokio::test]
async fn registration_override_is_explicit_and_records_only_the_offered_version() {
    let registration_version: academy_models::user::TermsVersion =
        "reviewed-signup-only".try_into().unwrap();
    for current_request in [false, true] {
        let request = UserCreateRequest {
            name: FOO.user.name.clone(),
            display_name: FOO.profile.display_name.clone(),
            email: FOO.user.email.clone().unwrap(),
            password: Some("secure password".try_into().unwrap()),
            oauth2_registration_token: None,
            age_confirmed: true,
            terms_version: if current_request {
                registration_version.clone()
            } else {
                TERMS_VERSION.clone()
            },
        };
        let mut sut = Sut::default();
        sut.config.registration_terms_version = Some(registration_version.clone());
        if current_request {
            let mut command = req_to_cmd(&request);
            command.terms_version = Some(registration_version.clone());
            let mut account = FOO.clone();
            account.user.terms_version = Some(registration_version.clone());
            let expected = Login {
                user_composite: account.clone(),
                session: FOO_1.clone(),
                access_token: "synthetic".into(),
                refresh_token: "synthetic".into(),
            };
            sut.db = MockDatabase::build(true);
            sut.captcha = MockCaptchaService::new().with_check(None, Ok(()));
            sut.user = MockUserService::new().with_create(command, Ok(account.clone()));
            sut.session = MockSessionService::new().with_create(
                account,
                FOO_1.device_name.clone(),
                true,
                false,
                SessionOrigin::SignIn,
                expected.clone(),
            );
            assert_eq!(
                sut.create_user(request, FOO_1.device_name.clone(), None)
                    .await
                    .unwrap(),
                expected
            );
        } else {
            assert!(matches!(
                sut.create_user(request, None, None).await,
                Err(UserCreateError::TermsVersionMismatch)
            ));
        }
    }
}
