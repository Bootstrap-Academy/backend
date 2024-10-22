use academy_auth_contracts::MockAuthService;
use academy_core_paypal_contracts::{
    coin_order::MockPaypalCoinOrderService, PaypalCreateCoinOrderError, PaypalFeatureService,
};
use academy_demo::{
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
};
use academy_extern_contracts::paypal::MockPaypalApiService;
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    paypal::PaypalCoinOrder,
};
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::{assert_matches, Apply};

use crate::{tests::Sut, PaypalFeatureServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = PaypalCoinOrder {
        id: "asdf1234".try_into().unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: 42,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let paypal_api = MockPaypalApiService::new().with_create_order(1337, Some(expected.id.clone()));

    let paypal_coin_order = MockPaypalCoinOrderService::new().with_create(expected.clone());

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        user_repo,
        paypal_api,
        paypal_coin_order,
        ..Sut::default()
    };

    // Act
    let result = sut.create_coin_order(&"token".into(), 1337).await;

    // Assert
    assert_eq!(result.unwrap(), expected.id);
}

#[tokio::test]
async fn amount_too_low() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut.create_coin_order(&"token".into(), 4).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCreateCoinOrderError::InvalidAmount(rng))
        if *rng == sut.config.purchase_range
    );
}

#[tokio::test]
async fn amount_too_high() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut.create_coin_order(&"token".into(), 5001).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCreateCoinOrderError::InvalidAmount(rng))
        if *rng == sut.config.purchase_range
    );
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = PaypalFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.create_coin_order(&"token".into(), 1337).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCreateCoinOrderError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = PaypalFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.create_coin_order(&"token".into(), 1337).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCreateCoinOrderError::Auth(AuthError::Authorize(
            AuthorizeError::EmailVerified
        )))
    );
}

#[tokio::test]
async fn incomplete_invoice_info() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(FOO.clone().with(|u| u.invoice_info.country = None)),
    );

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.create_coin_order(&"token".into(), 1337).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCreateCoinOrderError::IncompleteInvoiceInfo)
    );
}

#[tokio::test]
async fn create_order_failure() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let paypal_api = MockPaypalApiService::new().with_create_order(1337, None);

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        user_repo,
        paypal_api,
        ..Sut::default()
    };

    // Act
    let result = sut.create_coin_order(&"token".into(), 1337).await;

    // Assert
    assert_matches!(result, Err(PaypalCreateCoinOrderError::CreateOrderFailure));
}
