use academy_auth_contracts::MockAuthService;
use academy_core_finance_contracts::{
    coin::MockFinanceCoinService, invoice::MockFinanceInvoiceService,
};
use academy_core_paypal_contracts::coin_order::MockPaypalCoinOrderService;
use academy_core_purchase_contracts::MockPurchaseFeatureService;
use academy_extern_contracts::paypal::MockPaypalApiService;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, paypal::MockPaypalRepository, user::MockUserRepository,
};

use crate::{PaypalFeatureConfig, PaypalFeatureServiceImpl};

mod capture_coin_order;
mod create_coin_order;

type Sut = PaypalFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockPaypalApiService,
    MockUserRepository<MockTransaction>,
    MockPaypalRepository<MockTransaction>,
    MockPaypalCoinOrderService<MockTransaction>,
    MockPurchaseFeatureService,
    MockFinanceInvoiceService<MockTransaction>,
    MockFinanceCoinService,
>;

impl Default for PaypalFeatureConfig {
    fn default() -> Self {
        Self {
            purchase_range: 5..=5000,
        }
    }
}

#[tokio::test]
async fn smtp_false_never_acknowledges_receipt() {
    use academy_demo::user::FOO;
    use academy_models::{
        coin::Balance,
        paypal::{PaypalCapture, PaypalCoinOrder, PaypalPayment, PaypalPaymentSnapshot},
    };
    use chrono::Utc;
    use rust_decimal_macros::dec;
    let now = Utc::now();
    let payment = PaypalPayment {
        snapshot: PaypalPaymentSnapshot {
            provision_deadline: None,
            contract_order_id: None,
            order: PaypalCoinOrder {
                id: "ORDER1".try_into().unwrap(),
                user_id: FOO.user.id,
                created_at: now,
                captured_at: None,
                coins: 1337,
                invoice_number: 1,
                withdrawal_consent_at: None,
                withdrawal_text_version: None,
            },
            request_id: uuid::Uuid::new_v4(),
            merchant_id: "M".into(),
            currency: "EUR".into(),
            gross_total: dec!(13.37),
            net_unit: dec!(0.0084),
            net_total: dec!(11.24),
            vat_total: dec!(2.13),
            vat_percent: dec!(19),
            customer_details: vec!["Synthetic".into()],
            recipient: "synthetic@example.com".parse().unwrap(),
            consent_text: "Synthetic".into(),
        },
        started_at: Some(now),
        attempts: 1,
        capture: Some(PaypalCapture {
            id: "CAP1".into(),
            status: "COMPLETED".into(),
            currency: "EUR".into(),
            amount: dec!(13.37),
            created_at: now,
        }),
        balance: Some(Balance {
            coins: 1337,
            withheld_coins: 0,
        }),
        fulfilled_at: Some(now),
        receipt_sent_at: None,
        receipt_attempts: 0,
        last_error: None,
    };
    let mut db = MockDatabase::new();
    let mut txn = MockTransaction::new();
    txn.expect_commit()
        .once()
        .return_once(|| Box::pin(std::future::ready(Ok(()))));
    db.expect_begin_transaction()
        .once()
        .return_once(|| Box::pin(std::future::ready(Ok(txn))));
    let mut artifact_txn = MockTransaction::new();
    artifact_txn
        .expect_commit()
        .once()
        .return_once(|| Box::pin(std::future::ready(Ok(()))));
    db.expect_begin_transaction()
        .once()
        .return_once(|| Box::pin(std::future::ready(Ok(artifact_txn))));
    db.expect_begin_transaction()
        .once()
        .return_once(|| Box::pin(std::future::ready(Ok(MockTransaction::new()))));
    let id = payment.snapshot.order.id.clone();
    let mut paypal_repo = MockPaypalRepository::new();
    paypal_repo
        .expect_get_payment()
        .times(3)
        .returning(move |_, _| Box::pin(std::future::ready(Ok(Some(payment.clone())))));
    paypal_repo
        .expect_update_payment()
        .once()
        .return_once(|_, p| {
            assert_eq!(p.receipt_attempts, 1);
            assert!(p.receipt_sent_at.is_none());
            Box::pin(std::future::ready(Ok(())))
        });
    let mut finance_invoice = MockFinanceInvoiceService::new();
    finance_invoice
        .expect_render_payment_invoice()
        .once()
        .return_once(|_, _| Box::pin(std::future::ready(Ok(vec![1]))));
    let mut purchase = MockPurchaseFeatureService::new();
    purchase
        .expect_cash_receipt()
        .once()
        .return_once(|_, _| Box::pin(std::future::ready(Ok(false))));
    let sut = Sut {
        db,
        paypal_repo,
        finance_invoice,
        purchase,
        ..Sut::default()
    };
    assert!(
        sut.deliver_receipt(&id)
            .await
            .unwrap_err()
            .to_string()
            .contains("SMTP did not accept")
    );
}
