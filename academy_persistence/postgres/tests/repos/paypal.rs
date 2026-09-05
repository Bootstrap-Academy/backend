use academy_demo::user::{BAR, FOO};
use academy_models::paypal::PaypalCoinOrder;
use academy_persistence_contracts::{Database, Transaction, paypal::PaypalRepository};
use academy_persistence_postgres::paypal::PostgresPaypalRepository;
use chrono::{TimeZone, Utc};
use futures::StreamExt;

use crate::common::setup;

const REPO: PostgresPaypalRepository = PostgresPaypalRepository;

#[tokio::test]
async fn get_create_capture() {
    let db = setup().await;

    let mut order = PaypalCoinOrder {
        id: "asdf1234".try_into().unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: 42,
        withdrawal_consent_at: Some(FOO.user.created_at),
        withdrawal_text_version: Some("2026-09".try_into().unwrap()),
    };

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get_coin_order(&mut txn, &order.id).await.unwrap(),
        None
    );
    assert_eq!(REPO.count_coin_orders(&mut txn).await.unwrap(), 0);

    REPO.create_coin_order(&mut txn, &order).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get_coin_order(&mut txn, &order.id)
            .await
            .unwrap()
            .unwrap(),
        order
    );

    order.captured_at = Some(FOO.user.last_login.unwrap());
    REPO.capture_coin_order(&mut txn, &order.id, order.captured_at.unwrap())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get_coin_order(&mut txn, &order.id)
            .await
            .unwrap()
            .unwrap(),
        order
    );

    assert_eq!(REPO.count_coin_orders(&mut txn).await.unwrap(), 1);
    let mut stream = std::pin::pin!(REPO.stream_coin_orders(&mut txn));
    let result = stream.next().await.unwrap().unwrap();
    assert_eq!(result, order);
    assert!(stream.next().await.is_none());
}

/// The export of a user contains their coin orders, oldest first.
#[tokio::test]
async fn list_coin_orders_by_user_id() {
    let db = setup().await;

    let first = PaypalCoinOrder {
        id: "first".try_into().unwrap(),
        user_id: FOO.user.id,
        created_at: Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap(),
        captured_at: None,
        coins: 1337,
        invoice_number: 1,
        withdrawal_consent_at: None,
        withdrawal_text_version: None,
    };
    let second = PaypalCoinOrder {
        id: "second".try_into().unwrap(),
        created_at: Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap(),
        captured_at: Some(Utc.with_ymd_and_hms(2026, 9, 4, 12, 1, 0).unwrap()),
        invoice_number: 2,
        withdrawal_consent_at: Some(Utc.with_ymd_and_hms(2026, 9, 4, 11, 59, 0).unwrap()),
        withdrawal_text_version: Some("2026-09".try_into().unwrap()),
        ..first.clone()
    };
    let other = PaypalCoinOrder {
        id: "other".try_into().unwrap(),
        user_id: BAR.user.id,
        invoice_number: 3,
        ..first.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_coin_orders_by_user_id(&mut txn, FOO.user.id)
            .await
            .unwrap(),
        []
    );

    // inserted in the wrong order to cover the ordering of the query
    REPO.create_coin_order(&mut txn, &second).await.unwrap();
    REPO.create_coin_order(&mut txn, &first).await.unwrap();
    REPO.create_coin_order(&mut txn, &other).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_coin_orders_by_user_id(&mut txn, FOO.user.id)
            .await
            .unwrap(),
        [first, second]
    );
    assert_eq!(
        REPO.list_coin_orders_by_user_id(&mut txn, BAR.user.id)
            .await
            .unwrap(),
        [other]
    );
}

#[tokio::test]
async fn get_next_invoice_number() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 1);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 2);
    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 3);
    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 4);
}
