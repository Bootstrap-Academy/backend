use academy_demo::{user::FOO, UUID1, UUID2};
use academy_models::coin::{Balance, Transaction};
use academy_persistence_contracts::{
    coin::{CoinRepoAddCoinsError, CoinRepository},
    Database, Transaction as _,
};
use academy_persistence_postgres::coin::PostgresCoinRepository;
use academy_utils::assert_matches;
use chrono::{TimeZone, Utc};

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

#[tokio::test]
async fn release_coins() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO.add_coins(&mut txn, FOO.user.id, 42, true).await;
    assert_eq!(result.unwrap(), balance(0, 42));

    REPO.release_coins(&mut txn, FOO.user.id).await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await;
    assert_eq!(result.unwrap(), balance(42, 0));

    let result = REPO.add_coins(&mut txn, FOO.user.id, 1337, true).await;
    assert_eq!(result.unwrap(), balance(42, 1337));

    REPO.release_coins(&mut txn, FOO.user.id).await.unwrap();
    let result = REPO.get_balance(&mut txn, FOO.user.id).await;
    assert_eq!(result.unwrap(), balance(1379, 0));
}

#[tokio::test]
async fn transactions() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let d1 = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let d2 = Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap();
    let d3 = Utc.with_ymd_and_hms(2024, 1, 3, 0, 0, 0).unwrap();
    let d4 = Utc.with_ymd_and_hms(2024, 1, 4, 0, 0, 0).unwrap();
    let d5 = Utc.with_ymd_and_hms(2024, 1, 5, 0, 0, 0).unwrap();

    let t1 = Transaction {
        id: UUID1.into(),
        user_id: FOO.user.id,
        coins: 42,
        description: None,
        created_at: d2,
        include_in_credit_note: true,
    };
    let t2 = Transaction {
        id: UUID2.into(),
        user_id: FOO.user.id,
        coins: 1337,
        description: None,
        created_at: d4,
        include_in_credit_note: true,
    };

    let result = REPO
        .get_transactions(&mut txn, FOO.user.id, d1..d5)
        .await
        .unwrap();
    assert_eq!(result, []);

    REPO.create_transaction(&mut txn, &t1).await.unwrap();

    let result = REPO
        .get_transactions(&mut txn, FOO.user.id, d1..d5)
        .await
        .unwrap();
    assert_eq!(result, [t1.clone()]);

    REPO.create_transaction(&mut txn, &t2).await.unwrap();

    let result = REPO
        .get_transactions(&mut txn, FOO.user.id, d1..d5)
        .await
        .unwrap();
    assert_eq!(result, [t1.clone(), t2.clone()]);

    let result = REPO
        .get_transactions(&mut txn, FOO.user.id, d1..d2)
        .await
        .unwrap();
    assert_eq!(result, []);

    let result = REPO
        .get_transactions(&mut txn, FOO.user.id, d1..d3)
        .await
        .unwrap();
    assert_eq!(result, [t1]);

    let result = REPO
        .get_transactions(&mut txn, FOO.user.id, d3..d5)
        .await
        .unwrap();
    assert_eq!(result, [t2]);
}

fn balance(coins: u64, withheld_coins: u64) -> Balance {
    Balance {
        coins,
        withheld_coins,
    }
}
