use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::MIGRATIONS;
use common::{setup, setup_clean};

mod common;

#[tokio::test]
async fn durable_totp_migration_preserves_devices_and_can_be_reverted() {
    const NAME: &str = "2026-10-04-070000_durable_totp_steps";
    let db = common::setup_before(NAME, true).await;
    let txn = db.begin_transaction().await.unwrap();
    let before: String = txn.txn().query_one(
        "SELECT jsonb_agg(jsonb_build_array(id,encode(secret,'hex')) ORDER BY id)::text FROM totp_device_secrets", &[]
    ).await.unwrap().get(0);
    txn.commit().await.unwrap();
    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), [NAME]);
    let txn = db.begin_transaction().await.unwrap();
    let count: i64 = txn
        .txn()
        .query_one(
            "SELECT count(*) FROM totp_device_secrets WHERE last_accepted_step<>-1",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    txn.commit().await.unwrap();
    assert_eq!(db.revert_migrations(Some(1)).await.unwrap(), [NAME]);
    let txn = db.begin_transaction().await.unwrap();
    let after: String = txn.txn().query_one(
        "SELECT jsonb_agg(jsonb_build_array(id,encode(secret,'hex')) ORDER BY id)::text FROM totp_device_secrets", &[]
    ).await.unwrap().get(0);
    assert_eq!(after, before);
    txn.commit().await.unwrap();
    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), [NAME]);
}

/// Recording who signed in must not sign anybody out. Existing sessions keep
/// every value and their refresh tokens, and become legacy sessions; so does a
/// session an older backend creates without naming an origin.
#[tokio::test]
async fn session_origin_migration_keeps_sessions_and_marks_them_legacy() {
    use academy_models::session::SessionOrigin;
    use academy_persistence_contracts::session::SessionRepository;
    use academy_persistence_postgres::session::PostgresSessionRepository;

    const NAME: &str = "2026-10-04-175659_session_origin";
    const SESSIONS: &str = "SELECT jsonb_agg(jsonb_build_array(s.id,s.user_id,s.device_name,s.created_at,\
        s.updated_at,s.mfa_verified,encode(t.refresh_token_hash,'hex')) ORDER BY s.id)::text \
        FROM sessions s LEFT JOIN session_refresh_tokens t ON t.session_id=s.id";
    let db = common::setup_before(NAME, true).await;
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "INSERT INTO session_refresh_tokens(session_id,refresh_token_hash) SELECT id,sha256(id::text::bytea) FROM sessions",
            &[],
        )
        .await
        .unwrap();
    let before: String = txn.txn().query_one(SESSIONS, &[]).await.unwrap().get(0);
    txn.commit().await.unwrap();

    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), [NAME]);

    let mut txn = db.begin_transaction().await.unwrap();
    let after: String = txn.txn().query_one(SESSIONS, &[]).await.unwrap().get(0);
    assert_eq!(after, before);
    for &demo in &*academy_demo::session::ALL_SESSIONS {
        let session = PostgresSessionRepository
            .get(&mut txn, demo.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(session.origin, SessionOrigin::Legacy);
        assert_eq!(session.is_owner_sign_in(), demo.device_name.is_some());
    }
    let (user, created): (uuid::Uuid, chrono::DateTime<chrono::Utc>) = {
        let demo = &academy_demo::session::FOO_1;
        (*demo.user_id, demo.created_at)
    };
    let older_backend: String = txn
        .txn()
        .query_one(
            "INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified) \
             VALUES(gen_random_uuid(),$1,$2,$2,false) RETURNING origin",
            &[&user, &created],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(older_backend, "legacy");
    txn.rollback().await.unwrap();

    for statement in [
        "INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified,origin) \
         VALUES(gen_random_uuid(),$1,$2,$2,false,'owner')",
        "INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified,origin,impersonated_by) \
         VALUES(gen_random_uuid(),$1,$2,$2,false,'sign_in',$1)",
        "INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified,origin,impersonated_by) \
         VALUES(gen_random_uuid(),$1,$2,$2,false,'impersonation',gen_random_uuid())",
    ] {
        let txn = db.begin_transaction().await.unwrap();
        let error = txn
            .txn()
            .execute(statement, &[&user, &created])
            .await
            .unwrap_err();
        assert!(
            matches!(error.code().unwrap().code(), "23514" | "23503"),
            "{statement}: {error:?}"
        );
        txn.rollback().await.unwrap();
    }

    assert_eq!(db.revert_migrations(Some(1)).await.unwrap(), [NAME]);
    let txn = db.begin_transaction().await.unwrap();
    let reverted: String = txn.txn().query_one(SESSIONS, &[]).await.unwrap().get(0);
    assert_eq!(reverted, before);
    txn.commit().await.unwrap();
    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), [NAME]);
}

async fn fingerprint(db: &common::Db) -> std::collections::BTreeMap<String, String> {
    let tx = db.begin_transaction().await.unwrap();
    let mut state = std::collections::BTreeMap::new();
    for row in tx
        .txn()
        .query(
            "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
            &[],
        )
        .await
        .unwrap()
    {
        let table: String = row.get(0);
        let quoted = table.replace('"', "\"\"");
        let data: String = tx.txn().query_one(&format!("SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]')::text FROM \"{quoted}\" t"), &[]).await.unwrap().get(0);
        state.insert(table, data);
    }
    tx.commit().await.unwrap();
    state
}

async fn current_refusal(db: &common::Db) {
    let data = fingerprint(db).await;
    let before = common::fixture::evidence_path("current-migrations", "sql");
    common::fixture::preserve(db, "current-before-refusal").await;
    let tx = db.begin_transaction().await.unwrap();
    let original: String = tx
        .txn()
        .query_one(
            "SELECT jsonb_agg(to_jsonb(t) ORDER BY name)::text FROM _migrations t",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    common::assert_down_refused(
        db,
        "2026-09-26-120000_daily_learning_heart_receipts",
        "Heart operation replay receipts must not be removed by a downgrade",
    )
    .await;
    let tx = db.begin_transaction().await.unwrap();
    let after: String = tx
        .txn()
        .query_one(
            "SELECT jsonb_agg(to_jsonb(t) ORDER BY name)::text FROM _migrations t",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(after, original);
    tx.commit().await.unwrap();
    std::fs::write(before, after).unwrap();
    common::fixture::preserve(db, "current-after-refusal").await;
    assert!(db.run_migrations(None).await.unwrap().is_empty());
    assert_eq!(fingerprint(db).await, data);
}

async fn historical_matrix(with_data: bool) {
    // A separate empty database precedes the first unconditional preservation guard.
    let last = "2026-09-08-100000_purchase_contract_evidence";
    let db = common::fixture::fresh_history().await;
    let end = MIGRATIONS.iter().position(|m| m.name == last).unwrap() + 1;
    let names = MIGRATIONS[..end].iter().map(|m| m.name).collect::<Vec<_>>();
    assert_eq!(common::apply_through(&db, Some(last)).await, names);
    if with_data {
        common::seed(&db).await;
    }
    for i in 1..=end {
        let mut reverted = db.revert_migrations(Some(i)).await.unwrap();
        reverted.reverse();
        assert_eq!(reverted, names[end - i..]);
        let applied = common::apply_through(&db, Some(last)).await;
        assert_eq!(applied, names[end - i..]);
    }
}

#[tokio::test]
async fn migrations_clean() {
    let db = setup_clean().await;
    assert_eq!(
        db.run_migrations(None).await.unwrap(),
        MIGRATIONS.iter().map(|m| m.name).collect::<Vec<_>>()
    );
    current_refusal(&db).await;
    historical_matrix(false).await;
}

#[tokio::test]
async fn migrations_with_data() {
    let db = setup().await;
    current_refusal(&db).await;
    historical_matrix(true).await;
}

/// Making the actor nullable preserves history; reversal never deletes an
/// unknown operator's evidence to satisfy the old schema.
#[tokio::test]
async fn operator_audit_migration_preserves_entries_and_refuses_lossy_rollback() {
    const NAME: &str = "2026-10-05-180000_operator_audit";
    let db = common::setup_before(NAME, true).await;
    let txn = db.begin_transaction().await.unwrap();
    txn.txn().execute("INSERT INTO admin_audit_log(id,at,admin_user_id,method,path,status,request_id) VALUES(gen_random_uuid(),clock_timestamp(),$1,'PATCH','/auth/users/me',200,'known-actor')", &[&*academy_demo::user::ADMIN.user.id]).await.unwrap();
    let before: String = txn
        .txn()
        .query_one(
            "SELECT jsonb_agg(to_jsonb(a))::text FROM admin_audit_log a",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    txn.commit().await.unwrap();
    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), [NAME]);
    let txn = db.begin_transaction().await.unwrap();
    let after: String = txn
        .txn()
        .query_one(
            "SELECT jsonb_agg(to_jsonb(a))::text FROM admin_audit_log a",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(after, before);
    txn.txn().execute("INSERT INTO admin_audit_log(id,at,admin_user_id,method,path,status,request_id) VALUES(gen_random_uuid(),clock_timestamp(),NULL,'PUT','/auth/session',200,'unknown-operator')", &[]).await.unwrap();
    txn.commit().await.unwrap();
    assert!(db.revert_migrations(Some(1)).await.is_err());
    let txn = db.begin_transaction().await.unwrap();
    let count: i64 = txn
        .txn()
        .query_one(
            "SELECT count(*) FROM admin_audit_log WHERE admin_user_id IS NULL",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    txn.commit().await.unwrap();
}

/// The migration CLI cannot erase the origin of a live delegated session.
#[tokio::test]
async fn session_origin_rollback_refuses_live_impersonation() {
    const NAME: &str = "2026-10-04-175659_session_origin";
    let db = common::setup_before(NAME, true).await;
    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), [NAME]);
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .execute(
            "UPDATE sessions SET origin='impersonation',impersonated_by=$1 WHERE id=$2",
            &[
                &*academy_demo::user::ADMIN.user.id,
                &*academy_demo::session::FOO_1.id,
            ],
        )
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert!(db.revert_migrations(Some(1)).await.is_err());
    let txn = db.begin_transaction().await.unwrap();
    let origin: String = txn
        .txn()
        .query_one(
            "SELECT origin FROM sessions WHERE id=$1",
            &[&*academy_demo::session::FOO_1.id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(origin, "impersonation");
    txn.commit().await.unwrap();
}
