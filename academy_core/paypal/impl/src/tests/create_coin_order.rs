use academy_auth_contracts::MockAuthService;
use academy_core_paypal_contracts::{
    PaypalCreateCoinOrderError, PaypalFeatureService, coin_order::MockPaypalCoinOrderService,
};
use academy_demo::{
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
};
use academy_extern_contracts::paypal::MockPaypalApiService;
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    paypal::PaypalCoinOrder,
    withdrawal::WithdrawalConsentDeclaration,
};
use academy_persistence_contracts::{MockDatabase, user::MockUserRepository};
use academy_utils::{Apply, assert_matches};

use crate::{PaypalFeatureServiceImpl, tests::Sut};

fn declaration() -> WithdrawalConsentDeclaration {
    WithdrawalConsentDeclaration {
        given: true,
        text_version: Some("2026-09".try_into().unwrap()),
    }
}

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
        withdrawal_consent_at: Some(FOO.user.created_at),
        withdrawal_text_version: Some("2026-09".try_into().unwrap()),
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let paypal_api = MockPaypalApiService::new().with_create_order(1337, Some(expected.id.clone()));

    let paypal_coin_order = MockPaypalCoinOrderService::new()
        .with_create("2026-09".try_into().unwrap(), expected.clone());

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        user_repo,
        paypal_api,
        paypal_coin_order,
        ..Sut::default()
    };

    // Act
    let result = sut
        .create_coin_order(&"token".into(), 1337, declaration())
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected.id);
}

#[tokio::test]
async fn amount_too_low() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut
        .create_coin_order(&"token".into(), 4, declaration())
        .await;

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
    let result = sut
        .create_coin_order(&"token".into(), 5001, declaration())
        .await;

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
    let result = sut
        .create_coin_order(&"token".into(), 1337, declaration())
        .await;

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
    let result = sut
        .create_coin_order(&"token".into(), 1337, declaration())
        .await;

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
    let result = sut
        .create_coin_order(&"token".into(), 1337, declaration())
        .await;

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
    let result = sut
        .create_coin_order(&"token".into(), 1337, declaration())
        .await;

    // Assert
    assert_matches!(result, Err(PaypalCreateCoinOrderError::CreateOrderFailure));
}

#[tokio::test]
async fn withdrawal_consent_missing() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut
        .create_coin_order(
            &"token".into(),
            1337,
            WithdrawalConsentDeclaration {
                given: false,
                ..declaration()
            },
        )
        .await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCreateCoinOrderError::WithdrawalConsentMissing)
    );
}

#[tokio::test]
async fn withdrawal_text_version_missing() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut
        .create_coin_order(
            &"token".into(),
            1337,
            WithdrawalConsentDeclaration {
                given: true,
                text_version: None,
            },
        )
        .await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCreateCoinOrderError::WithdrawalConsentMissing)
    );
}
