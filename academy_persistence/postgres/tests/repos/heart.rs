use std::time::Duration;

use academy_demo::{
    UUID1, UUID2,
    user::{BAR, FOO},
};
use academy_models::heart::{
    HeartOperation, HeartOperationClaim, HeartOperationOutcome, HeartOperationReceipt, Hearts,
};
use academy_persistence_contracts::{
    Database, Transaction, heart::HeartRepository, user::UserRepository,
};
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

fn operation() -> HeartOperation {
    HeartOperation {
        id: UUID1.into(),
        user_id: FOO.user.id,
        half_hearts: 2,
        reason: "incorrect_challenge_attempt".into(),
    }
}

fn receipt() -> HeartOperationReceipt {
    HeartOperationReceipt {
        operation_id: UUID1.into(),
        user_id: FOO.user.id,
        charged_half_hearts: 2,
        hearts: 0,
        outcome: HeartOperationOutcome::Charged,
    }
}

#[tokio::test]
async fn simultaneous_retries_wait_for_one_committed_receipt() {
    let db = setup().await;
    let mut first = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.claim_operation(&mut first, &operation())
            .await
            .unwrap(),
        HeartOperationClaim::New
    );
    assert!(REPO.lock_user(&mut first, FOO.user.id).await.unwrap());
    REPO.set(
        &mut first,
        FOO.user.id,
        Hearts {
            hearts: 0,
            last_refill: FOO.user.created_at,
        },
    )
    .await
    .unwrap();

    let mut second = db.begin_transaction().await.unwrap();
    let mut retry = tokio::spawn(async move {
        let result = REPO
            .claim_operation(&mut second, &operation())
            .await
            .unwrap();
        second.commit().await.unwrap();
        result
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut retry)
            .await
            .is_err(),
        "retry passed an uncommitted debit"
    );
    REPO.complete_operation(&mut first, &operation(), receipt())
        .await
        .unwrap();
    first.commit().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), retry)
            .await
            .unwrap()
            .unwrap(),
        HeartOperationClaim::Completed(receipt())
    );

    // A later refill never changes what an exact replay returns.
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.set(
        &mut txn,
        FOO.user.id,
        Hearts {
            hearts: 10,
            last_refill: FOO.user.created_at,
        },
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.claim_operation(&mut txn, &operation()).await.unwrap(),
        HeartOperationClaim::Completed(receipt())
    );
    assert_eq!(
        REPO.get(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .hearts,
        10
    );
    let rows: serde_json::Value =
        serde_json::from_str(&REPO.export_operations(&mut txn, FOO.user.id).await.unwrap())
            .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn distinct_attempts_share_the_balance_lock_with_premium_and_refills() {
    use academy_persistence_contracts::premium::PremiumRepository;
    use academy_persistence_postgres::premium::PostgresPremiumRepository;
    let db = setup().await;
    let mut first = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.claim_operation(&mut first, &operation())
            .await
            .unwrap(),
        HeartOperationClaim::New
    );
    assert!(REPO.lock_user(&mut first, FOO.user.id).await.unwrap());
    REPO.set(
        &mut first,
        FOO.user.id,
        Hearts {
            hearts: 0,
            last_refill: FOO.user.created_at,
        },
    )
    .await
    .unwrap();

    let mut second = db.begin_transaction().await.unwrap();
    let other = HeartOperation {
        id: UUID2.into(),
        ..operation()
    };
    assert_eq!(
        REPO.claim_operation(&mut second, &other).await.unwrap(),
        HeartOperationClaim::New
    );
    let mut waiter = tokio::spawn(async move {
        assert!(REPO.lock_user(&mut second, FOO.user.id).await.unwrap());
        let balance = REPO.get(&mut second, FOO.user.id).await.unwrap().unwrap();
        let receipt = HeartOperationReceipt {
            operation_id: other.id,
            user_id: other.user_id,
            charged_half_hearts: 0,
            hearts: balance.hearts,
            outcome: HeartOperationOutcome::Insufficient,
        };
        REPO.complete_operation(&mut second, &other, receipt)
            .await
            .unwrap();
        second.commit().await.unwrap();
        balance.hearts
    });
    let mut premium_tx = db.begin_transaction().await.unwrap();
    let mut premium = tokio::spawn(async move {
        // The actual purchase/renewal service takes this repository lock before
        // creating/extending premium. It cannot race a settled failure debit.
        PostgresPremiumRepository
            .get_latest_by_user_id(&mut premium_tx, FOO.user.id)
            .await
            .unwrap();
        premium_tx.rollback().await.unwrap();
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut waiter)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut premium)
            .await
            .is_err()
    );
    REPO.complete_operation(&mut first, &operation(), receipt())
        .await
        .unwrap();
    first.commit().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap(),
        0
    );
    tokio::time::timeout(Duration::from_secs(5), premium)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn receipt_and_debit_roll_back_together_and_conflicts_never_change_them() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.set(
        &mut txn,
        FOO.user.id,
        Hearts {
            hearts: 2,
            last_refill: FOO.user.created_at,
        },
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.claim_operation(&mut txn, &operation()).await.unwrap(),
        HeartOperationClaim::New
    );
    REPO.set(
        &mut txn,
        FOO.user.id,
        Hearts {
            hearts: 0,
            last_refill: FOO.user.created_at,
        },
    )
    .await
    .unwrap();
    REPO.complete_operation(&mut txn, &operation(), receipt())
        .await
        .unwrap();
    txn.rollback().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.claim_operation(&mut txn, &operation()).await.unwrap(),
        HeartOperationClaim::New
    );
    assert_eq!(
        REPO.get(&mut txn, FOO.user.id)
            .await
            .unwrap()
            .unwrap()
            .hearts,
        2
    );
    REPO.set(
        &mut txn,
        FOO.user.id,
        Hearts {
            hearts: 0,
            last_refill: FOO.user.created_at,
        },
    )
    .await
    .unwrap();
    REPO.complete_operation(&mut txn, &operation(), receipt())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    for conflicting in [
        HeartOperation {
            user_id: BAR.user.id,
            ..operation()
        },
        HeartOperation {
            half_hearts: 1,
            ..operation()
        },
        HeartOperation {
            reason: "different".into(),
            ..operation()
        },
    ] {
        let mut txn = db.begin_transaction().await.unwrap();
        assert_eq!(
            REPO.claim_operation(&mut txn, &conflicting).await.unwrap(),
            HeartOperationClaim::Conflict
        );
    }
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.export_operations(&mut txn, BAR.user.id).await.unwrap(),
        "[]"
    );
    // A normal learning receipt is erased with its owner; no orphan learning
    // history or user recreation is allowed on a late outbox retry.
    academy_persistence_postgres::user::PostgresUserRepository
        .delete(&mut txn, FOO.user.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.export_operations(&mut txn, FOO.user.id).await.unwrap(),
        "[]"
    );
    assert!(!REPO.lock_user(&mut txn, FOO.user.id).await.unwrap());
}

#[tokio::test]
async fn migration_is_additive_and_cannot_discard_replay_receipts() {
    const MIGRATION: &str = "2026-09-12-170000_internal_heart_operations";
    let db = crate::common::setup_before(MIGRATION, true).await;
    let mut txn = db.begin_transaction().await.unwrap();
    let before = Hearts {
        hearts: 7,
        last_refill: FOO.user.created_at,
    };
    REPO.set(&mut txn, FOO.user.id, before).await.unwrap();
    txn.commit().await.unwrap();
    assert_eq!(
        crate::common::apply_through(&db, Some(MIGRATION)).await,
        [MIGRATION]
    );
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get(&mut txn, FOO.user.id).await.unwrap(), Some(before));
    assert_eq!(
        REPO.export_operations(&mut txn, FOO.user.id).await.unwrap(),
        "[]"
    );
    txn.rollback().await.unwrap();
    super::assert_down_refused(&db, MIGRATION, "must not be removed").await;
}
