use std::time::Duration;

use academy_demo::user::FOO;
use academy_models::heart::Hearts;
use academy_persistence_contracts::{Database, Transaction, heart::HeartRepository};
use academy_persistence_postgres::heart::PostgresHeartRepository;

use crate::common::setup;

const REPO: PostgresHeartRepository = PostgresHeartRepository;

#[tokio::test]
async fn get_set() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO.get(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);

    let hearts = Hearts {
        hearts: 42,
        last_refill: FOO.user.created_at,
    };

    REPO.set(&mut txn, FOO.user.id, hearts).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(hearts));

    let hearts = Hearts {
        hearts: 7,
        last_refill: FOO.user.created_at + Duration::from_secs(1337),
    };
    REPO.set(&mut txn, FOO.user.id, hearts).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(hearts));
}
