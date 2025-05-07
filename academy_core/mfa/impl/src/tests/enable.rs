use academy_auth_contracts::MockAuthService;
use academy_core_mfa_contracts::{
    MfaEnableError, MfaFeatureService,
    recovery::MockMfaRecoveryService,
    totp_device::{MfaTotpDeviceConfirmError, MockMfaTotpDeviceService},
};
use academy_demo::{
    mfa::FOO_TOTP_1,
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    mfa::{MfaRecoveryCode, TotpCode},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    MockDatabase, mfa::MockMfaRepository, user::MockUserRepository,
};
use academy_utils::{Apply, assert_matches};

use crate::{MfaFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = MfaRecoveryCode::try_new("PJVURV-QRK3YJ-O3U7T6-D50KAC").unwrap();
    let code = TotpCode::try_new("123456").unwrap();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let mfa_repo = MockMfaRepository::new()
        .with_list_totp_devices_by_user(FOO.user.id, vec![FOO_TOTP_1.clone()]);

    let mfa_totp_device = MockMfaTotpDeviceService::new().with_confirm(
        FOO_TOTP_1.clone(),
        code.clone(),
        Ok(FOO_TOTP_1.clone().with(|x| x.enabled = true)),
    );

    let mfa_recovery = MockMfaRecoveryService::new().with_setup(FOO.user.id, expected.clone());

    let sut = MfaFeatureServiceImpl {
        auth,
        db,
        user_repo,
        mfa_repo,
        mfa_recovery,
        mfa_totp_device,
        ..Sut::default()
    };

    // Act
    let result = sut.enable(&"token".into(), UserIdOrSelf::Slf, code).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let code = TotpCode::try_new("123456").unwrap();

    let auth = MockAuthService::new().with_authenticate(None);

    let sut = MfaFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.enable(&"token".into(), FOO.user.id.into(), code).await;

    // Assert
    assert_matches!(
        result,
        Err(MfaEnableError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let code = TotpCode::try_new("123456").unwrap();

    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = MfaFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.enable(&"token".into(), FOO.user.id.into(), code).await;

    // Assert
    assert_matches!(
        result,
        Err(MfaEnableError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let code = TotpCode::try_new("123456").unwrap();

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
    let result = sut.enable(&"token".into(), FOO.user.id.into(), code).await;

    // Assert
    assert_matches!(result, Err(MfaEnableError::NotFound));
}

#[tokio::test]
async fn already_enabled() {
    // Arrange
    let code = TotpCode::try_new("123456").unwrap();

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
    let result = sut.enable(&"token".into(), UserIdOrSelf::Slf, code).await;

    // Assert
    assert_matches!(result, Err(MfaEnableError::AlreadyEnabled));
}

#[tokio::test]
async fn not_initialized() {
    // Arrange
    let code = TotpCode::try_new("123456").unwrap();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let mfa_repo = MockMfaRepository::new().with_list_totp_devices_by_user(FOO.user.id, vec![]);

    let sut = MfaFeatureServiceImpl {
        auth,
        db,
        user_repo,
        mfa_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.enable(&"token".into(), UserIdOrSelf::Slf, code).await;

    // Assert
    assert_matches!(result, Err(MfaEnableError::NotInitialized));
}

#[tokio::test]
async fn invalid_code() {
    // Arrange
    let code = TotpCode::try_new("123456").unwrap();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let mfa_repo = MockMfaRepository::new()
        .with_list_totp_devices_by_user(FOO.user.id, vec![FOO_TOTP_1.clone()]);

    let mfa_totp_device = MockMfaTotpDeviceService::new().with_confirm(
        FOO_TOTP_1.clone(),
        code.clone(),
        Err(MfaTotpDeviceConfirmError::InvalidCode),
    );

    let sut = MfaFeatureServiceImpl {
        auth,
        db,
        user_repo,
        mfa_repo,
        mfa_totp_device,
        ..Sut::default()
    };

    // Act
    let result = sut.enable(&"token".into(), UserIdOrSelf::Slf, code).await;

    // Assert
    assert_matches!(result, Err(MfaEnableError::InvalidCode));
}
