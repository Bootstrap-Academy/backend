use academy_auth_contracts::MockAuthService;
use academy_core_user_contracts::{
    UserDeclineTermsError, UserFeatureService, update::MockUserUpdateService,
};
use academy_demo::{session::FOO_1, user::FOO};
use academy_models::{
    auth::{AuthError, AuthenticateError},
    user::{User, UserComposite},
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::assert_matches;
use chrono::{TimeZone, Utc};

use crate::{UserFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let now = Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap();

    // The version the user accepted before survives the refusal.
    let updated_user = User {
        terms_declined_at: Some(now),
        ..FOO.user.clone()
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let user_update =
        MockUserUpdateService::new().with_decline_terms(FOO.user.clone(), updated_user.clone());

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_update,
        ..Sut::default()
    };

    // Act
    let result = sut.decline_terms(&"token".into()).await;

    // Assert
    let user = result.unwrap().user;
    assert_eq!(user, updated_user);
    assert_eq!(user.terms_version, FOO.user.terms_version);
    assert_eq!(user.terms_accepted_at, FOO.user.terms_accepted_at);
}

/// Declining a second time is not an error and only moves the timestamp
/// forward, so the gate can be dismissed again on the next visit.
#[tokio::test]
async fn ok_repeated() {
    // Arrange
    let first = Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap();

    let declined = UserComposite {
        user: User {
            terms_declined_at: Some(first),
            ..FOO.user.clone()
        },
        ..FOO.clone()
    };

    let updated_user = User {
        terms_declined_at: Some(now),
        ..FOO.user.clone()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((declined.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo =
        MockUserRepository::new().with_get_composite(FOO.user.id, Some(declined.clone()));

    let user_update = MockUserUpdateService::new()
        .with_decline_terms(declined.user.clone(), updated_user.clone());

    let sut = UserFeatureServiceImpl {
        auth,
        db,
        user_repo,
        user_update,
        ..Sut::default()
    };

    // Act
    let result = sut.decline_terms(&"token".into()).await;

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
    let result = sut.decline_terms(&"token".into()).await;

    // Assert
    assert_matches!(
        result,
        Err(UserDeclineTermsError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
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
    let result = sut.decline_terms(&"token".into()).await;

    // Assert
    assert_matches!(result, Err(UserDeclineTermsError::NotFound));
}
