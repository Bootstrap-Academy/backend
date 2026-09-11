use crate::common::setup;
use academy_demo::user::{BAR, FOO};
use academy_persistence_contracts::{Database, Transaction, user::UserRepository};
use academy_persistence_postgres::{deletion, user::PostgresUserRepository};

#[tokio::test]
async fn deletion_work_is_atomic_durable_and_per_service() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    PostgresUserRepository
        .delete(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(deletion::backlog(&mut txn).await.unwrap().len(), 3);
    txn.rollback().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(deletion::backlog(&mut txn).await.unwrap().is_empty());
    PostgresUserRepository
        .delete(&mut txn, FOO.user.id)
        .await
        .unwrap();
    PostgresUserRepository
        .delete(&mut txn, BAR.user.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    crate::repos::assert_down_refused(
        &db,
        "2026-09-07-200000_durable_user_deletion",
        "Cannot discard pending account erasure work",
    )
    .await;
    let mut txn = db.begin_transaction().await.unwrap();
    let first = deletion::claim(&mut txn, &[]).await.unwrap().unwrap();
    let mut other = db.begin_transaction().await.unwrap();
    let second = deletion::claim(&mut other, &[]).await.unwrap().unwrap();
    assert_ne!(
        (first.user_id, &first.service),
        (second.user_id, &second.service)
    );
    txn.commit().await.unwrap();
    other.commit().await.unwrap();
    assert_eq!(first.attempts, 1);
    assert_eq!(second.attempts, 1);
    let mut txn = db.begin_transaction().await.unwrap();
    deletion::acknowledge(&mut txn, &first, false)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut other = db.begin_transaction().await.unwrap();
    deletion::acknowledge(&mut other, &second, true)
        .await
        .unwrap();
    other.rollback().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    // A rolled-back acknowledgement does not roll back its committed claim.
    let later = deletion::claim(&mut txn, &[]).await.unwrap().unwrap();
    assert_ne!(
        (second.user_id, &second.service),
        (later.user_id, &later.service)
    );
    txn.rollback().await.unwrap();
    let txn = db.begin_transaction().await.unwrap();
    txn.txn().execute("UPDATE user_deletion_work SET next_attempt_at=now()-interval '1 hour' WHERE user_id=$1 AND service=$2", &[&*second.user_id, &second.service]).await.unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let replay = deletion::claim(&mut txn, &[]).await.unwrap().unwrap();
    assert_eq!(
        (second.user_id, second.service.clone()),
        (replay.user_id, replay.service.clone())
    );
    assert_eq!(replay.attempts, 2);
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    // Both late success and late failure from an expired owner are harmless.
    assert!(
        !deletion::acknowledge(&mut txn, &second, true)
            .await
            .unwrap()
    );
    assert!(
        !deletion::acknowledge(&mut txn, &second, false)
            .await
            .unwrap()
    );
    deletion::acknowledge(&mut txn, &replay, true)
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        deletion::backlog(&mut txn)
            .await
            .unwrap()
            .iter()
            .map(|r| r.1)
            .sum::<i64>(),
        5
    );
}

#[tokio::test]
async fn claim_and_acknowledgement_commits_are_independent() {
    let db = setup().await;
    let txn = db.begin_transaction().await.unwrap();
    txn.txn().batch_execute("INSERT INTO user_deletion_work(user_id,service) VALUES ('11111111-1111-4111-8111-111111111111','skills');
        CREATE FUNCTION reject_claim() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic claim COMMIT fault'; END $$;
        CREATE CONSTRAINT TRIGGER reject_claim AFTER UPDATE ON user_deletion_work DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_claim();").await.unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let uncommitted = deletion::claim(&mut txn, &[]).await.unwrap().unwrap();
    assert_eq!(uncommitted.attempts, 1);
    assert!(txn.commit().await.is_err());
    let txn = db.begin_transaction().await.unwrap();
    let row = txn
        .txn()
        .query_one(
            "SELECT attempts, next_attempt_at<=now() FROM user_deletion_work",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, i64>(0), 0);
    assert!(row.get::<_, bool>(1));
    txn.txn().batch_execute("DROP TRIGGER reject_claim ON user_deletion_work; DROP FUNCTION reject_claim();
        CREATE FUNCTION reject_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic ack COMMIT fault'; END $$;
        CREATE CONSTRAINT TRIGGER reject_ack AFTER DELETE ON user_deletion_work DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_ack();").await.unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let work = deletion::claim(&mut txn, &[]).await.unwrap().unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(deletion::acknowledge(&mut txn, &work, true).await.unwrap());
    assert!(txn.commit().await.is_err());
    let mut txn = db.begin_transaction().await.unwrap();
    let row = txn
        .txn()
        .query_one(
            "SELECT attempts, next_attempt_at>now(), last_error FROM user_deletion_work",
            &[],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, i64>(0), 1);
    assert!(row.get::<_, bool>(1));
    assert_eq!(row.get::<_, String>(2), "UnacknowledgedAttempt");
    assert!(deletion::claim(&mut txn, &[]).await.unwrap().is_none());
    // Ack failure and process death leave the same recoverable leased state.
    txn.txn()
        .batch_execute(
            "DROP TRIGGER reject_ack ON user_deletion_work; DROP FUNCTION reject_ack();
        UPDATE user_deletion_work SET next_attempt_at=now()-interval '1 minute';",
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    let replay = deletion::claim(&mut txn, &[]).await.unwrap().unwrap();
    assert_eq!(replay.attempts, 2);
    txn.commit().await.unwrap();
    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        deletion::acknowledge(&mut txn, &replay, true)
            .await
            .unwrap()
    );
    txn.commit().await.unwrap();
}
