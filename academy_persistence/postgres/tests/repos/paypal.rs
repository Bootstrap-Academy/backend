use academy_demo::user::FOO;
use academy_models::paypal::PaypalCoinOrder;
use academy_persistence_contracts::{paypal::PaypalRepository, Database, Transaction};
use academy_persistence_postgres::paypal::PostgresPaypalRepository;
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
