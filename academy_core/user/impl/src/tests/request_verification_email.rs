use academy_auth_contracts::MockAuthService;
use std::time::Duration;

use academy_core_user_contracts::{
    UserFeatureService, UserRequestVerificationEmailError,
    email_confirmation::MockUserEmailConfirmationService, email_limit::MockUserEmailLimitService,
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::{Apply, assert_matches};

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok_self() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(FOO.clone().with(|u| u.user.email_verified = false)),
    );

    let user_email_limit = foo_within_limit();

    let user_email_confirmation = MockUserEmailConfirmationService::new()
        .with_request_verification(
            FOO.user.id,
            FOO.user
                .email
                .clone()
                .unwrap()
                .with_name(FOO.profile.display_name.clone().into_inner()),
        );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_email_confirmation,
        user_email_limit,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn ok_admin() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())))
        .with_authenticate_in_transaction(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_lock_account(FOO.user.id, true)
        .with_lock_account(ADMIN.user.id, true)
        .with_get_composite(
            FOO.user.id,
            Some(FOO.clone().with(|u| u.user.email_verified = false)),
        );

    // The email is counted for the account it is for, not the administrator's.
    let user_email_limit = foo_within_limit();

    let user_email_confirmation = MockUserEmailConfirmationService::new()
        .with_request_verification(
            FOO.user.id,
            FOO.user
                .email
                .clone()
                .unwrap()
                .with_name(FOO.profile.display_name.clone().into_inner()),
        );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_email_confirmation,
        user_email_limit,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), FOO.user.id.into())
        .await;

    // Assert
    result.unwrap();
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), FOO.user.id.into())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserRequestVerificationEmailError::Auth(
            AuthError::Authenticate(AuthenticateError::InvalidToken)
        ))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), FOO.user.id.into())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserRequestVerificationEmailError::Auth(
            AuthError::Authorize(AuthorizeError::Admin)
        ))
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())))
        .with_authenticate_in_transaction(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new()
        .with_lock_account(ADMIN.user.id, true)
        .with_lock_account(FOO.user.id, true)
        .with_get_composite(FOO.user.id, None);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), FOO.user.id.into())
        .await;

    // Assert
    assert_matches!(result, Err(UserRequestVerificationEmailError::NotFound));
}

#[tokio::test]
async fn already_verified() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserRequestVerificationEmailError::AlreadyVerified)
    );
}

#[tokio::test]
async fn no_email() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((BAR.user.clone(), BAR_1.clone())))
        .with_authenticate_in_transaction(Some((BAR.user.clone(), BAR_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(BAR.user.id, Some(BAR.clone()));

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    assert_matches!(result, Err(UserRequestVerificationEmailError::NoEmail));
}

#[tokio::test]
async fn limited() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
        .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(FOO.clone().with(|u| u.user.email_verified = false)),
    );

    // No email is sent.
    let user_email_limit = MockUserEmailLimitService::new().with_reserve_verification(
        FOO.user.id,
        FOO.user.email.clone().unwrap(),
        Err(Duration::from_secs(42)),
    );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_email_limit,
        ..Sut::default()
    };

    // Act
    let result = sut
        .request_verification_email(&"token".into(), UserIdOrSelf::Slf)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserRequestVerificationEmailError::RateLimit(retry_after))
            if *retry_after == Duration::from_secs(42)
    );
}

fn foo_within_limit() -> MockUserEmailLimitService {
    MockUserEmailLimitService::new().with_reserve_verification(
        FOO.user.id,
        FOO.user.email.clone().unwrap(),
        Ok(()),
    )
}
