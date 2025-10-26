use std::time::Duration;

use academy_demo::{
    SHA256HASH1, SHA256HASH2, UUID1,
    session::{ADMIN_1, ALL_SESSIONS, FOO_1, FOO_2},
    user::{ADMIN, ALL_USERS, FOO},
};
use academy_models::session::{Session, SessionRefreshTokenHash};
use academy_persistence_contracts::{Database, Transaction, session::SessionRepository};
use academy_persistence_postgres::session::PostgresSessionRepository;
use academy_utils::patch::Patch;
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use pretty_assertions::assert_eq;

use crate::common::setup;

const REPO: PostgresSessionRepository = PostgresSessionRepository;

#[tokio::test]
async fn get() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for &session in &*ALL_SESSIONS {
        let result = REPO.get(&mut txn, session.id).await.unwrap().unwrap();
        assert_eq!(&result, session);
    }

    let result = REPO.get(&mut txn, UUID1.into()).await.unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn get_by_refresh_token_hash() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.save_refresh_token_hash(&mut txn, FOO_1.id, (*SHA256HASH1).into())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_by_refresh_token_hash(&mut txn, (*SHA256HASH1).into())
        .await
        .unwrap();
    assert_eq!(result.unwrap(), *FOO_1);

    let result = REPO
        .get_by_refresh_token_hash(&mut txn, (*SHA256HASH2).into())
        .await
        .unwrap();
    assert_eq!(result, None);
}

#[tokio::test]
async fn list_by_user() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    for &user_composite in &*ALL_USERS {
        let expected = ALL_SESSIONS
            .iter()
            .filter(|s| s.user_id == user_composite.user.id)
            .copied()
            .cloned()
            .collect::<Vec<_>>();

        let result = REPO
            .list_by_user(&mut txn, user_composite.user.id)
            .await
            .unwrap();

        assert_eq!(result, expected);
    }
}

#[tokio::test]
async fn create() {
    let db = setup().await;

    let session = Session {
        id: UUID1.into(),
        user_id: ADMIN.user.id,
        device_name: Some("some device name".try_into().unwrap()),
        created_at: ADMIN.user.created_at + Duration::from_secs(10 * 3600),
        updated_at: ADMIN.user.created_at + Duration::from_secs(7 * 24 * 3600),
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, &session).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, session.id).await.unwrap().unwrap(),
        session
    );
}

#[tokio::test]
async fn update() {
    let db = setup().await;

    let expected = Session {
        id: FOO_1.id,
        created_at: FOO_1.created_at,
        ..FOO_2.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .update(&mut txn, FOO_1.id, expected.as_patch_ref())
        .await
        .unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.get(&mut txn, FOO_1.id).await.unwrap();
    assert_eq!(result.unwrap(), expected);

    let result = REPO
        .update(&mut txn, UUID1.into(), expected.as_patch_ref())
        .await
        .unwrap();
    assert!(!result);
}

#[tokio::test]
async fn delete() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO.delete(&mut txn, FOO_1.id).await.unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get(&mut txn, FOO_1.id).await.unwrap(), None);
    let result = REPO.delete(&mut txn, FOO_1.id).await.unwrap();
    assert!(!result);
}

#[tokio::test]
async fn delete_by_user() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.delete_by_user(&mut txn, FOO.user.id).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get(&mut txn, FOO_1.id).await.unwrap(), None);
    assert_eq!(REPO.get(&mut txn, FOO_2.id).await.unwrap(), None);
}

#[tokio::test]
async fn delete_by_last_update() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .delete_by_updated_at(&mut txn, FOO_2.updated_at + Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(result, 2);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get(&mut txn, ADMIN_1.id).await.unwrap(), None);
    assert_eq!(REPO.get(&mut txn, FOO_1.id).await.unwrap().unwrap(), *FOO_1);
    assert_eq!(REPO.get(&mut txn, FOO_2.id).await.unwrap(), None);
}

#[tokio::test]
async fn active_users_returns_buckets() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let start = Utc.with_ymd_and_hms(2024, 3, 14, 13, 0, 0).unwrap();
    let bucket = ChronoDuration::hours(1);

    let result = REPO.active_users(&mut txn, start, bucket, 3).await.unwrap();

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].bucket_start, start);
    assert_eq!(result[0].active_users, 1);
    assert_eq!(result[1].bucket_start, start + bucket);
    assert_eq!(result[1].active_users, 0);
    assert_eq!(result[2].bucket_start, start + bucket * 2);
    assert_eq!(result[2].active_users, 0);
}

#[tokio::test]
async fn list_refresh_token_hashes_by_user() {
    let db = setup().await;

    let rth1 = SessionRefreshTokenHash::from(*SHA256HASH1);
    let rth2 = SessionRefreshTokenHash::from(*SHA256HASH2);

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.save_refresh_token_hash(&mut txn, FOO_1.id, rth1)
        .await
        .unwrap();
    REPO.save_refresh_token_hash(&mut txn, FOO_2.id, rth2)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .list_refresh_token_hashes_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();

    assert_eq!(result, [rth1, rth2]);
}

#[tokio::test]
async fn refresh_token_hash() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.save_refresh_token_hash(&mut txn, FOO_1.id, (*SHA256HASH1).into())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_refresh_token_hash(&mut txn, FOO_1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, (*SHA256HASH1).into());

    REPO.save_refresh_token_hash(&mut txn, FOO_1.id, (*SHA256HASH2).into())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_refresh_token_hash(&mut txn, FOO_1.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, (*SHA256HASH2).into());

    let result = REPO
        .get_refresh_token_hash(&mut txn, UUID1.into())
        .await
        .unwrap();
    assert_eq!(result, None);
}
