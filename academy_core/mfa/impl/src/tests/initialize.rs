use academy_auth_contracts::MockAuthService;
use academy_core_mfa_contracts::{
    self, MfaFeatureService, MfaInitializeError, totp_device::MockMfaTotpDeviceService,
};
use academy_demo::{
    mfa::FOO_TOTP_1,
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    mfa::TotpSetup,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    MockDatabase, mfa::MockMfaRepository, user::MockUserRepository,
};
use academy_utils::{Apply, assert_matches};

use crate::{MfaFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn new() {
    // Arrange
    let expected = TotpSetup {
        secret: "the totp secret".into(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let mfa_repo = MockMfaRepository::new().with_list_totp_devices_by_user(FOO.user.id, vec![]);

    let mfa_totp_device =
        MockMfaTotpDeviceService::new().with_create(FOO.user.id, expected.clone());

    let sut = MfaFeatureServiceImpl {
        auth,
        db,
        user_repo,
        mfa_repo,
        mfa_totp_device,
        ..Sut::default()
    };

    // Act
    let result = sut.initialize(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn reset_disabled() {
    // Arrange
    let expected = TotpSetup {
        secret: "the totp secret".into(),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let mfa_repo = MockMfaRepository::new()
        .with_list_totp_devices_by_user(FOO.user.id, vec![FOO_TOTP_1.clone()]);

    let mfa_totp_device =
        MockMfaTotpDeviceService::new().with_reset(FOO_TOTP_1.id, expected.clone());

    let sut = MfaFeatureServiceImpl {
        auth,
        db,
        user_repo,
        mfa_repo,
        mfa_totp_device,
        ..Sut::default()
    };

    // Act
    let result = sut.initialize(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = MfaFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.initialize(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(MfaInitializeError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = MfaFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.initialize(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(MfaInitializeError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, false);

    let sut = MfaFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.initialize(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(result, Err(MfaInitializeError::NotFound));
}

#[tokio::test]
async fn already_enabled() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let mfa_repo = MockMfaRepository::new().with_list_totp_devices_by_user(
        FOO.user.id,
        vec![FOO_TOTP_1.clone().with(|t| t.enabled = true)],
    );

    let sut = MfaFeatureServiceImpl {
        auth,
        db,
        user_repo,
        mfa_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.initialize(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_matches!(result, Err(MfaInitializeError::AlreadyEnabled));
}
