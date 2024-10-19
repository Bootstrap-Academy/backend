use academy_demo::user::FOO;
use academy_models::coin::Balance;
use academy_persistence_contracts::{coin::CoinRepository, Database, Transaction};
use academy_persistence_postgres::coin::PostgresCoinRepository;

use crate::common::setup;

const REPO: PostgresCoinRepository = PostgresCoinRepository;

#[tokio::test]
async fn balance() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);

    let balance = Balance {
        coins: 42,
        withheld_coins: 1337,
    };
    REPO.save_balance(&mut txn, FOO.user.id, balance)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(balance));

    let balance = Balance {
        coins: 1234,
        withheld_coins: 17,
    };
    REPO.save_balance(&mut txn, FOO.user.id, balance)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(balance));
}
