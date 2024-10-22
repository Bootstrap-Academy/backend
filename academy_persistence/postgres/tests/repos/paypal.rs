use academy_demo::user::FOO;
use academy_models::paypal::PaypalCoinOrder;
use academy_persistence_contracts::{paypal::PaypalRepository, Database, Transaction};
use academy_persistence_postgres::paypal::PostgresPaypalRepository;

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
}

#[tokio::test]
async fn get_next_invoice_number() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 1);

    let order = |num| PaypalCoinOrder {
        id: format!("asdf1234-{num}").try_into().unwrap(),
        user_id: FOO.user.id,
        created_at: FOO.user.created_at,
        captured_at: None,
        coins: 1337,
        invoice_number: num,
    };

    REPO.create_coin_order(&mut txn, &order(42)).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 43);

    REPO.create_coin_order(&mut txn, &order(1337))
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 1338);

    REPO.create_coin_order(&mut txn, &order(100)).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get_next_invoice_number(&mut txn).await.unwrap(), 1338);
}
