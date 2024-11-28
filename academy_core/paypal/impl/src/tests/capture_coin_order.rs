use academy_auth_contracts::MockAuthService;
use academy_core_finance_contracts::{
    coin::{CoinPrices, MockFinanceCoinService},
    invoice::MockFinanceInvoiceService,
};
use academy_core_paypal_contracts::{
    coin_order::MockPaypalCoinOrderService, PaypalCaptureCoinOrderError, PaypalFeatureService,
};
use academy_demo::{
    session::{BAR_1, FOO_1},
    user::{BAR, FOO},
};
use academy_email_contracts::template::MockTemplateEmailService;
use academy_extern_contracts::paypal::MockPaypalApiService;
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    coin::Balance,
    paypal::{PaypalCoinOrder, PaypalOrderId},
};
use academy_persistence_contracts::{
    paypal::MockPaypalRepository, user::MockUserRepository, MockDatabase,
};
use academy_templates_contracts::PurchaseConfirmationTemplate;
use academy_utils::{assert_matches, Apply};
use rust_decimal_macros::dec;

use crate::{tests::Sut, PaypalFeatureServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let order = PaypalCoinOrder {
        id: PaypalOrderId::try_new("asdf1234").unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: 42,
    };

    let expected = Balance {
        coins: 123456,
        withheld_coins: 7,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let paypal_api = MockPaypalApiService::new().with_capture_order(order.id.clone(), true);

    let paypal_coin_order = MockPaypalCoinOrderService::new().with_capture(order.clone(), expected);

    let pdf = vec![1, 2, 3, 4, 5];

    let prices = CoinPrices {
        net_unit: 1.into(),
        net_total: 2.into(),
        vat_total: 3.into(),
        gross_total: 4.into(),
    };
    let finance_coin = MockFinanceCoinService::new()
        .with_get_price(1337, prices)
        .with_vat_percent(19.into());
    let finance_invoice = MockFinanceInvoiceService::new().with_get_invoice_pdf(
        Some(FOO.user.id),
        42,
        Some(pdf.clone()),
    );

    let template_email = MockTemplateEmailService::new().with_send_purchase_confirmation_email(
        FOO.user
            .email
            .clone()
            .unwrap()
            .with_name(FOO.profile.display_name.clone().into_inner()),
        PurchaseConfirmationTemplate {
            coins: order.coins,
            vat_percent: dec!(19),
            vat_total: prices.vat_total,
            gross_total: prices.gross_total,
        },
        pdf.clone(),
        true,
    );

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        paypal_repo,
        user_repo,
        paypal_api,
        paypal_coin_order,
        template_email,
        finance_invoice,
        finance_coin,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order.id).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

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

    let paypal_repo = MockPaypalRepository::new().with_get_coin_order(order_id.clone(), None);

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
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

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
async fn already_captured() {
    // Arrange
    let order = PaypalCoinOrder {
        id: PaypalOrderId::try_new("asdf1234").unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: Some(FOO.user.last_login.unwrap()),
        coins: 1337,
        invoice_number: 42,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

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
async fn incomplete_invoice_info() {
    // Arrange
    let order = PaypalCoinOrder {
        id: PaypalOrderId::try_new("asdf1234").unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: 42,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

    let user_repo = MockUserRepository::new().with_get_composite(
        FOO.user.id,
        Some(FOO.clone().with(|u| u.invoice_info.country = None)),
    );

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        paypal_repo,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order.id).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCaptureCoinOrderError::IncompleteInvoiceInfo)
    );
}

#[tokio::test]
async fn capture_order_failure() {
    // Arrange
    let order = PaypalCoinOrder {
        id: PaypalOrderId::try_new("asdf1234").unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: 42,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let paypal_api = MockPaypalApiService::new().with_capture_order(order.id.clone(), false);

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        paypal_repo,
        user_repo,
        paypal_api,
        ..Sut::default()
    };

    // Act
    let result = sut.capture_coin_order(&"token".into(), order.id).await;

    // Assert
    assert_matches!(
        result,
        Err(PaypalCaptureCoinOrderError::CaptureOrderFailure)
    );
}
