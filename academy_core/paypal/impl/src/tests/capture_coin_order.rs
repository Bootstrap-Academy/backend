use crate::{PaypalFeatureServiceImpl, tests::Sut};
use academy_auth_contracts::MockAuthService;
use academy_core_paypal_contracts::{PaypalCaptureCoinOrderError, PaypalFeatureService};
use academy_demo::{
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    paypal::{PaypalCoinOrder, PaypalOrderId},
};
use academy_persistence_contracts::{MockDatabase, paypal::MockPaypalRepository};
use academy_utils::assert_matches;

// Financial success/failure/replay cases use real PostgreSQL and the HTTP/SMTP boundaries in
// tests/paypal-recovery.py. These unit checks isolate authorization and legacy handling.
#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let order_id = PaypalOrderId::try_new("asdf1234").unwrap();

    let auth = MockAuthService::new().with_authenticate(None);

    let sut = PaypalFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order_id).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCaptureCoinOrderError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let order_id = PaypalOrderId::try_new("asdf1234").unwrap();

    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = PaypalFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order_id).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCaptureCoinOrderError::Auth(AuthError::Authorize(
            AuthorizeError::EmailVerified
        )))
    );
}

#[tokio::test]
async fn order_not_found() {
    // Arrange
    let order_id = PaypalOrderId::try_new("asdf1234").unwrap();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let mut paypal_repo = MockPaypalRepository::new().with_get_coin_order(order_id.clone(), None);

    paypal_repo
        .expect_get_payment()
        .once()
        .return_once(|_, _| Box::pin(std::future::ready(Ok(None))));

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        paypal_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order_id).await;

    // Assert
    assert_matches!(result, Err(PaypalCaptureCoinOrderError::NotFound));
}

#[tokio::test]
async fn different_user() {
    // Arrange
    let order = PaypalCoinOrder {
        id: PaypalOrderId::try_new("asdf1234").unwrap(),
        user_id: BAR.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: 42,
        withdrawal_consent_at: Some(FOO.user.created_at),
        withdrawal_text_version: Some("2026-09".try_into().unwrap()),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let mut paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

    paypal_repo
        .expect_get_payment()
        .once()
        .return_once(|_, _| Box::pin(std::future::ready(Ok(None))));

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        paypal_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order.id).await;

    // Assert
    assert_matches!(result, Err(PaypalCaptureCoinOrderError::NotFound));
}

#[tokio::test]
async fn legacy_captured_requires_reconciliation() {
    // Arrange
    let order = PaypalCoinOrder {
        id: PaypalOrderId::try_new("asdf1234").unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: Some(FOO.user.last_login.unwrap()),
        coins: 1337,
        invoice_number: 42,
        withdrawal_consent_at: Some(FOO.user.created_at),
        withdrawal_text_version: Some("2026-09".try_into().unwrap()),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let mut paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

    paypal_repo
        .expect_get_payment()
        .once()
        .return_once(|_, _| Box::pin(std::future::ready(Ok(None))));

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        paypal_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order.id).await;

    // Assert
    assert_matches!(result, Err(PaypalCaptureCoinOrderError::Pending));
}
