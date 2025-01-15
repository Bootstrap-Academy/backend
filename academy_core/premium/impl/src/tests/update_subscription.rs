use academy_auth_contracts::MockAuthService;
use academy_core_premium_contracts::{
    premium::MockPremiumService, PremiumFeatureService, PremiumUpdateSubscriptionError,
};
use academy_demo::{
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
    UUID1,
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    premium::{Premium, PremiumPlan},
};
use academy_persistence_contracts::{premium::MockPremiumRepository, MockDatabase};
use academy_utils::assert_matches;

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
    let result = sut
        .update_subscription(&"token".into(), Some(PremiumPlan::Yearly))
        .await;

    // Assert
    assert_matches!(
        result,
        Err(PremiumUpdateSubscriptionError::Auth(
            AuthError::Authenticate(AuthenticateError::InvalidToken)
        ))
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
        .update_subscription(&"token".into(), Some(PremiumPlan::Yearly))
        .await;

    // Assert
    assert_matches!(
        result,
        Err(PremiumUpdateSubscriptionError::Auth(AuthError::Authorize(
            AuthorizeError::EmailVerified
        )))
    );
}

#[tokio::test]
async fn no_premium() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let premium = MockPremiumService::new().with_get_active(FOO.user.id, None);

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        premium,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_subscription(&"token".into(), Some(PremiumPlan::Yearly))
        .await;

    // Assert
    assert_matches!(result, Err(PremiumUpdateSubscriptionError::NoPremium));
}

#[tokio::test]
async fn ok() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let premium = MockPremiumService::new().with_get_active(
        FOO.user.id,
        Some(Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: FOO.user.created_at,
            until: FOO.user.last_login.unwrap(),
        }),
    );

    let premium_repo =
        MockPremiumRepository::new().with_set_subscription(FOO.user.id, Some(PremiumPlan::Yearly));

    let sut = PremiumFeatureServiceImpl {
        auth,
        db,
        premium,
        premium_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .update_subscription(&"token".into(), Some(PremiumPlan::Yearly))
        .await;

    // Assert
    result.unwrap();
}
