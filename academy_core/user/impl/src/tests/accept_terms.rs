use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserAcceptTermsError, UserAcceptTermsRequest, UserFeatureService, update::MockUserUpdateService,
};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError},
    user::User,
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;
use chrono::{TimeZone, Utc};

use crate::{UserFeatureServiceImpl, tests::Sut};

fn terms_version() -> academy_models::user::TermsVersion {
    "2026-09".try_into().unwrap()
}

#[tokio::test]
async fn ok() {
    // Arrange
    let now = Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap();

    let updated_user = User {
        terms_version: Some(terms_version()),
        terms_accepted_at: Some(now),
        ..FOO.user.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update = MockUserUpdateService::new().with_accept_terms(
        FOO.user.clone(),
        terms_version(),
        updated_user.clone(),
    );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_update,
        ..Sut::default()
    };

    // Act
    let result = sut
        .accept_terms(
            &"token".into(),
            UserAcceptTermsRequest {
                terms_version: terms_version(),
                age_confirmed: true,
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap().user, updated_user);
}

/// Accounts that were created before the acceptance was recorded have no
/// `terms_version` at all, and have to be able to accept the terms as well.
#[tokio::test]
async fn ok_account_without_previous_acceptance() {
    // Arrange
    let now = Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap();

    let updated_user = User {
        terms_version: Some(terms_version()),
        terms_accepted_at: Some(now),
        age_confirmed_at: Some(now),
        ..ADMIN.user.clone()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo =
        MockUserRepository::new().with_get_composite(ADMIN.user.id, Some(ADMIN.clone()));

    let user_update = MockUserUpdateService::new().with_accept_terms(
        ADMIN.user.clone(),
        terms_version(),
        updated_user.clone(),
    );

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_update,
        ..Sut::default()
    };

    // Act
    let result = sut
        .accept_terms(
            &"token".into(),
            UserAcceptTermsRequest {
                terms_version: terms_version(),
                age_confirmed: true,
            },
        )
        .await;

    // Assert
    assert_eq!(result.unwrap().user, updated_user);
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
        .accept_terms(
            &"token".into(),
            UserAcceptTermsRequest {
                terms_version: terms_version(),
                age_confirmed: true,
            },
        )
        .await;

    // Assert
    assert_matches!(
        result,
        Err(UserAcceptTermsError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

/// Nothing is recorded unless the user confirms the minimum age, exactly as on
/// signup.
#[tokio::test]
async fn age_not_confirmed() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = UserFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .accept_terms(
            &"token".into(),
            UserAcceptTermsRequest {
                terms_version: terms_version(),
                age_confirmed: false,
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserAcceptTermsError::AgeNotConfirmed));
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, None);

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .accept_terms(
            &"token".into(),
            UserAcceptTermsRequest {
                terms_version: terms_version(),
                age_confirmed: true,
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(UserAcceptTermsError::NotFound));
}
