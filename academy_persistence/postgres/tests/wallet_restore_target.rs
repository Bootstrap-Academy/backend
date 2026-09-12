//! Owned ordinary wallet restoration controls. Synthetic proof hashes exercise SQL admission.
mod common;
use academy_persistence_contracts::{
    Database, Transaction,
    moderation::{ModerationConflict, ModerationRepository},
};
use academy_persistence_postgres::{
    PostgresDatabase, PostgresTransaction, moderation::PostgresModerationRepository as Repo,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};
use uuid::Uuid;
const FORWARD: &str = "2026-09-11-050000_wallet_restore_target";
const CLAIM: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn owner() -> Uuid {
    *academy_demo::user::FOO.user.id
}
async fn setup(pre_wallet: bool) -> PostgresDatabase {
    if pre_wallet {
        common::setup_before(FORWARD, true).await
    } else {
        common::setup().await
    }
}
async fn sql(tx: &PostgresTransaction, q: &str) {
    tx.txn().batch_execute(q).await.unwrap();
}
async fn op(tx: &mut PostgresTransaction, kind: &str, body: &Value) -> anyhow::Result<Value> {
    Repo.commercial_operation(tx, kind, Some(academy_demo::user::FOO.user.id), body)
        .await
}
async fn call(db: &PostgresDatabase, kind: &str, body: &Value) -> anyhow::Result<Value> {
    let mut tx = db.begin_transaction().await?;
    let r = op(&mut tx, kind, body).await?;
    tx.commit().await?;
    Ok(r)
}
async fn seed(db: &PostgresDatabase) -> (Uuid, Uuid, Uuid) {
    let tx = db.begin_transaction().await.unwrap();
    let c: Uuid = tx
        .txn()
        .query_one("SELECT commercial_lock_subject($1)", &[&owner()])
        .await
        .unwrap()
        .get(0);
    tx.txn().execute("UPDATE commercial_cases SET contact='wallet@example.invalid',contact_verified=true WHERE id=$1",&[&c]).await.unwrap();
    tx.txn().execute("INSERT INTO commercial_access_keys(hash,case_id,epoch,provenance) VALUES($1,$2,1,'{\"synthetic\":true}')",&[&CLAIM,&c]).await.unwrap();
    let o = Uuid::new_v4();
    tx.txn().execute("INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,status,original) VALUES($1,$2,'backend','synthetic-original-wallet','wallet_available',1000,'established','{\"available\":1000,\"withheld\":80,\"source\":\"synthetic_original\"}')",&[&o,&c]).await.unwrap();
    tx.commit().await.unwrap();
    let s = start(db).await;
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,10,7) ON CONFLICT(user_id) DO UPDATE SET coins=10,withheld_coins=7",&[&s]).await.unwrap();
    tx.commit().await.unwrap();
    (c, o, s)
}
async fn start(db: &PostgresDatabase) -> Uuid {
    let r=call(db,"learning_start",&json!({"command_id":Uuid::new_v4(),"hash":Uuid::new_v4().simple().to_string().repeat(2),"_claim_hash":CLAIM,"use_retained_value":true,"expected_no_active_subject":true})).await.unwrap();
    Uuid::parse_str(r["subject"].as_str().unwrap()).unwrap()
}
fn request(o: Uuid, s: Uuid) -> Value {
    json!({"command_id":Uuid::new_v4(),"obligation_id":o,"expected_subject":s,"units":125,"choose_coins":true,"_claim_hash":CLAIM})
}
async fn fingerprint(db: &PostgresDatabase) -> BTreeMap<String, String> {
    let tx = db.begin_transaction().await.unwrap();
    let mut out = BTreeMap::new();
    for row in tx
        .txn()
        .query(
            "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
            &[],
        )
        .await
        .unwrap()
    {
        let name: String = row.get(0);
        let q = format!(
            "SELECT md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]')::text) FROM public.\"{}\" t",
            name.replace('"', "\"\"")
        );
        out.insert(name, tx.txn().query_one(&q, &[]).await.unwrap().get(0));
    }
    out
}
async fn refused(db: &PostgresDatabase, body: &Value) {
    let before = fingerprint(db).await;
    let err = call(db, "restore_credit", body)
        .await
        .expect_err("must refuse exact-target request");
    assert!(err.is::<ModerationConflict>(), "{err:?}");
    assert_eq!(fingerprint(db).await, before);
}
#[tokio::test]
async fn wallet_target_refuses_missing_wrong_type_foreign_and_never_follows_active() {
    let db = setup(false).await;
    let (_, o, s) = seed(&db).await;
    let mut b = request(o, s);
    b.as_object_mut().unwrap().remove("expected_subject");
    refused(&db, &b).await;
    for target in [
        Value::Null,
        json!(false),
        json!(15),
        json!([]),
        json!({}),
        json!("bad-uuid"),
        json!(owner()),
        json!(Uuid::new_v4()),
    ] {
        let mut b = request(o, s);
        b["expected_subject"] = target;
        refused(&db, &b).await;
    }
    let receipt = call(&db, "restore_credit", &request(o, s)).await.unwrap();
    assert_eq!(receipt["subject"], json!(s));
    assert_eq!(receipt["wallet_balance"], 135);
    assert!(receipt["claimant_cash_capacity"].is_null());
    println!(
        "WALLET_GROUP exact target and null-safe schema refusals; accepted exact S with nullable cash"
    );
}
#[tokio::test]
async fn wallet_isolation_entry_precedes_malformed_cast_and_restoration() {
    let db = setup(false).await;
    let (_, o, s) = seed(&db).await;
    for level in ["REPEATABLE READ", "SERIALIZABLE", "READ UNCOMMITTED"] {
        for body in [request(o, s), json!({"command_id":"malformed"})] {
            let tx = db.begin_transaction().await.unwrap();
            sql(&tx, &format!("SET TRANSACTION ISOLATION LEVEL {level}")).await;
            let e = tx
                .txn()
                .query_one(
                    "SELECT commercial_operation('restore_credit',$1,$2::text::jsonb)",
                    &[&owner(), &body.to_string()],
                )
                .await
                .expect_err("restore isolation must refuse before casts");
            assert_eq!(e.as_db_error().unwrap().code().code(), "P0001");
            assert!(
                e.as_db_error()
                    .unwrap()
                    .message()
                    .contains("READ COMMITTED")
            );
        }
    }
    println!("WALLET_GROUP restore-only RC entry before malformed command cast");
}
async fn begin_erase(db: &PostgresDatabase, s: Uuid) -> PostgresTransaction {
    let r = call(
        db,
        "learning_erasure_target",
        &json!({"_claim_hash":CLAIM,"subject":s,"erase_learning_data":true}),
    )
    .await
    .unwrap();
    assert_eq!(r["subject"], json!(s));
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .query_one(
            "SELECT set_config('academy.moderation_erasure_subject',$1,true)",
            &[&s.to_string()],
        )
        .await
        .unwrap();
    tx.txn()
        .execute(
            "SELECT commercial_record_erasure_intake($1,clock_timestamp())",
            &[&s],
        )
        .await
        .unwrap();
    assert_eq!(
        tx.txn()
            .execute("DELETE FROM users WHERE id=$1", &[&s])
            .await
            .unwrap(),
        1
    );
    tx
}
async fn pid(tx: &PostgresTransaction) -> i32 {
    tx.txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0)
}
async fn worker(
    db: &PostgresDatabase,
    body: Value,
) -> (i32, tokio::task::JoinHandle<anyhow::Result<Value>>) {
    let db = db.clone();
    let (send, recv) = tokio::sync::oneshot::channel();
    let job = tokio::spawn(async move {
        let mut tx = db.begin_transaction().await.unwrap();
        send.send(pid(&tx).await).unwrap();
        let r = op(&mut tx, "restore_credit", &body).await?;
        tx.commit().await?;
        Ok(r)
    });
    (recv.await.unwrap(), job)
}
async fn wait_for(db: &PostgresDatabase, waiter: i32, holder: i32, label: &str) {
    let observed=tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {let tx=db.begin_transaction().await.unwrap();let r=tx.txn().query_one("SELECT coalesce(wait_event_type='Lock',false),pg_blocking_pids(pid),coalesce(wait_event,'') FROM pg_stat_activity WHERE pid=$1",&[&waiter]).await.unwrap();let blocking:Vec<i32>=r.get(1);if r.get::<_,bool>(0)&&blocking.contains(&holder){break json!({"label":label,"waiter_pid":waiter,"holder_pid":holder,"blocking_pids":blocking,"wait_event":r.get::<_,String>(2)});}drop(tx);tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}).await.expect("actual expected owning waiter");
    println!("ACTUAL_WAIT {observed}");
    let root = PathBuf::from(std::env::var("ACADEMY_UNIT_TEST_FIXTURE").unwrap());
    std::fs::write(
        root.join("evidence")
            .join(format!("wait-{}.json", Uuid::new_v4())),
        serde_json::to_vec_pretty(&observed).unwrap(),
    )
    .unwrap();
}
#[tokio::test]
async fn wallet_legacy_real_receipts_survive_upgrade_erasure_replacement_and_isolation_refusal() {
    let db = setup(true).await;
    let (c, o, s) = seed(&db).await;
    let mut old = request(o, s);
    old.as_object_mut().unwrap().remove("expected_subject");
    old["units"] = json!("125");
    old["legacy_extra"] = json!({"ignored":true});
    let mut extra = request(o, s);
    extra["expected_subject"] = json!({"previously":"ignored"});
    let r1 = call(&db, "restore_credit", &old).await.unwrap();
    let r2 = call(&db, "restore_credit", &extra).await.unwrap();
    let before = fingerprint(&db).await;
    assert_eq!(
        db.run_migrations(None).await.unwrap(),
        vec![FORWARD, "2026-09-12-070000_legacy_moderation_email"]
    );
    let after = fingerprint(&db).await;
    for (k, v) in &before {
        if k != "_migrations" {
            assert_eq!(after[k], *v, "forward must not change rows of {k}");
        }
    }
    for (b, r) in [(&old, &r1), (&extra, &r2)] {
        assert_eq!(call(&db, "restore_credit", b).await.unwrap(), *r);
        let mut changed = b.clone();
        changed["command_id"] = json!(Uuid::new_v4());
        refused(&db, &changed).await;
        for (key, value) in [
            ("expected_subject", json!(s)),
            ("units", json!(126)),
            ("choose_coins", json!(false)),
            ("obligation_id", json!(Uuid::new_v4())),
            ("extra_changed", json!(true)),
        ] {
            let mut changed = b.clone();
            changed[key] = value;
            refused(&db, &changed).await;
        }
    }
    begin_erase(&db, s).await.commit().await.unwrap();
    let s2 = start(&db).await;
    assert_ne!(s, s2);
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request) VALUES($1,$2,750,'wallet','completed','{\"synthetic_remaining_use\":true}')",&[&Uuid::new_v4(),&o]).await.unwrap();
    tx.txn()
        .execute(
            "UPDATE commercial_cases SET contact_verified=false WHERE id=$1",
            &[&c],
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let before = fingerprint(&db).await;
    assert_eq!(call(&db, "restore_credit", &old).await.unwrap(), r1);
    assert_eq!(call(&db, "restore_credit", &extra).await.unwrap(), r2);
    assert_eq!(fingerprint(&db).await, before);
    assert_eq!(r1["subject"], json!(s));
    for level in ["REPEATABLE READ", "SERIALIZABLE", "READ UNCOMMITTED"] {
        let mut tx = db.begin_transaction().await.unwrap();
        sql(&tx, &format!("SET TRANSACTION ISOLATION LEVEL {level}")).await;
        assert!(
            op(&mut tx, "restore_credit", &old)
                .await
                .unwrap_err()
                .is::<ModerationConflict>()
        );
    }
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .execute(
            "UPDATE commercial_access_keys SET revoked_at=clock_timestamp() WHERE hash=$1",
            &[&CLAIM],
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    refused(&db, &old).await;
    println!(
        "WALLET_GROUP genuine accepted685 old receipts then additive upgrade, exact business bodies and historical S1 replay; RC still gates replay"
    );
}
#[tokio::test]
async fn wallet_proof_and_contact_are_current_after_actual_command_user_case_obligation_waits() {
    for kind in ["command", "subject", "case", "obligation", "contact"] {
        let db = setup(false).await;
        let (c, o, s) = seed(&db).await;
        let b = request(o, s);
        let holder = db.begin_transaction().await.unwrap();
        let hp = pid(&holder).await;
        match kind {
            "command" => {
                holder.txn().query_one("SELECT pg_advisory_xact_lock(hashtextextended('commercial-command:'||$1::text,0))",&[&b["command_id"].as_str().unwrap()]).await.unwrap();
            }
            "subject" => {
                holder
                    .txn()
                    .query_one("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&s])
                    .await
                    .unwrap();
            }
            "case" | "contact" => {
                holder
                    .txn()
                    .query_one(
                        "SELECT id FROM commercial_cases WHERE id=$1 FOR UPDATE",
                        &[&c],
                    )
                    .await
                    .unwrap();
            }
            "obligation" => {
                holder
                    .txn()
                    .query_one(
                        "SELECT id FROM commercial_obligations WHERE id=$1 FOR UPDATE",
                        &[&o],
                    )
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let (wp, job) = worker(&db, b.clone()).await;
        wait_for(&db, wp, hp, kind).await;
        if kind == "contact" {
            holder
                .txn()
                .execute(
                    "UPDATE commercial_cases SET contact_verified=false WHERE id=$1",
                    &[&c],
                )
                .await
                .unwrap();
        } else {
            holder
                .txn()
                .execute(
                    "UPDATE commercial_access_keys SET revoked_at=clock_timestamp() WHERE hash=$1",
                    &[&CLAIM],
                )
                .await
                .unwrap();
        }
        holder.commit().await.unwrap();
        let after_revocation = fingerprint(&db).await;
        assert!(job.await.unwrap().unwrap_err().is::<ModerationConflict>());
        assert_eq!(fingerprint(&db).await, after_revocation);
    }
    println!(
        "WALLET_GROUP witnessed command/S-user/C/obligation waits and contact case wait all refuse changed authority"
    );
}
#[tokio::test]
async fn wallet_actual_erasure_first_refuses_and_restore_first_preserves_exact_residual() {
    let db = setup(false).await;
    let (_, o, s) = seed(&db).await;
    let b = request(o, s);
    let holder = begin_erase(&db, s).await;
    let hp = pid(&holder).await;
    let (wp, job) = worker(&db, b.clone()).await;
    wait_for(&db, wp, hp, "erasure-first").await;
    holder.commit().await.unwrap();
    assert!(job.await.unwrap().unwrap_err().is::<ModerationConflict>());
    let s2 = start(&db).await;
    refused(&db, &b).await;
    assert_ne!(s, s2);
    let tx = db.begin_transaction().await.unwrap();
    let n: i64 = tx
        .txn()
        .query_one(
            "SELECT count(*) FROM commercial_reservations WHERE obligation_id=$1",
            &[&o],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 0);
    drop(tx);
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO coins(user_id,coins,withheld_coins) VALUES($1,10,7) ON CONFLICT(user_id) DO UPDATE SET coins=10,withheld_coins=7",&[&s2]).await.unwrap();
    tx.commit().await.unwrap();
    let b2 = request(o, s2);
    let mut holder = db.begin_transaction().await.unwrap();
    let hp = pid(&holder).await;
    let receipt = op(&mut holder, "restore_credit", &b2).await.unwrap();
    // Resolve the exact target before DELETE; deletion itself waits for restore's S row.
    let target = call(
        &db,
        "learning_erasure_target",
        &json!({"_claim_hash":CLAIM,"subject":s2,"erase_learning_data":true}),
    )
    .await
    .unwrap();
    assert_eq!(target["subject"], json!(s2));
    let workerdb = db.clone();
    let (send, recv) = tokio::sync::oneshot::channel();
    let erase = tokio::spawn(async move {
        let tx = workerdb.begin_transaction().await.unwrap();
        send.send(pid(&tx).await).unwrap();
        tx.txn()
            .query_one(
                "SELECT set_config('academy.moderation_erasure_subject',$1,true)",
                &[&s2.to_string()],
            )
            .await
            .unwrap();
        tx.txn()
            .execute(
                "SELECT commercial_record_erasure_intake($1,clock_timestamp())",
                &[&s2],
            )
            .await
            .unwrap();
        assert_eq!(
            tx.txn()
                .execute("DELETE FROM users WHERE id=$1", &[&s2])
                .await
                .unwrap(),
            1
        );
        tx.commit().await.unwrap();
    });
    let wp = recv.await.unwrap();
    wait_for(&db, wp, hp, "restore-first").await;
    holder.commit().await.unwrap();
    erase.await.unwrap();
    let tx = db.begin_transaction().await.unwrap();
    let r=tx.txn().query_one("SELECT evidence::text FROM commercial_evidence WHERE category='wallet_boundary' AND source_key=$1",&[&s2.to_string()]).await.unwrap();
    let observed: Value = serde_json::from_str(r.get(0)).unwrap();
    assert_eq!(observed["available"], 135);
    assert_eq!(observed["withheld"], 7);
    drop(tx);
    let before = fingerprint(&db).await;
    assert_eq!(call(&db, "restore_credit", &b2).await.unwrap(), receipt);
    assert_eq!(fingerprint(&db).await, before);
    println!(
        "WALLET_GROUP both actual DELETE/restore actor orders; old obligation unconsumed or once restored then residual preserved"
    );
}
#[tokio::test]
async fn wallet_same_command_wait_replays_and_distinct_competing_amounts_serialize() {
    let db = setup(false).await;
    let (_, o, s) = seed(&db).await;
    let mut b = request(o, s);
    b["units"] = json!(600);
    let mut holder = db.begin_transaction().await.unwrap();
    let hp = pid(&holder).await;
    let receipt = op(&mut holder, "restore_credit", &b).await.unwrap();
    let (wp, job) = worker(&db, b.clone()).await;
    wait_for(&db, wp, hp, "same-command-original").await;
    holder.commit().await.unwrap();
    assert_eq!(job.await.unwrap().unwrap(), receipt);
    let mut remaining = request(o, s);
    remaining["units"] = json!(300);
    let mut holder = db.begin_transaction().await.unwrap();
    let hp = pid(&holder).await;
    op(&mut holder, "restore_credit", &remaining).await.unwrap();
    let mut competitor = request(o, s);
    competitor["units"] = json!(200);
    let (wp, job) = worker(&db, competitor.clone()).await;
    wait_for(&db, wp, hp, "different-command-same-wallet").await;
    holder.commit().await.unwrap();
    assert!(job.await.unwrap().unwrap_err().is::<ModerationConflict>());
    let tx = db.begin_transaction().await.unwrap();
    let r=tx.txn().query_one("SELECT commercial_remaining($1),(SELECT count(*) FROM commercial_reservations WHERE obligation_id=$1),(SELECT count(*) FROM transactions WHERE user_id=$2),(SELECT withheld_coins FROM coins WHERE user_id=$2)",&[&o,&s]).await.unwrap();
    assert_eq!(r.get::<_, i64>(0), 100);
    assert_eq!(r.get::<_, i64>(1), 2);
    assert_eq!(r.get::<_, i64>(2), 2);
    assert_eq!(r.get::<_, i64>(3), 7);
    println!(
        "WALLET_GROUP exact concurrent replay and competing remaining amounts; once-only effects and withheld preserved"
    );
}
async fn staff(db: &PostgresDatabase, c: Uuid) -> Value {
    let session = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .execute("UPDATE users SET admin=true WHERE id=$1", &[&owner()])
        .await
        .unwrap();
    tx.txn().execute("INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified) VALUES($1,$2,clock_timestamp(),clock_timestamp(),true)",&[&session,&owner()]).await.unwrap();
    tx.txn()
        .execute(
            "INSERT INTO session_refresh_tokens(session_id,refresh_token_hash) VALUES($1,$2)",
            &[&session, &vec![0x5au8; 32]],
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    json!({"command_id":Uuid::new_v4(),"case_id":c,"_staff_session":session,"_staff_refresh_hash":"5a".repeat(32),"assessment":"Synthetic verified prior refund records and captured original facts checked.","prior_refund_units":0,"legacy_refund_records_checked":true,"evidence":{"synthetic":true}})
}
#[tokio::test]
async fn wallet_cash_branch_upgrade_replay_and_split_remaining_preserve_all_value() {
    let db = setup(true).await;
    let (c, o, s) = seed(&db).await;
    let body = staff(&db, c).await;
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,captured_at,coins,invoice_number) VALUES('synthetic-wallet-capture',$1,clock_timestamp(),clock_timestamp(),2000,90000001)",&[&owner()]).await.unwrap();
    tx.commit().await.unwrap();
    let old = call(&db, "cash_basis_review", &body).await.unwrap();
    assert_eq!(old["cash_capacity"], 2000);
    let before = fingerprint(&db).await;
    assert_eq!(
        db.run_migrations(None).await.unwrap(),
        vec![FORWARD, "2026-09-12-070000_legacy_moderation_email"]
    );
    let after = fingerprint(&db).await;
    for (k, v) in &before {
        if k != "_migrations" {
            assert_eq!(after[k], *v);
        }
    }
    for level in [
        "READ COMMITTED",
        "READ UNCOMMITTED",
        "REPEATABLE READ",
        "SERIALIZABLE",
    ] {
        let mut tx = db.begin_transaction().await.unwrap();
        sql(&tx, &format!("SET TRANSACTION ISOLATION LEVEL {level}")).await;
        assert_eq!(op(&mut tx, "cash_basis_review", &body).await.unwrap(), old);
        tx.commit().await.unwrap();
    }
    let mut correction = body.clone();
    correction["command_id"] = json!(Uuid::new_v4());
    correction["corrects_previous_assessment"] = json!(true);
    correction["prior_refund_units"] = json!(100);
    let mut tx = db.begin_transaction().await.unwrap();
    sql(&tx, "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").await;
    let r = op(&mut tx, "cash_basis_review", &correction).await.unwrap();
    assert_eq!(r["cash_capacity"], 1900);
    assert_eq!(r["paid"], false);
    tx.commit().await.unwrap();
    let parent = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request) VALUES($1,$2,300,'cash','uncertain','{\"synthetic_held\":true}')",&[&parent,&o]).await.unwrap();
    tx.txn().execute("INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request) VALUES($1,$2,200,'wallet','completed','{\"synthetic_previous_return\":true}'),($3,$2,400,'wallet','failed','{\"synthetic_failed\":true}')",&[&Uuid::new_v4(),&o,&Uuid::new_v4()]).await.unwrap();
    tx.commit().await.unwrap();
    let mut split = body.clone();
    split["command_id"] = json!(Uuid::new_v4());
    split["reservation_id"] = json!(parent);
    split["children"] = json!([{"id":Uuid::new_v4(),"units":100,"purchase_capacity_units":100},{"id":Uuid::new_v4(),"units":200,"purchase_capacity_units":200}]);
    call(&db, "split_cash", &split).await.unwrap();
    let tx = db.begin_transaction().await.unwrap();
    assert_eq!(
        tx.txn()
            .query_one("SELECT commercial_remaining($1)", &[&o])
            .await
            .unwrap()
            .get::<_, i64>(0),
        500
    );
    drop(tx);
    let mut excessive = request(o, s);
    excessive["units"] = json!(501);
    refused(&db, &excessive).await;
    for amount in [Value::Null, json!(0), json!(-1)] {
        let mut b = request(o, s);
        b["units"] = amount;
        refused(&db, &b).await;
    }
    let b = request(o, s);
    let receipt = call(&db, "restore_credit", &b).await.unwrap();
    assert_eq!(receipt["units_returned"], 125);
    assert_eq!(receipt["wallet_balance"], 135);
    assert_eq!(receipt["cash_paid"], false);
    assert_eq!(receipt["value_expires"], false);
    assert_eq!(receipt["claimant_cash_capacity"], 1600);
    let original = fingerprint(&db).await;
    assert_eq!(call(&db, "restore_credit", &b).await.unwrap(), receipt);
    assert_eq!(fingerprint(&db).await, original);
    let tx = db.begin_transaction().await.unwrap();
    let r=tx.txn().query_one("SELECT commercial_remaining($1),(SELECT withheld_coins FROM coins WHERE user_id=$2),(SELECT count(*) FROM commercial_evidence WHERE case_id=$3 AND category='learning_ledger'),(SELECT count(*) FROM transactions WHERE user_id=$2),(SELECT count(*) FROM commercial_cash_payments),(SELECT original::text FROM commercial_obligations WHERE id=$1)",&[&o,&s,&c]).await.unwrap();
    assert_eq!(r.get::<_, i64>(0), 375);
    assert_eq!(r.get::<_, i64>(1), 7);
    assert_eq!(r.get::<_, i64>(2), 1);
    assert_eq!(r.get::<_, i64>(3), 1);
    assert_eq!(r.get::<_, i64>(4), 0);
    let original: Value = serde_json::from_str(r.get(5)).unwrap();
    assert_eq!(original["withheld"], 80);
    drop(tx);
    for (status, units) in [
        ("established", None),
        ("pending_evidence", Some(10i64)),
        ("historical_wallet_application", Some(10)),
        ("rejected", Some(10)),
        ("established", Some(0)),
    ] {
        let id = Uuid::new_v4();
        let tx = db.begin_transaction().await.unwrap();
        tx.txn().execute("INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,status,original) VALUES($1,$2,'backend',$3,'wallet_withheld',$4,$5,'{\"synthetic\":true}')",&[&id,&c,&id.to_string(),&units,&status]).await.unwrap();
        tx.commit().await.unwrap();
        refused(&db, &request(id, s)).await;
    }
    println!(
        "WALLET_GROUP unchanged cash branch/replay/strong-isolation behavior; split parents excluded, held leaves conserved, partial amount and unknown/status refusals"
    );
}
#[tokio::test]
async fn wallet_foreign_lock_avoidance_and_forward_only_preserve_immediate_guards() {
    let db = setup(false).await;
    let (_, o, s) = seed(&db).await;
    let mut tx = db.begin_transaction().await.unwrap();
    let other = academy_demo::user::BAR.user.id;
    let c: Uuid = tx
        .txn()
        .query_one("SELECT commercial_lock_subject($1)", &[&*other])
        .await
        .unwrap()
        .get(0);
    tx.txn().execute("UPDATE commercial_cases SET contact='other@example.invalid',contact_verified=true WHERE id=$1",&[&c]).await.unwrap();
    let other_claim = "b".repeat(64);
    tx.txn().execute("INSERT INTO commercial_access_keys(hash,case_id,epoch,provenance) VALUES($1,$2,1,'{}')",&[&other_claim,&c]).await.unwrap();
    let foreign_receipt=Repo.commercial_operation(&mut tx,"learning_start",Some(other),&json!({"command_id":Uuid::new_v4(),"hash":"c".repeat(64),"_claim_hash":other_claim,"use_retained_value":true,"expected_no_active_subject":true})).await.unwrap();
    let foreign = Uuid::parse_str(foreign_receipt["subject"].as_str().unwrap()).unwrap();
    tx.commit().await.unwrap();
    let holder = db.begin_transaction().await.unwrap();
    holder
        .txn()
        .query_one("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&foreign])
        .await
        .unwrap();
    let b = request(o, foreign);
    tokio::time::timeout(std::time::Duration::from_secs(2), refused(&db, &b))
        .await
        .expect("foreign discovery must refuse before foreign user lock");
    drop(holder);
    let before = fingerprint(&db).await;
    let e = db.revert_migrations(Some(1)).await.unwrap_err();
    assert!(
        format!("{e:#}").contains(
            "Historical moderation email protection requires a reviewed forward migration"
        )
    );
    assert_eq!(fingerprint(&db).await, before);
    // The newest migration refuses first; the wallet's own protection must also remain intact.
    let tx = db.begin_transaction().await.unwrap();
    let wallet = academy_persistence_postgres::MIGRATIONS
        .iter()
        .find(|m| m.name == FORWARD)
        .unwrap();
    let error = tx.txn().batch_execute(wallet.down).await.unwrap_err();
    assert_eq!(error.code().unwrap().code(), "P0001");
    assert!(
        error
            .as_db_error()
            .unwrap()
            .message()
            .contains("forward repair")
    );
    tx.rollback().await.unwrap();
    assert_eq!(fingerprint(&db).await, before);
    let tx = db.begin_transaction().await.unwrap();
    let r=tx.txn().query_one("SELECT pg_get_functiondef('commercial_operation(text,uuid,jsonb)'::regprocedure),pg_get_functiondef('commercial_retention_operation(text,uuid,jsonb)'::regprocedure),pg_get_functiondef('commercial_learning_operation(text,uuid,jsonb)'::regprocedure)",&[]).await.unwrap();
    assert!(r.get::<_, String>(0).contains("admin_retention_page"));
    assert!(r.get::<_, String>(1).contains("READ COMMITTED"));
    assert!(r.get::<_, String>(2).contains("expected_no_active_subject"));
    drop(tx);
    let b = request(o, s);
    assert_eq!(
        call(&db, "restore_credit", &b).await.unwrap()["subject"],
        json!(s)
    );
    let before = fingerprint(&db).await;
    let mut changed_actor = b.clone();
    changed_actor["_claim_hash"] = json!("b".repeat(64));
    let mut tx = db.begin_transaction().await.unwrap();
    let err = Repo
        .commercial_operation(&mut tx, "restore_credit", Some(other), &changed_actor)
        .await
        .unwrap_err();
    assert!(err.is::<ModerationConflict>());
    drop(tx);
    assert_eq!(fingerprint(&db).await, before);
    println!(
        "WALLET_GROUP foreign lock avoided; wallet down refuses with current paging/invoice/start guards preserved"
    );
}
