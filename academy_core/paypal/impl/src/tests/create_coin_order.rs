use crate::tests::Sut;
use academy_auth_contracts::MockAuthService;
use academy_core_paypal_contracts::{PaypalCreateCoinOrderError, PaypalFeatureService};
use academy_core_purchase_contracts::MockPurchaseFeatureService;
use academy_demo::{session::FOO_1, user::FOO};
use academy_models::{
    auth::{AuthError, AuthenticateError},
    paypal::PaypalRemoteOrder,
    purchase::*,
};
use academy_persistence_contracts::{MockDatabase, paypal::MockPaypalRepository};
use chrono::{TimeDelta, Utc};
use rust_decimal_macros::dec;
use serde_json::json;
use uuid::Uuid;

fn accepted() -> PurchaseStatus {
    let now = Utc::now() - TimeDelta::days(100);
    PurchaseStatus {
        offer: PurchaseOffer {
            id: Uuid::new_v4(),
            user_id: *FOO.user.id,
            source: "paypal".into(),
            created_at: now,
            expires_at: now + TimeDelta::minutes(20),
            recipient: "Original <original@example.invalid>".into(),
            product: PurchaseProduct {
                kind: "coins".into(),
                reference: "1337".into(),
                title: "1337 MorphCoins".into(),
                description: "Accepted historical offer".into(),
                coins: 1337,
                facts: json!({"gross_total":"13.37","net_unit":"0.0084","net_total":"11.24","vat_total":"2.13","vat_percent":"19","customer_details":["Original customer"]}),
                revision: "original".into(),
                service_starts_at: None,
            },
            document_hash: "original-pdfs".into(),
            hash: "accepted-offer-hash".into(),
            text: "Original contract text".into(),
            declaration: "Original explicit request".into(),
            provision_window_seconds: None,
            summary: None,
            acceptance: None,
            terms_version: None,
        },
        state: "awaiting_payment".into(),
        accepted_at: Some(now),
        confirmation_smtp_accepted_at: None,
        fulfillment: None,
        financial_evidence: None,
        review_reason: None,
        provision_deadline: None,
        provision_timing: None,
        document_corrections: Vec::new(),
        refusal: None,
    }
}
fn request(s: &PurchaseStatus) -> PurchaseAcceptance {
    PurchaseAcceptance {
        order_id: s.offer.id,
        offer_hash: s.offer.hash.clone(),
        accepted: true,
        early_performance_requested: true,
        declaration_language: None,
    }
}
fn purchase(status: &PurchaseStatus) -> MockPurchaseFeatureService {
    let mut service = MockPurchaseFeatureService::new();
    let s = status.clone();
    service
        .expect_get()
        .once()
        .return_once(move |_, _| Box::pin(std::future::ready(Ok(s))));
    let s = status.clone();
    let a = request(status);
    service
        .expect_cash_accept()
        .once()
        .return_once(move |_, given| {
            assert_eq!(given, a);
            Box::pin(std::future::ready(Ok(s)))
        });
    service
}
#[tokio::test]
async fn accepted_retry_returns_original_provider_identity_without_repricing() {
    let status = accepted();
    let a = request(&status);
    let mut repo = MockPaypalRepository::new();
    repo.expect_lock_contract_order()
        .once()
        .return_once(|_, _| Box::pin(std::future::ready(Ok(Some("ORIGINAL".try_into().unwrap())))));
    let sut = Sut {
        auth: MockAuthService::new()
            .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
            .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone()))),
        db: MockDatabase::build(true),
        paypal_repo: repo,
        purchase: purchase(&status),
        ..Sut::default()
    };
    assert_eq!(
        sut.create_coin_order(&"token".into(), 1337, a)
            .await
            .unwrap()
            .as_str(),
        "ORIGINAL"
    );
}

#[tokio::test]
async fn accepted_purchase_still_requires_current_session_before_provider_order_creation() {
    let status = accepted();
    let acceptance = request(&status);
    let mut repo = MockPaypalRepository::new();
    repo.expect_lock_contract_order().never();
    let mut api = academy_extern_contracts::paypal::MockPaypalApiService::new();
    api.expect_create_order().never();
    api.expect_get_order().never();
    let sut = Sut {
        auth: MockAuthService::new()
            .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
            .with_authenticate_in_transaction(None),
        db: MockDatabase::build(false),
        purchase: purchase(&status),
        paypal_repo: repo,
        paypal_api: api,
        ..Sut::default()
    };

    assert!(matches!(
        sut.create_coin_order(&"token".into(), 1337, acceptance)
            .await,
        Err(PaypalCreateCoinOrderError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    ));
    // Both purchase mocks must complete successfully. The subsequent PayPal
    // transaction still needs current authority before any order effect or commit.
}
#[tokio::test]
async fn amount_from_another_offer_cannot_create_or_replay_payment() {
    let status = accepted();
    let a = request(&status);
    let mut purchase = MockPurchaseFeatureService::new();
    purchase
        .expect_get()
        .once()
        .return_once(move |_, _| Box::pin(std::future::ready(Ok(status))));
    let sut = Sut {
        auth: MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone()))),
        purchase,
        ..Sut::default()
    };
    assert!(matches!(
        sut.create_coin_order(&"token".into(), 1999, a).await,
        Err(PaypalCreateCoinOrderError::OfferChanged)
    ));
}
#[tokio::test]
async fn accepted_offer_creates_snapshot_from_original_tax_recipient_and_declaration_time() {
    creates_order(accepted(), true).await;
}

#[tokio::test]
async fn a_coin_offer_of_stage_two_records_no_request_for_an_early_start() {
    // Terms 2026-10: the order button accepts, and a coin purchase asks for no early start.
    let mut status = accepted();
    status.offer.acceptance = Some(academy_models::purchase::OfferAcceptance::Button);
    status.offer.declaration = String::new();
    creates_order(status, false).await;
}

/// Creates the provider order of the accepted `status`; `consent` says whether the order
/// records the request for an early start of its declaration.
async fn creates_order(status: PurchaseStatus, consent: bool) {
    let a = request(&status);
    let id = status.offer.id;
    let at = status.accepted_at;
    let declaration = status.offer.declaration.clone();
    let mut repo = MockPaypalRepository::new();
    repo.expect_lock_contract_order()
        .once()
        .return_once(|_, _| Box::pin(std::future::ready(Ok(None))));
    repo.expect_get_next_invoice_number()
        .once()
        .return_once(|_| Box::pin(std::future::ready(Ok(42))));
    repo.expect_create_coin_order()
        .once()
        .return_once(move |_, o| {
            assert_eq!(o.withdrawal_consent_at, at.filter(|_| consent));
            assert_eq!(
                o.withdrawal_text_version.as_ref().map(|v| v.to_string()),
                consent.then(|| "L1-request-2026-09".to_owned())
            );
            assert_eq!(o.invoice_number, 42);
            assert_eq!(o.coins, 1337);
            Box::pin(std::future::ready(Ok(())))
        });
    repo.expect_create_payment()
        .once()
        .return_once(move |_, p| {
            assert_eq!(p.snapshot.contract_order_id, Some(id));
            assert_eq!(p.snapshot.customer_details, vec!["Original customer"]);
            assert_eq!(p.snapshot.gross_total, dec!(13.37));
            assert_eq!(p.snapshot.vat_percent, dec!(19));
            assert_eq!(p.snapshot.consent_text, declaration);
            assert_eq!(
                p.snapshot.recipient.0.to_string(),
                "Original <original@example.invalid>"
            );
            Box::pin(std::future::ready(Ok(())))
        });
    let mut api = academy_extern_contracts::paypal::MockPaypalApiService::new()
        .with_create_order(1337, Some("CREATED".try_into().unwrap()));
    api.expect_get_order().once().return_once(|_| {
        Box::pin(std::future::ready(Ok(PaypalRemoteOrder {
            id: "CREATED".try_into().unwrap(),
            intent: "CAPTURE".into(),
            status: "CREATED".into(),
            merchant_id: "Merchant".into(),
            currency: "EUR".into(),
            amount: dec!(13.37),
            captures: vec![],
        })))
    });
    let sut = Sut {
        auth: MockAuthService::new()
            .with_authenticate(Some((FOO.user.clone(), FOO_1.clone())))
            .with_authenticate_in_transaction(Some((FOO.user.clone(), FOO_1.clone()))),
        db: MockDatabase::build(true),
        paypal_repo: repo,
        purchase: purchase(&status),
        paypal_api: api,
        ..Sut::default()
    };
    assert_eq!(
        sut.create_coin_order(&"token".into(), 1337, a)
            .await
            .unwrap()
            .as_str(),
        "CREATED"
    );
}

#[test]
fn coin_product_is_short_and_keeps_invoice_details_in_its_facts() {
    use academy_core_finance_contracts::coin::{CoinPrices, MockFinanceCoinService};
    let prices = CoinPrices {
        net_unit: dec!(0.0084),
        net_total: dec!(8.4034),
        vat_total: dec!(1.5966),
        gross_total: dec!(10.00),
    };
    let sut = Sut {
        finance_coin: MockFinanceCoinService::new()
            .with_get_price(1000, prices)
            .with_vat_percent(dec!(19)),
        ..Sut::default()
    };
    let product = sut.coin_product(1000, &FOO);
    assert_eq!(product.kind, "coins");
    assert_eq!(product.title, "1.000 Morphcoins");
    // The order page states the invoice details once; the offer text does not repeat them.
    assert_eq!(
        product.description,
        "Guthaben für Angebote auf Bootstrap Academy, einmaliger Kauf."
    );
    assert_eq!(product.facts["gross_total"], "10.00");
    assert_eq!(product.facts["vat_percent"], "19");
    assert!(product.facts["customer_details"].is_array());
}
