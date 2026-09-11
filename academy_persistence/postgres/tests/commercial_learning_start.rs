//! Serial functional tests, restricted to an explicitly owned disposable fixture.
//! Synthetic proof hashes exercise the repository/SQL boundary, not HTTP authentication.
mod common;

use academy_demo::user::FOO;
use academy_persistence_contracts::{
    Database, Transaction,
    moderation::{ModerationConflict, ModerationRepository},
};
use academy_persistence_postgres::{
    PostgresDatabase, PostgresDatabaseConfig, moderation::PostgresModerationRepository as Repo,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use uuid::Uuid;

const CLAIM: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RIGHTS: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

async fn setup() -> (PostgresDatabase, Uuid) {
    // common::setup resets public. Authenticate the fixture target before calling it.
    let root = PathBuf::from(std::env::var("BOOTSTRAP_L3_LEARNING_FIXTURE").unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(root.parent().unwrap(), std::path::Path::new("/tmp"));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("bootstrap-l3-learning-refresh-")
    );
    let marker: Value =
        serde_json::from_slice(&std::fs::read(root.join("OWNER.json")).unwrap()).unwrap();
    let config_path = root.join("fixture.toml");
    assert_eq!(
        std::env::var("ACADEMY_CONFIG").unwrap(),
        config_path.to_str().unwrap()
    );
    assert!(std::env::var_os("DATABASE_URL").is_none());
    assert_eq!(std::env::var("SQLX_OFFLINE").unwrap(), "true");
    let config = academy_config::load().unwrap();
    let parsed: bb8_postgres::tokio_postgres::Config = config.database.url.parse().unwrap();
    assert_eq!(
        parsed.get_hosts(),
        &[bb8_postgres::tokio_postgres::config::Host::Tcp(
            "127.0.0.1".into()
        )]
    );
    assert!(parsed.get_hostaddrs().is_empty());
    assert_eq!(parsed.get_ports(), &[56720]);
    assert_eq!(parsed.get_dbname(), marker["database"].as_str());
    assert_eq!(parsed.get_user(), marker["user"].as_str());
    assert!(parsed.get_options().is_none());
    let db = PostgresDatabase::connect(&PostgresDatabaseConfig {
        url: config.database.url,
        max_connections: 4,
        min_connections: 0,
        acquire_timeout: config.database.acquire_timeout.into(),
        idle_timeout: None,
        max_lifetime: None,
    })
    .await
    .unwrap();
    let txn = db.begin_transaction().await.unwrap();
    let row = txn.txn().query_one("SELECT current_database()::text,current_user::text,inet_server_port(),current_setting('data_directory'),version()", &[]).await.unwrap();
    assert_eq!(row.get::<_, &str>(0), marker["database"].as_str().unwrap());
    assert_eq!(row.get::<_, &str>(1), marker["user"].as_str().unwrap());
    assert_eq!(row.get::<_, i32>(2), 56720);
    assert_eq!(
        PathBuf::from(row.get::<_, &str>(3)).canonicalize().unwrap(),
        root.join("pgdata").canonicalize().unwrap()
    );
    assert!(row.get::<_, &str>(4).starts_with("PostgreSQL 18.6"));
    println!(
        "verified before reset: {} / {} / 56720 / {} / {}",
        row.get::<_, &str>(0),
        row.get::<_, &str>(1),
        row.get::<_, &str>(3),
        row.get::<_, &str>(4)
    );
    txn.commit().await.unwrap();
    drop(db);
    let db = common::setup().await;
    if std::env::var("BOOTSTRAP_LEARNING_START_BASELINE").as_deref() == Ok("1") {
        db.execute(include_str!(
            "../migrations/2026-09-09-050000_learning_refresh_target/up.sql"
        ))
        .await
        .unwrap();
    }
    let txn = db.begin_transaction().await.unwrap();
    let case: Uuid = txn
        .txn()
        .query_one("SELECT commercial_lock_subject($1)", &[&*FOO.user.id])
        .await
        .unwrap()
        .get(0);
    txn.txn().execute("UPDATE commercial_cases SET contact='synthetic@example.invalid',contact_verified=true WHERE id=$1", &[&case]).await.unwrap();
    txn.txn().execute("INSERT INTO commercial_access_keys(hash,case_id,epoch,provenance) VALUES($1,$2,1,'{\"test\":true}')", &[&CLAIM,&case]).await.unwrap();
    txn.txn().execute("INSERT INTO moderation_capabilities(hash,subject,scope,expires_at) VALUES($1,$2,'rights',clock_timestamp()+interval '1 hour')", &[&RIGHTS,&*FOO.user.id]).await.unwrap();
    txn.commit().await.unwrap();
    (db, case)
}

fn request() -> Value {
    json!({"command_id":Uuid::new_v4(), "hash":Uuid::new_v4().simple().to_string().repeat(2),
        "_claim_hash":CLAIM, "use_retained_value":true, "expected_no_active_subject":true})
}

async fn call(db: &PostgresDatabase, operation: &str, body: Value) -> anyhow::Result<Value> {
    let mut txn = db.begin_transaction().await?;
    let result = Repo
        .commercial_operation(&mut txn, operation, Some(FOO.user.id), &body)
        .await?;
    txn.commit().await?;
    Ok(result)
}

async fn snapshot(db: &PostgresDatabase) -> Value {
    let txn = db.begin_transaction().await.unwrap();
    let row = txn.txn().query_one("SELECT jsonb_build_object('subjects',(SELECT jsonb_agg(s ORDER BY subject) FROM commercial_learning_subjects s),'keys',(SELECT jsonb_agg(k ORDER BY hash) FROM commercial_learning_keys k),'journal',(SELECT jsonb_agg(j ORDER BY command_id) FROM commercial_journal j),'cases',(SELECT jsonb_agg(c ORDER BY id) FROM commercial_cases c),'users',(SELECT jsonb_agg(u ORDER BY id) FROM users u),'profiles',(SELECT jsonb_agg(p ORDER BY user_id) FROM user_profiles p))::text", &[]).await.unwrap();
    serde_json::from_str(row.get(0)).unwrap()
}

async fn refused(db: &PostgresDatabase, body: Value) {
    let before = snapshot(db).await;
    let error = call(db, "learning_start", body)
        .await
        .expect_err("fresh guarded start must not follow an active subject");
    assert!(error.is::<ModerationConflict>(), "{error:?}");
    assert_eq!(
        snapshot(db).await,
        before,
        "no subject, key, journal, epoch or user mutation on refusal"
    );
}

async fn erase(db: &PostgresDatabase, subject: Uuid) {
    // Real owned SQL erasure target and erasure-envelope trigger. This fixture
    // supplies authenticated context; it is not paired HTTP/downstream erasure.
    let target = call(
        db,
        "learning_erasure_target",
        json!({"_claim_hash":CLAIM,"subject":subject,"erase_learning_data":true}),
    )
    .await
    .unwrap();
    assert_eq!(target["subject"], json!(subject));
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .query_one(
            "SELECT set_config('academy.moderation_erasure_subject',$1,true)",
            &[&subject.to_string()],
        )
        .await
        .unwrap();
    txn.txn()
        .execute(
            "SELECT commercial_record_erasure_intake($1,clock_timestamp())",
            &[&subject],
        )
        .await
        .unwrap();
    assert_eq!(
        txn.txn()
            .execute("DELETE FROM users WHERE id=$1", &[&subject])
            .await
            .unwrap(),
        1
    );
    txn.commit().await.unwrap();
    let txn = db.begin_transaction().await.unwrap();
    assert!(
        txn.txn()
            .query_one(
                "SELECT erased_at IS NOT NULL FROM commercial_learning_subjects WHERE subject=$1",
                &[&subject]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
}

#[tokio::test]
async fn guarded_empty_start_is_explicit_private_and_replays_the_exact_original_receipt() {
    let (db, case) = setup().await;
    let body = request();
    let result = call(&db, "learning_start", body.clone()).await.unwrap();
    let subject = Uuid::parse_str(result["subject"].as_str().unwrap()).unwrap();
    assert_eq!(result["purpose"], "retained_learning");
    for flag in [
        "ordinary_authority",
        "financial_authority",
        "claims_satisfied",
    ] {
        assert_eq!(result[flag], false);
    }
    let txn = db.begin_transaction().await.unwrap();
    let user = txn.txn().query_one("SELECT u.enabled,u.admin,u.email_verified,u.email,u.terms_version,u.terms_accepted_at,u.age_confirmed_at,p.leaderboard_opt_out FROM users u JOIN user_profiles p ON p.user_id=u.id WHERE u.id=$1", &[&subject]).await.unwrap();
    for column in [0, 1, 2] {
        assert!(!user.get::<_, bool>(column));
    }
    assert!(user.get::<_, Option<String>>(3).is_none());
    assert!(user.get::<_, Option<String>>(4).is_none());
    for column in [5, 6] {
        assert!(
            user.get::<_, Option<chrono::DateTime<chrono::Utc>>>(column)
                .is_none()
        );
    }
    assert!(user.get::<_, bool>(7));
    assert_eq!(
        txn.txn()
            .query_one(
                "SELECT case_id FROM commercial_learning_subjects WHERE subject=$1",
                &[&subject]
            )
            .await
            .unwrap()
            .get::<_, Uuid>(0),
        case
    );
    drop(txn);
    let before = snapshot(&db).await;
    assert_eq!(call(&db, "learning_start", body).await.unwrap(), result);
    assert_eq!(snapshot(&db).await, before);
}

#[tokio::test]
async fn guarded_start_requires_literal_true_and_existing_creation_prerequisites() {
    let (db, _) = setup().await;
    for guard in [
        Value::Null,
        json!(false),
        json!("true"),
        json!(1),
        json!({}),
        json!([]),
    ] {
        let mut body = request();
        body["expected_no_active_subject"] = guard;
        refused(&db, body).await;
    }
    let mut missing = request();
    missing
        .as_object_mut()
        .unwrap()
        .remove("use_retained_value");
    refused(&db, missing).await;
    let mut absent_proof = request();
    absent_proof.as_object_mut().unwrap().remove("_claim_hash");
    refused(&db, absent_proof).await;
    let txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .batch_execute("UPDATE commercial_cases SET contact_verified=false")
        .await
        .unwrap();
    txn.commit().await.unwrap();
    refused(&db, request()).await;
}

#[tokio::test]
async fn guarded_preparation_refuses_intervening_active_subjects_but_allows_current_absence() {
    let (db, _) = setup().await;
    let prepared = request();
    let first = call(&db, "learning_start", request()).await.unwrap();
    let first_id = Uuid::parse_str(first["subject"].as_str().unwrap()).unwrap();
    refused(&db, prepared.clone()).await; // another tab created S1
    erase(&db, first_id).await;
    let second = call(&db, "learning_start", request()).await.unwrap();
    let second_id = Uuid::parse_str(second["subject"].as_str().unwrap()).unwrap();
    assert_ne!(second_id, first_id);
    refused(&db, prepared.clone()).await; // S1 erased, active S2 exists
    erase(&db, second_id).await;
    let third = call(&db, "learning_start", prepared.clone()).await.unwrap();
    assert_ne!(third["subject"], first["subject"]);
    assert_ne!(third["subject"], second["subject"]);
    assert_eq!(snapshot(&db).await["subjects"].as_array().unwrap().len(), 3);
    let txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        txn.txn()
            .query_one(
                "SELECT count(*) FROM users WHERE id IN ($1,$2)",
                &[&first_id, &second_id]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    drop(txn);
    assert_eq!(call(&db, "learning_start", prepared).await.unwrap(), third);
}

#[tokio::test]
async fn guarded_and_legacy_receipts_replay_after_erasure_replacement_without_new_authority() {
    let (db, _) = setup().await;
    // Actual predecessor issuance followed by the additive function replacement.
    db.execute(include_str!(
        "../migrations/2026-09-09-050000_learning_refresh_target/up.sql"
    ))
    .await
    .unwrap();
    let mut legacy = request();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("expected_no_active_subject");
    let original = call(&db, "learning_start", legacy.clone()).await.unwrap();
    db.execute(include_str!(
        "../migrations/2026-09-10-010000_learning_start_guard/up.sql"
    ))
    .await
    .unwrap();
    let subject = Uuid::parse_str(original["subject"].as_str().unwrap()).unwrap();
    // Unguarded compatibility still intentionally issues for the existing subject.
    let mut compatible = request();
    compatible
        .as_object_mut()
        .unwrap()
        .remove("expected_no_active_subject");
    assert_eq!(
        call(&db, "learning_start", compatible).await.unwrap()["subject"],
        original["subject"]
    );
    erase(&db, subject).await;
    let guarded = request();
    let next = call(&db, "learning_start", guarded.clone()).await.unwrap();
    let next_id = Uuid::parse_str(next["subject"].as_str().unwrap()).unwrap();
    erase(&db, next_id).await;
    let active = call(&db, "learning_start", request()).await.unwrap();
    assert_ne!(active["subject"], next["subject"]);
    let before = snapshot(&db).await;
    for (body, receipt) in [(legacy.clone(), original), (guarded.clone(), next)] {
        assert_eq!(
            call(&db, "learning_start", body.clone()).await.unwrap(),
            receipt
        );
        assert!(
            call(&db, "learning_authority", json!({"hash":body["hash"]}))
                .await
                .unwrap()
                .is_null()
        );
        assert_eq!(snapshot(&db).await, before);
        let mut different = body.clone();
        different["hash"] = request()["hash"].clone();
        refused(&db, different).await;
        let mut no_proof = body;
        no_proof.as_object_mut().unwrap().remove("_claim_hash");
        refused(&db, no_proof).await;
    }
    let mut altered = legacy;
    altered["expected_no_active_subject"] = json!(true);
    refused(&db, altered).await;
}

#[tokio::test]
async fn guarded_two_prepared_first_commands_wait_on_the_actual_owner_and_only_one_creates() {
    let (db, _) = setup().await;
    let first = request();
    let second = request();
    let mut holder = db.begin_transaction().await.unwrap();
    let holder_pid = holder
        .txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get::<_, i32>(0);
    let receipt = Repo
        .commercial_operation(&mut holder, "learning_start", Some(FOO.user.id), &first)
        .await
        .unwrap();
    let worker_db = db.clone();
    let worker_body = second.clone();
    let (send, receive) = tokio::sync::oneshot::channel();
    let worker = tokio::spawn(async move {
        let mut txn = worker_db.begin_transaction().await.unwrap();
        let pid = txn
            .txn()
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .unwrap()
            .get::<_, i32>(0);
        send.send(pid).unwrap();
        let result = Repo
            .commercial_operation(&mut txn, "learning_start", Some(FOO.user.id), &worker_body)
            .await;
        if result.is_ok() {
            txn.commit().await.unwrap();
        }
        result
    });
    let waiter_pid = receive.await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let observer=db.begin_transaction().await.unwrap();
            let row=observer.txn().query_one("SELECT coalesce(wait_event_type='Lock',false), pg_blocking_pids(pid) FROM pg_stat_activity WHERE pid=$1", &[&waiter_pid]).await.unwrap();
            let waiting=row.get::<_,bool>(0) && row.get::<_,Vec<i32>>(1).contains(&holder_pid);
            drop(observer);
            if waiting {println!("actual current owner wait: holder={holder_pid}, waiter={waiter_pid}");break;}
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }).await.expect("second prepared command must wait on the first owning transaction");
    holder.commit().await.unwrap();
    let error = worker
        .await
        .unwrap()
        .expect_err("second fresh command must refuse after owner commit");
    assert!(error.is::<ModerationConflict>());
    let before = snapshot(&db).await;
    assert_eq!(before["subjects"].as_array().unwrap().len(), 1);
    assert_eq!(before["keys"].as_array().unwrap().len(), 1);
    assert_eq!(call(&db, "learning_start", first).await.unwrap(), receipt);
    refused(&db, second).await;
    assert_eq!(snapshot(&db).await, before);
}
