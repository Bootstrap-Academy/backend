use academy_auth_contracts::MockAuthService;
use academy_core_premium_contracts::{PremiumFeatureService, PremiumUpdateSubscriptionError};
use academy_demo::{
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    premium::PremiumPlan,
};
use academy_persistence_contracts::{MockDatabase, premium::MockPremiumRepository};
use academy_utils::assert_matches;

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
        .update_subscription(&"token".into(), Some(PremiumPlan::Yearly), None)
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
        .update_subscription(&"token".into(), Some(PremiumPlan::Yearly), None)
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
async fn no_consent_cannot_enable_monthly_or_yearly() {
    for plan in [PremiumPlan::Monthly, PremiumPlan::Yearly] {
        let sut = PremiumFeatureServiceImpl {
            auth: MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone()))),
            ..Sut::default()
        };
        assert_matches!(
            sut.update_subscription(&"token".into(), Some(plan), None)
                .await,
            Err(PremiumUpdateSubscriptionError::RenewalConsentRequired)
        );
    }
}

#[tokio::test]
async fn cancellation_never_calls_charge_on_read_even_without_premium() {
    let sut = PremiumFeatureServiceImpl {
        auth: MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone()))),
        db: MockDatabase::build(true),
        premium_repo: MockPremiumRepository::new().with_set_subscription(FOO.user.id, None),
        ..Sut::default()
    };
    sut.update_subscription(&"token".into(), None, None)
        .await
        .unwrap();
}
