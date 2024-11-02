use academy_auth_contracts::MockAuthService;
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
use academy_render_contracts::pdf::MockRenderPdfService;
use academy_shared_contracts::time::MockTimeService;
use academy_templates_contracts::{
    InvoiceItem, InvoiceTemplate, MockTemplateService, PurchaseConfirmationTemplate, LOGO_BASE64,
};
use academy_utils::{assert_matches, Apply};
use rust_decimal::Decimal;
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

    let timestamp = Default::default();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(true);

    let time = MockTimeService::new().with_now(timestamp);

    let paypal_repo =
        MockPaypalRepository::new().with_get_coin_order(order.id.clone(), Some(order.clone()));

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let paypal_api = MockPaypalApiService::new().with_capture_order(order.id.clone(), true);

    let paypal_coin_order = MockPaypalCoinOrderService::new().with_capture(order.clone(), expected);

    let template = MockTemplateService::new().with_render(
        InvoiceTemplate {
            logo_base64: &LOGO_BASE64,
            title: "Rechnung",
            customer_details: FOO
                .invoice_info
                .clone()
                .into_details(Some(FOO.profile.display_name.clone().into_inner())),
            timestamp,
            invoice_number: "R0000042".into(),
            items: vec![InvoiceItem {
                description: "MorphCoins".into(),
                net_unit: dec!(0.01) / dec!(1.19),
                count: order.coins,
                net_total: dec!(0.01) / dec!(1.19) * Decimal::from(order.coins),
            }],
            vat_percent: dec!(19),
            net_total: dec!(0.01) / dec!(1.19) * Decimal::from(order.coins),
            vat_total: dec!(0.01) / dec!(1.19) * Decimal::from(order.coins) * dec!(0.19),
            gross_total: dec!(0.01) * Decimal::from(order.coins),
        },
        "invoice-template-html".into(),
    );

    let pdf = vec![1, 2, 3, 4, 5];
    let render_pdf =
        MockRenderPdfService::new().with_render("invoice-template-html".into(), pdf.clone());

    let template_email = MockTemplateEmailService::new().with_send_purchase_confirmation_email(
        FOO.user
            .email
            .clone()
            .unwrap()
            .with_name(FOO.profile.display_name.clone().into_inner()),
        PurchaseConfirmationTemplate {
            coins: order.coins,
            vat_percent: dec!(19),
            vat_total: dec!(0.01) / dec!(1.19) * Decimal::from(order.coins) * dec!(0.19),
            gross_total: dec!(0.01) * Decimal::from(order.coins),
        },
        pdf,
        true,
    );

    let sut = PaypalFeatureServiceImpl {
        auth,
        db,
        time,
        paypal_repo,
        user_repo,
        paypal_api,
        paypal_coin_order,
        template,
        render_pdf,
        template_email,
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
