use academy_auth_contracts::MockAuthService;
use academy_core_premium_contracts::{
    PremiumFeatureService, PremiumPurchaseError, purchase::MockPremiumPurchaseService,
};
use academy_demo::{
    UUID1,
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    premium::{Premium, PremiumPlan, PremiumStatus},
};
use academy_persistence_contracts::{MockDatabase, premium::MockPremiumRepository};
use academy_utils::assert_matches;
use chrono::{TimeZone, Utc};

use crate::{PremiumFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = PremiumFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .purchase(&"token".into(), PremiumPlan::Monthly, false)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(PremiumPurchaseError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn email_not_verified() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = PremiumFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .purchase(&"token".into(), PremiumPlan::Monthly, false)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(PremiumPurchaseError::Auth(AuthError::Authorize(
            AuthorizeError::EmailVerified
        )))
    );
}

#[tokio::test]
async fn not_enough_coins() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let premium_purchase = MockPremiumPurchaseService::new().with_purchase(
        FOO.user.id,
        PremiumPlan::Monthly,
        Err(academy_core_premium_contracts::purchase::PremiumPurchaseError::NotEnoughCoins),
    );

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        premium_purchase,
        ..Sut::default()
    };

    // Act
    let result = sut
        .purchase(&"token".into(), PremiumPlan::Monthly, false)
        .await;

    // Assert
    assert_matches!(result, Err(PremiumPurchaseError::NotEnoughCoins));
}

#[tokio::test]
async fn no_subscribe() {
    // Arrange
    let expected = PremiumStatus {
        since: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
        subscription: None,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let premium_purchase = MockPremiumPurchaseService::new().with_purchase(
        FOO.user.id,
        PremiumPlan::Monthly,
        Ok(Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: expected.since,
            until: expected.until,
        }),
    );

    let premium_repo = MockPremiumRepository::new().with_get_subscription(FOO.user.id, None);

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        premium_purchase,
        premium_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .purchase(&"token".into(), PremiumPlan::Monthly, false)
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn subscribe() {
    // Arrange
    let expected = PremiumStatus {
        since: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
        subscription: Some(PremiumPlan::Monthly),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let premium_purchase = MockPremiumPurchaseService::new().with_purchase(
        FOO.user.id,
        PremiumPlan::Monthly,
        Ok(Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: expected.since,
            until: expected.until,
        }),
    );

    let premium_repo =
        MockPremiumRepository::new().with_set_subscription(FOO.user.id, Some(PremiumPlan::Monthly));

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        premium_purchase,
        premium_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .purchase(&"token".into(), PremiumPlan::Monthly, true)
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}
