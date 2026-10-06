use std::time::Duration;

use academy_demo::{
    SHA256HASH1, SHA256HASH2, UUID1, UUID2,
    session::{ADMIN_1, ALL_SESSIONS, FOO_1, FOO_2},
    user::{ADMIN, ALL_USERS, FOO},
};
use academy_models::session::{Session, SessionOrigin, SessionRefreshTokenHash};
use academy_persistence_contracts::{
    Database, Transaction, session::SessionRepository, user::UserRepository,
};
use academy_persistence_postgres::session::PostgresSessionRepository;
use academy_utils::patch::Patch;
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
        mfa_verified: true,
        origin: SessionOrigin::SignIn,
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

/// The origin is stored once and read back unchanged, including who signed in
/// to someone else's account.
#[tokio::test]
async fn create_impersonation() {
    let db = setup().await;

    for (id, admin) in [(UUID1, Some(ADMIN.user.id)), (UUID2, None)] {
        let session = Session {
            id: id.into(),
            device_name: None,
            mfa_verified: false,
            origin: SessionOrigin::Impersonation { admin },
            ..FOO_1.clone()
        };

        let mut txn = db.begin_transaction().await.unwrap();
        REPO.create(&mut txn, &session).await.unwrap();
        txn.commit().await.unwrap();

        let mut txn = db.begin_transaction().await.unwrap();
        let result = REPO.get(&mut txn, session.id).await.unwrap().unwrap();
        assert_eq!(result, session);
        assert!(!result.is_owner_sign_in());
    }
}

/// Sessions an administrator opened in other accounts end together with the
/// administrator's own account.
#[tokio::test]
async fn impersonation_sessions_end_with_the_admin_account() {
    let db = setup().await;
    let session = Session {
        id: UUID1.into(),
        device_name: None,
        mfa_verified: false,
        origin: SessionOrigin::Impersonation {
            admin: Some(ADMIN.user.id),
        },
        ..FOO_1.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, &session).await.unwrap();
    REPO.save_refresh_token_hash(&mut txn, session.id, (*SHA256HASH1).into())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        academy_persistence_postgres::user::PostgresUserRepository
            .delete(&mut txn, ADMIN.user.id)
            .await
            .unwrap()
    );
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get(&mut txn, session.id).await.unwrap(), None);
    assert_eq!(
        REPO.get(&mut txn, FOO_1.id).await.unwrap().as_ref(),
        Some(&*FOO_1)
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

/// Administrative authority is granted to a session that was authenticated
/// with the second factor, so removing that factor has to take it away from
/// every session of the account and from nobody else's.
#[tokio::test]
async fn clear_mfa_verified_by_user() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        REPO.get(&mut txn, ADMIN_1.id)
            .await
            .unwrap()
            .unwrap()
            .mfa_verified
    );

    REPO.clear_mfa_verified_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();
    REPO.clear_mfa_verified_by_user(&mut txn, ADMIN.user.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    for session in [&*ADMIN_1, &*FOO_1, &*FOO_2] {
        let result = REPO.get(&mut txn, session.id).await.unwrap().unwrap();
        assert_eq!(
            result,
            Session {
                mfa_verified: false,
                ..session.clone()
            }
        );
    }
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

async fn refresh_fixture() -> academy_persistence_postgres::PostgresDatabase {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.save_refresh_token_hash(&mut txn, FOO_1.id, (*SHA256HASH1).into())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    db
}

async fn backend_pid(txn: &academy_persistence_postgres::PostgresTransaction) -> i32 {
    txn.txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0)
}

async fn wait_for_owner_lock(
    db: &academy_persistence_postgres::PostgresDatabase,
    waiter: i32,
    holder: i32,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let observer = db.begin_transaction().await.unwrap();
            let row = observer.txn().query_one(
                "SELECT pg_blocking_pids(pid), coalesce(wait_event_type='Lock', false) FROM pg_stat_activity WHERE pid=$1",
                &[&waiter],
            ).await.unwrap();
            if row.get::<_, Vec<i32>>(0).contains(&holder) && row.get::<_, bool>(1) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("refresh must actually wait for the owning transaction");
}

#[tokio::test]
async fn refresh_rotation_has_one_winner_and_preserves_winning_hash() {
    let db = refresh_fixture().await;
    let mut winner = db.begin_transaction().await.unwrap();
    assert_eq!(
        winner
            .txn()
            .query_one("SHOW transaction_isolation", &[])
            .await
            .unwrap()
            .get::<_, &str>(0),
        "read committed"
    );
    let session = REPO
        .get_by_refresh_token_hash_for_update(&mut winner, (*SHA256HASH1).into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session, *FOO_1);
    let holder = backend_pid(&winner).await;

    let mut loser = db.begin_transaction().await.unwrap();
    let waiter = backend_pid(&loser).await;
    let loser = tokio::spawn(async move {
        let result = REPO
            .get_by_refresh_token_hash_for_update(&mut loser, (*SHA256HASH1).into())
            .await
            .unwrap();
        loser.commit().await.unwrap();
        result
    });
    wait_for_owner_lock(&db, waiter, holder).await;

    // Ordinary access lookups remain reads, even while the refresh owns locks.
    let mut reader = db.begin_transaction().await.unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(1),
            REPO.get_by_refresh_token_hash(&mut reader, (*SHA256HASH1).into())
        )
        .await
        .unwrap()
        .unwrap()
        .is_some()
    );
    reader.commit().await.unwrap();
    REPO.save_refresh_token_hash(&mut winner, FOO_1.id, (*SHA256HASH2).into())
        .await
        .unwrap();
    winner.commit().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), loser)
            .await
            .unwrap()
            .unwrap(),
        None
    );

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get_by_refresh_token_hash_for_update(&mut txn, (*SHA256HASH1).into())
            .await
            .unwrap(),
        None
    );
    // This is the same live session/hash authority check used by Access auth.
    assert_eq!(
        REPO.get_by_refresh_token_hash(&mut txn, (*SHA256HASH2).into())
            .await
            .unwrap(),
        Some(FOO_1.clone())
    );
    assert_eq!(
        REPO.get_refresh_token_hash(&mut txn, FOO_1.id)
            .await
            .unwrap(),
        Some((*SHA256HASH2).into())
    );
}

#[tokio::test]
async fn refresh_rechecks_session_revocation_while_waiting() {
    let db = refresh_fixture().await;
    let owner = db.begin_transaction().await.unwrap();
    owner
        .txn()
        .query_one(
            "SELECT id FROM users WHERE id=$1 FOR UPDATE",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    let holder = backend_pid(&owner).await;
    let mut refresh = db.begin_transaction().await.unwrap();
    let waiter = backend_pid(&refresh).await;
    let refresh = tokio::spawn(async move {
        REPO.get_by_refresh_token_hash_for_update(&mut refresh, (*SHA256HASH1).into())
            .await
            .unwrap()
    });
    wait_for_owner_lock(&db, waiter, holder).await;
    owner
        .txn()
        .execute("DELETE FROM sessions WHERE id=$1", &[&*FOO_1.id])
        .await
        .unwrap();
    owner.commit().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), refresh)
            .await
            .unwrap()
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn refresh_rechecks_disabled_owner_while_waiting() {
    let db = refresh_fixture().await;
    let owner = db.begin_transaction().await.unwrap();
    owner
        .txn()
        .query_one(
            "SELECT id FROM users WHERE id=$1 FOR UPDATE",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    let holder = backend_pid(&owner).await;
    let mut refresh = db.begin_transaction().await.unwrap();
    let waiter = backend_pid(&refresh).await;
    let refresh = tokio::spawn(async move {
        REPO.get_by_refresh_token_hash_for_update(&mut refresh, (*SHA256HASH1).into())
            .await
            .unwrap()
    });
    wait_for_owner_lock(&db, waiter, holder).await;
    owner
        .txn()
        .batch_execute("SET LOCAL academy.moderation_write='authorized'")
        .await
        .unwrap();
    owner
        .txn()
        .execute(
            "UPDATE users SET enabled=false WHERE id=$1",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    owner.commit().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), refresh)
            .await
            .unwrap()
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn refresh_reads_fresh_expiry_and_mfa_after_waiting() {
    let db = refresh_fixture().await;
    let owner = db.begin_transaction().await.unwrap();
    owner
        .txn()
        .query_one(
            "SELECT id FROM users WHERE id=$1 FOR UPDATE",
            &[&*FOO.user.id],
        )
        .await
        .unwrap();
    let holder = backend_pid(&owner).await;
    let mut refresh = db.begin_transaction().await.unwrap();
    let waiter = backend_pid(&refresh).await;
    let refresh = tokio::spawn(async move {
        REPO.get_by_refresh_token_hash_for_update(&mut refresh, (*SHA256HASH1).into())
            .await
            .unwrap()
    });
    wait_for_owner_lock(&db, waiter, holder).await;
    let expired = FOO_1.updated_at - Duration::from_secs(3600);
    owner
        .txn()
        .execute(
            "UPDATE sessions SET updated_at=$1,mfa_verified=false WHERE id=$2",
            &[&expired, &*FOO_1.id],
        )
        .await
        .unwrap();
    owner.commit().await.unwrap();
    let session = tokio::time::timeout(Duration::from_secs(5), refresh)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(session.updated_at, expired);
    assert!(!session.mfa_verified);
}

#[tokio::test]
async fn refresh_holds_session_lock_against_direct_revocation() {
    let db = refresh_fixture().await;
    let mut refresh = db.begin_transaction().await.unwrap();
    REPO.get_by_refresh_token_hash_for_update(&mut refresh, (*SHA256HASH1).into())
        .await
        .unwrap()
        .unwrap();
    let holder = backend_pid(&refresh).await;
    let mut deletion = db.begin_transaction().await.unwrap();
    let waiter = backend_pid(&deletion).await;
    let deletion = tokio::spawn(async move {
        let removed = REPO.delete(&mut deletion, FOO_1.id).await.unwrap();
        deletion.commit().await.unwrap();
        removed
    });
    wait_for_owner_lock(&db, waiter, holder).await;
    refresh.commit().await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), deletion)
            .await
            .unwrap()
            .unwrap()
    );
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get_by_refresh_token_hash_for_update(&mut txn, (*SHA256HASH1).into())
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn refreshes_of_distinct_sessions_share_owner_order() {
    let db = refresh_fixture().await;
    let mut seed = db.begin_transaction().await.unwrap();
    REPO.save_refresh_token_hash(&mut seed, FOO_2.id, (*SHA256HASH2).into())
        .await
        .unwrap();
    seed.commit().await.unwrap();
    let mut first = db.begin_transaction().await.unwrap();
    REPO.get_by_refresh_token_hash_for_update(&mut first, (*SHA256HASH1).into())
        .await
        .unwrap()
        .unwrap();
    let holder = backend_pid(&first).await;
    let mut second = db.begin_transaction().await.unwrap();
    let waiter = backend_pid(&second).await;
    let second = tokio::spawn(async move {
        REPO.get_by_refresh_token_hash_for_update(&mut second, (*SHA256HASH2).into())
            .await
            .unwrap()
    });
    wait_for_owner_lock(&db, waiter, holder).await;
    first.commit().await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), second)
            .await
            .unwrap()
            .unwrap(),
        Some(FOO_2.clone())
    );
}
