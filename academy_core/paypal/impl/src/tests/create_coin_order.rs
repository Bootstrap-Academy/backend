use crate::tests::Sut;
use academy_auth_contracts::MockAuthService;
use academy_core_paypal_contracts::{PaypalCreateCoinOrderError, PaypalFeatureService};
use academy_core_purchase_contracts::MockPurchaseFeatureService;
use academy_demo::{session::FOO_1, user::FOO};
use academy_models::{paypal::PaypalRemoteOrder, purchase::*};
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
    }
}
fn request(s: &PurchaseStatus) -> PurchaseAcceptance {
    PurchaseAcceptance {
        order_id: s.offer.id,
        offer_hash: s.offer.hash.clone(),
        accepted: true,
        early_performance_requested: true,
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
        auth: MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone()))),
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
    let status = accepted();
    let a = request(&status);
    let id = status.offer.id;
    let at = status.accepted_at;
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
            assert_eq!(o.withdrawal_consent_at, at);
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
        auth: MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone()))),
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
