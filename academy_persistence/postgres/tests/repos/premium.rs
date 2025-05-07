use academy_demo::{UUID1, UUID2, user::FOO};
use academy_models::premium::{Premium, PremiumPlan};
use academy_persistence_contracts::{Database, Transaction, premium::PremiumRepository};
use academy_persistence_postgres::premium::PostgresPremiumRepository;
use chrono::{TimeZone, Utc};

use crate::common::setup;

const REPO: PostgresPremiumRepository = PostgresPremiumRepository;

#[tokio::test]
async fn premium() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_latest_by_user_id(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, None);

    let mut p1 = Premium {
        id: UUID1.into(),
        user_id: FOO.user.id,
        since: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap(),
    };
    let p2 = Premium {
        id: UUID2.into(),
        user_id: FOO.user.id,
        since: Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap(),
        until: Utc.with_ymd_and_hms(2024, 4, 1, 0, 0, 0).unwrap(),
    };
    REPO.create(&mut txn, p1).await.unwrap();
    REPO.create(&mut txn, p2).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_latest_by_user_id(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, Some(p2));

    p1.until = Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap();
    REPO.extend(&mut txn, p1.id, p1.until).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_latest_by_user_id(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, Some(p1));
}

#[tokio::test]
async fn subscriptions() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);

    let result = REPO.list_subscription_users(&mut txn).await.unwrap();
    assert_eq!(result, []);

    REPO.set_subscription(&mut txn, FOO.user.id, Some(PremiumPlan::Monthly))
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(PremiumPlan::Monthly));

    REPO.set_subscription(&mut txn, FOO.user.id, Some(PremiumPlan::Yearly))
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, Some(PremiumPlan::Yearly));

    let result = REPO.list_subscription_users(&mut txn).await.unwrap();
    assert_eq!(result, [FOO.user.id]);

    REPO.set_subscription(&mut txn, FOO.user.id, None)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get_subscription(&mut txn, FOO.user.id).await.unwrap();
    assert_eq!(result, None);

    let result = REPO.list_subscription_users(&mut txn).await.unwrap();
    assert_eq!(result, []);
}
