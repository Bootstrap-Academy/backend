use academy_demo::user::FOO;
use academy_models::coin::Balance;
use academy_persistence_contracts::{
    coin::{CoinRepoAddCoinsError, CoinRepository},
    Database, Transaction,
};
use academy_persistence_postgres::coin::PostgresCoinRepository;
use academy_utils::assert_matches;

use crate::common::setup;

const REPO: PostgresCoinRepository = PostgresCoinRepository;

#[tokio::test]
async fn get_balance_and_add_coins() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, balance(0, 0));

    let result = REPO
        .add_coins(&mut txn, FOO.user.id, 42, false)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(result, balance(42, 0));

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, balance(42, 0));

    let result = REPO
        .add_coins(&mut txn, FOO.user.id, 1337, true)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(result, balance(42, 1337));

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, balance(42, 1337));

    let result = REPO
        .add_coins(&mut txn, FOO.user.id, -35, false)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(result, balance(7, 1337));

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, balance(7, 1337));

    let result = REPO
        .add_coins(&mut txn, FOO.user.id, -1337, true)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(result, balance(7, 0));

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, balance(7, 0));
}

#[tokio::test]
async fn remove_coins_not_enough_coins() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.add_coins(&mut txn, FOO.user.id, -7, false).await;
    assert_matches!(result, Err(CoinRepoAddCoinsError::NotEnoughCoins));

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.add_coins(&mut txn, FOO.user.id, -7, true).await;
    assert_matches!(result, Err(CoinRepoAddCoinsError::NotEnoughCoins));

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .add_coins(&mut txn, FOO.user.id, 5, false)
        .await
        .unwrap();
    assert_eq!(result, balance(5, 0));
    let result = REPO
        .add_coins(&mut txn, FOO.user.id, 6, true)
        .await
        .unwrap();
    assert_eq!(result, balance(5, 6));
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.add_coins(&mut txn, FOO.user.id, -7, false).await;
    assert_matches!(result, Err(CoinRepoAddCoinsError::NotEnoughCoins));

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.add_coins(&mut txn, FOO.user.id, -7, true).await;
    assert_matches!(result, Err(CoinRepoAddCoinsError::NotEnoughCoins));
}

fn balance(coins: u64, withheld_coins: u64) -> Balance {
    Balance {
        coins,
        withheld_coins,
    }
}
