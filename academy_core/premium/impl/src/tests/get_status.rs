use academy_auth_contracts::MockAuthService;
use academy_core_premium_contracts::{
    premium::MockPremiumService, PremiumFeatureService, PremiumGetStatusError,
};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
    UUID1,
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    premium::{Premium, PremiumPlan, PremiumStatus},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    premium::MockPremiumRepository, user::MockUserRepository, MockDatabase,
};
use academy_utils::assert_matches;
use chrono::{TimeZone, Utc};

use crate::{tests::Sut, PremiumFeatureServiceImpl};

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = PremiumFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.get_status(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_matches!(
        result,
        Err(PremiumGetStatusError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = PremiumFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .get_status(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    assert_matches!(
        result,
        Err(PremiumGetStatusError::Auth(AuthError::Authorize(
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

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .get_status(&"token".into(), UserIdOrSelf::UserId(FOO.user.id))
        .await;

    // Assert
    assert_matches!(result, Err(PremiumGetStatusError::NotFound));
}

#[tokio::test]
async fn no_premium() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let premium = MockPremiumService::new().with_get_active(FOO.user.id, None);

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        user_repo,
        premium,
        ..Sut::default()
    };

    // Act
    let result = sut.get_status(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), None);
}

#[tokio::test]
async fn no_subscription() {
    // Arrange
    let expected = PremiumStatus {
        since: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
        subscription: None,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let premium = MockPremiumService::new().with_get_active(
        FOO.user.id,
        Some(Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: expected.since,
            until: expected.until,
        }),
    );

    let premium_repo =
        MockPremiumRepository::new().with_get_subscription(FOO.user.id, expected.subscription);

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        user_repo,
        premium,
        premium_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get_status(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), Some(expected));
}

#[tokio::test]
async fn with_subscription() {
    // Arrange
    let expected = PremiumStatus {
        since: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
        subscription: Some(PremiumPlan::Monthly),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let premium = MockPremiumService::new().with_get_active(
        FOO.user.id,
        Some(Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: expected.since,
            until: expected.until,
        }),
    );

    let premium_repo =
        MockPremiumRepository::new().with_get_subscription(FOO.user.id, expected.subscription);

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        user_repo,
        premium,
        premium_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get_status(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), Some(expected));
}
