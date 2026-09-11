//! Ordinary synthetic SQL only. Every destructive reset follows parsed configuration
//! and live server identity checks against this unit's new marked fixture.
mod common;
use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{PostgresDatabase, PostgresTransaction};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;
const FORWARD: &str = "2026-09-11-010000_retained_hold_review";
const TABLES: [&str; 4] = [
    "commercial_document_holds",
    "commercial_contract_holds",
    "commercial_renewal_holds",
    "commercial_legacy_renewal_holds",
];
const KEYS: [&str; 4] = ["number", "declaration_id", "agreement_id", "user_id"];
async fn setup() -> PostgresDatabase {
    common::setup().await
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
        let quoted = name.replace('"', "\"\"");
        let hash:String=tx.txn().query_one(&format!("SELECT md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) FROM \"{quoted}\" t"),&[]).await.unwrap().get(0);
        out.insert(name, hash);
    }
    tx.commit().await.unwrap();
    out
}

async fn staff_body(db: &PostgresDatabase, c: Uuid, o: Uuid) -> Value {
    let tx = db.begin_transaction().await.unwrap();
    let staff = *academy_demo::user::FOO.user.id;
    tx.txn()
        .execute(
            "UPDATE users SET enabled=true,admin=true WHERE id=$1",
            &[&staff],
        )
        .await
        .unwrap();
    let session = Uuid::new_v4();
    let hash = vec![0x5au8; 32];
    tx.txn().execute("INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified) VALUES($1,$2,clock_timestamp(),clock_timestamp(),true)",&[&session,&staff]).await.unwrap();
    tx.txn()
        .execute(
            "INSERT INTO session_refresh_tokens(session_id,refresh_token_hash) VALUES($1,$2)",
            &[&session, &hash],
        )
        .await
        .unwrap();
    let b = json!({"case_id":c,"subject":o,"_staff_session":session,"_staff_refresh_hash":"5a".repeat(32)});
    tx.commit().await.unwrap();
    b
}

async fn sql(db: &PostgresDatabase, s: &str) {
    let t = db.begin_transaction().await.unwrap();
    t.txn().batch_execute(s).await.unwrap();
    t.commit().await.unwrap();
}
async fn query(
    t: &PostgresTransaction,
    s: &str,
    args: &[&(dyn bb8_postgres::tokio_postgres::types::ToSql + Sync)],
) -> Value {
    serde_json::from_str(
        &t.txn()
            .query_one(s, args)
            .await
            .unwrap()
            .get::<_, String>(0),
    )
    .unwrap()
}
async fn op_tx(t: &PostgresTransaction, op: &str, b: &Value) -> Result<Value, String> {
    let actor = *academy_demo::user::FOO.user.id;
    let row = t
        .txn()
        .query_one(
            "SELECT commercial_operation($1,$2,$3::text::jsonb)::text",
            &[&op, &actor, &b.to_string()],
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(serde_json::from_str(&row.get::<_, String>(0)).unwrap())
}
async fn op(db: &PostgresDatabase, op: &str, b: &Value) -> Result<Value, String> {
    let t = db.begin_transaction().await.unwrap();
    let r = op_tx(&t, op, b).await;
    if r.is_ok() {
        t.commit().await.unwrap();
    }
    r
}
async fn queue(db: &PostgresDatabase, b: &Value, cursor: Value, limit: u32) -> Value {
    let mut q = json!({"version":1,"limit":limit,"cursor":cursor});
    q["_staff_session"] = b["_staff_session"].clone();
    q["_staff_refresh_hash"] = b["_staff_refresh_hash"].clone();
    op(db, "hold_queue", &q).await.unwrap()
}
async fn seed(db: &PostgresDatabase) -> (Value, Vec<Value>) {
    let o = *academy_demo::user::BAR.user.id;
    let c = Uuid::new_v4();
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO commercial_cases(id,subject) VALUES($1,$2)",
            &[&c, &o],
        )
        .await
        .unwrap();
    let id = Uuid::new_v4();
    let number = "LC1 original text / ä 10000000";
    t.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES($1,'credit_note',$2,'2020-01-01',ARRAY['synthetic original'])",&[&number,&o]).await.unwrap();
    t.txn().execute("INSERT INTO contract_declarations(id,kind,received_at,name,email,user_id,contract,details) VALUES($1,'withdrawal','2020-01-01','synthetic','fixture.invalid',$2,'other','original evidence')",&[&id,&o]).await.unwrap();
    t.txn().execute("INSERT INTO premium_renewal_agreements(id,user_id,received_at,offer_id,monthly_price,recipient,document,terms_pdf,withdrawal_pdf) VALUES($1,$2,'2020-01-01','synthetic',10,'original','original text',$3,$3)",&[&id,&o,&b"%PDF original".to_vec()]).await.unwrap();
    t.txn().execute("INSERT INTO premium_legacy_renewals(user_id,plan,paid_periods) VALUES($1,'monthly','[]') ON CONFLICT DO NOTHING",&[&o]).await.unwrap();
    let records = [
        number.to_owned(),
        id.to_string(),
        id.to_string(),
        o.to_string(),
    ];
    for (i, key) in records.iter().enumerate() {
        t.txn().execute(&format!("INSERT INTO {}(case_id,{},basis,review_due_at) VALUES($1,$2{},'specific preserved original basis','2000-01-01')",TABLES[i],KEYS[i],if i==0{""}else{"::text::uuid"}),&[&c,key]).await.unwrap();
    }
    t.commit().await.unwrap();
    let b = staff_body(db, c, o).await;
    let q = queue(db, &b, Value::Null, 100).await;
    assert_eq!(q["rows"].as_array().unwrap().len(), 4);
    (b, q["rows"].as_array().unwrap().clone())
}
fn review(b: &Value, h: &Value) -> Value {
    json!({"version":1,"command_id":Uuid::new_v4(),"case_id":h["case_id"],"subject":h["subject"],"hold":h["hold"],
 "expected":{"incarnation_id":h["incarnation_id"],"review_version":h["review_version"]},"decision":"keep","review_scope":"entire_existing_hold",
 "assessment":"Synthetic individual assessment for this exact original hold","next_review_at":"2500-01-01T01:00:00.123456+01:00",
 "_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]})
}
fn original_only(mut f: BTreeMap<String, String>) -> BTreeMap<String, String> {
    for t in [
        "commercial_journal",
        "commercial_document_holds",
        "commercial_contract_holds",
        "commercial_renewal_holds",
        "commercial_legacy_renewal_holds",
    ] {
        f.remove(t);
    }
    f
}
#[tokio::test]
async fn hold_review_four_families_exact_history_and_original_preservation() {
    let db = setup().await;
    let (b, rows) = seed(&db).await;
    let originals = original_only(fingerprint(&db).await);
    for h in &rows {
        assert_eq!(h["review_version"], "0");
        assert!(h["last_review"].is_null());
        let body = review(&b, h);
        let receipt = op(&db, "hold_review", &body).await.unwrap();
        assert_eq!(receipt["previous_review_version"], "0");
        assert_eq!(receipt["review_version"], "1");
        assert_eq!(receipt["next_review_at"], "2500-01-01 00:00:00.123456+00");
        assert_eq!(op(&db, "hold_review", &body).await.unwrap(), receipt);
        let mut changed = body.clone();
        changed["assessment"] = json!("A different sufficiently specific assessment");
        assert!(
            op(&db, "hold_review", &changed)
                .await
                .unwrap_err()
                .contains("Conflicting")
        );
        changed = body.clone();
        changed["command_id"] = json!(Uuid::new_v4());
        assert!(
            op(&db, "hold_review", &changed)
                .await
                .unwrap_err()
                .contains("already reviewed")
        );
        let q = queue(&db, &b, Value::Null, 100).await;
        let current = q["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["hold"] == h["hold"])
            .unwrap();
        assert_eq!(current["last_review"]["command_id"], body["command_id"]);
        assert_eq!(current["basis"], h["basis"]);
        let later = review(&b, current);
        let r2 = op(&db, "hold_review", &later).await.unwrap();
        assert_eq!(r2["review_version"], "2");
        assert_eq!(op(&db, "hold_review", &body).await.unwrap(), receipt);
    }
    assert_eq!(original_only(fingerprint(&db).await), originals);
    let t = db.begin_transaction().await.unwrap();
    let stored = query(
        &t,
        "SELECT jsonb_agg(to_jsonb(j))::text FROM commercial_journal j WHERE kind='hold_review'",
        &[],
    )
    .await;
    assert!(!stored.to_string().contains("_staff_"));
    assert!(!stored.to_string().contains(&"5a".repeat(32)));
    t.commit().await.unwrap();
    println!(
        "LC1_GROUP four kinds; exact replay; later review; stale version; original/basis fingerprints; no transient proof"
    );
}

async fn rejected_sql(db: &PostgresDatabase, s: &str) -> String {
    let t = db.begin_transaction().await.unwrap();
    format!("{:?}", t.txn().batch_execute(s).await.unwrap_err())
}
#[tokio::test]
async fn hold_review_incarnation_replay_conflicts_and_isolation() {
    let db = setup().await;
    let (b, rows) = seed(&db).await;
    let h = &rows[0];
    let mut body = review(&b, h);
    let t = db.begin_transaction().await.unwrap();
    body["next_review_at"] = json!(
        t.txn()
            .query_one("SELECT (clock_timestamp()+interval '1 second')::text", &[])
            .await
            .unwrap()
            .get::<_, String>(0)
    );
    t.commit().await.unwrap();
    let receipt = op(&db, "hold_review", &body).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let c: Uuid = serde_json::from_value(h["case_id"].clone()).unwrap();
    let key = h["hold"]["record_id"].as_str().unwrap();
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute(
            "DELETE FROM commercial_document_holds WHERE case_id=$1 AND number=$2",
            &[&c, &key],
        )
        .await
        .unwrap();
    t.commit().await.unwrap();
    assert_eq!(op(&db, "hold_review", &body).await.unwrap(), receipt);
    let mut absent = body.clone();
    absent["command_id"] = json!(Uuid::new_v4());
    assert!(
        op(&db, "hold_review", &absent)
            .await
            .unwrap_err()
            .contains("absent")
    );
    let t = db.begin_transaction().await.unwrap();
    t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,review_due_at,incarnation_id,review_version,review_command_id) VALUES($1,$2,'new hold basis','2000-01-01',$3,99,$4)",&[&c,&key,&serde_json::from_value::<Uuid>(h["incarnation_id"].clone()).unwrap(),&serde_json::from_value::<Uuid>(body["command_id"].clone()).unwrap()]).await.unwrap();
    t.commit().await.unwrap();
    let q = queue(&db, &b, Value::Null, 100).await;
    let new = q["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["hold"] == h["hold"])
        .unwrap();
    assert_ne!(new["incarnation_id"], h["incarnation_id"]);
    assert_eq!(new["review_version"], "0");
    assert!(new["last_review"].is_null());
    assert!(
        op(&db, "hold_review", &absent)
            .await
            .unwrap_err()
            .contains("replaced")
    );
    assert_eq!(op(&db, "hold_review", &body).await.unwrap(), receipt);
    let wrongs = [
        ("case_id", json!(Uuid::new_v4())),
        ("subject", json!(Uuid::new_v4())),
    ];
    for (k, v) in wrongs {
        let mut wrong = review(&b, new);
        wrong[k] = v;
        assert!(
            op(&db, "hold_review", &wrong)
                .await
                .unwrap_err()
                .contains("Exact existing original case")
        );
    }
    for level in ["REPEATABLE READ", "SERIALIZABLE"] {
        for replay in [true, false] {
            let t = db.begin_transaction().await.unwrap();
            t.txn()
                .batch_execute(&format!("SET TRANSACTION ISOLATION LEVEL {level}"))
                .await
                .unwrap();
            let (operation, request) = if replay {
                ("hold_review", body.clone())
            } else {
                let mut q = json!({"version":1,"limit":100,"cursor":null});
                q["_staff_session"] = b["_staff_session"].clone();
                q["_staff_refresh_hash"] = b["_staff_refresh_hash"].clone();
                ("hold_queue", q)
            };
            assert!(
                op_tx(&t, operation, &request)
                    .await
                    .unwrap_err()
                    .contains("READ COMMITTED")
            );
        }
    }
    for s in [
        "UPDATE commercial_document_holds SET basis='changed'",
        "UPDATE commercial_document_holds SET review_due_at='2500-01-01'",
        "UPDATE commercial_document_holds SET review_version=review_version+1",
        "UPDATE commercial_document_holds SET incarnation_id=gen_random_uuid()",
    ] {
        assert!(rejected_sql(&db, s).await.contains("immutable"));
    }
    // A different currently proved staff actor cannot adopt this exact old command.
    let actor = Uuid::new_v4();
    let session = Uuid::new_v4();
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute("INSERT INTO users(id,name,email_verified,created_at,enabled,admin) VALUES($1,'lc1-second-staff',false,clock_timestamp(),true,true)", &[&actor])
        .await
        .unwrap();
    t.txn().execute("INSERT INTO user_profiles(user_id,display_name,bio,tags) VALUES($1,'LC1 second staff','','{}')",&[&actor]).await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO user_invoice_info(user_id) VALUES($1)",
            &[&actor],
        )
        .await
        .unwrap();
    t.txn().execute("INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified) VALUES($1,$2,clock_timestamp(),clock_timestamp(),true)",&[&session,&actor]).await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO session_refresh_tokens(session_id,refresh_token_hash) VALUES($1,$2)",
            &[&session, &vec![0x5au8; 32]],
        )
        .await
        .unwrap();
    t.commit().await.unwrap();
    let mut other = body.clone();
    other["_staff_session"] = json!(session);
    let t = db.begin_transaction().await.unwrap();
    let e = t
        .txn()
        .query_one(
            "SELECT commercial_operation('hold_review',$1,$2::text::jsonb)::text",
            &[&actor, &other.to_string()],
        )
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("Conflicting"));
    drop(t);
    println!(
        "LC1_GROUP actual expired-date replay; released/reinserted ABA; stale incarnation; exact C/O; immutable basis; current different actor; RR/serializable replay and queue refusal"
    );
}

async fn wait_for_block(db: &PostgresDatabase, pid: i32, holder: i32) -> Value {
    for _ in 0..300 {
        let t = db.begin_transaction().await.unwrap();
        let r=query(&t,"SELECT jsonb_build_object('pid',pid,'wait_event_type',wait_event_type,'wait_event',wait_event,'blockers',pg_blocking_pids(pid))::text FROM pg_stat_activity WHERE pid=$1",&[&pid]).await;
        t.commit().await.unwrap();
        if r["blockers"]
            .as_array()
            .is_some_and(|a| a.contains(&json!(holder)))
        {
            println!("OBSERVED_ACTUAL_WAIT {r}");
            return r;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("Did not observe actual wait for holder {holder}")
}
async fn revoke_staff(db: &PostgresDatabase, b: &Value, mode: &str) {
    let t = db.begin_transaction().await.unwrap();
    let session: Uuid = serde_json::from_value(b["_staff_session"].clone()).unwrap();
    if mode == "refresh" {
        t.txn()
            .execute(
                "DELETE FROM session_refresh_tokens WHERE session_id=$1",
                &[&session],
            )
            .await
            .unwrap();
    } else if mode == "mfa" {
        t.txn()
            .execute(
                "UPDATE sessions SET mfa_verified=false WHERE id=$1",
                &[&session],
            )
            .await
            .unwrap();
    } else {
        let staff = *academy_demo::user::FOO.user.id;
        t.txn()
            .execute("UPDATE users SET admin=false WHERE id=$1", &[&staff])
            .await
            .unwrap();
    }
    t.commit().await.unwrap();
}
#[tokio::test]
async fn hold_review_actual_lock_waits_recheck_staff_and_competing_revision() {
    let db = setup().await;
    let (_, rows) = seed(&db).await;
    let h = &rows[0];
    for (index, lock) in ["command", "user", "case", "hold", "projection"]
        .iter()
        .enumerate()
    {
        let b = staff_body(
            &db,
            serde_json::from_value(h["case_id"].clone()).unwrap(),
            serde_json::from_value(h["subject"].clone()).unwrap(),
        )
        .await;
        let body = review(&b, h);
        let holder = db.begin_transaction().await.unwrap();
        let holder_pid: i32 = holder
            .txn()
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .unwrap()
            .get(0);
        match *lock {
            "command" => {
                holder.txn().query_one("SELECT pg_advisory_xact_lock(hashtextextended('commercial-command:'||$1::text,0))",&[&body["command_id"].as_str().unwrap()]).await.unwrap();
            }
            "user" => {
                holder
                    .txn()
                    .query_one(
                        "SELECT id FROM users WHERE id=$1 FOR UPDATE",
                        &[&serde_json::from_value::<Uuid>(h["subject"].clone()).unwrap()],
                    )
                    .await
                    .unwrap();
            }
            "case" => {
                holder
                    .txn()
                    .query_one(
                        "SELECT id FROM commercial_cases WHERE id=$1 FOR UPDATE",
                        &[&serde_json::from_value::<Uuid>(h["case_id"].clone()).unwrap()],
                    )
                    .await
                    .unwrap();
            }
            "hold" => {
                holder.txn().query_one("SELECT number FROM commercial_document_holds WHERE case_id=$1 AND number=$2 FOR UPDATE",&[&serde_json::from_value::<Uuid>(h["case_id"].clone()).unwrap(),&h["hold"]["record_id"].as_str().unwrap()]).await.unwrap();
            }
            "projection" => {
                holder
                    .txn()
                    .batch_execute("LOCK TABLE commercial_document_holds IN ACCESS EXCLUSIVE MODE")
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let request = if *lock == "projection" {
            json!({"version":1,"limit":100,"cursor":null,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]})
        } else {
            body
        };
        let operation = if *lock == "projection" {
            "hold_queue"
        } else {
            "hold_review"
        };
        let worker = db.clone();
        let (send, recv) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let t = worker.begin_transaction().await.unwrap();
            let pid: i32 = t
                .txn()
                .query_one("SELECT pg_backend_pid()", &[])
                .await
                .unwrap()
                .get(0);
            send.send(pid).unwrap();
            let r = op_tx(&t, operation, &request).await;
            if r.is_ok() {
                t.commit().await.unwrap();
            }
            r
        });
        let pid = recv.await.unwrap();
        wait_for_block(&db, pid, holder_pid).await;
        revoke_staff(&db, &b, ["mfa", "refresh", "admin"][index % 3]).await;
        holder.commit().await.unwrap();
        let r = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap();
        assert!(r.unwrap_err().contains("authority changed"), "{lock}");
        println!("LC1_WAIT_STAFF_REFUSAL {lock}");
    }
    // Two live decisions on the same old revision: the loser actually waits on O.
    let b = staff_body(
        &db,
        serde_json::from_value(h["case_id"].clone()).unwrap(),
        serde_json::from_value(h["subject"].clone()).unwrap(),
    )
    .await;
    let first = review(&b, h);
    let second = review(&b, h);
    let t = db.begin_transaction().await.unwrap();
    let receipt = op_tx(&t, "hold_review", &first).await.unwrap();
    let holder_pid: i32 = t
        .txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let worker = db.clone();
    let (send, recv) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let t = worker.begin_transaction().await.unwrap();
        let pid: i32 = t
            .txn()
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .unwrap()
            .get(0);
        send.send(pid).unwrap();
        let r = op_tx(&t, "hold_review", &second).await;
        if r.is_ok() {
            t.commit().await.unwrap();
        }
        r
    });
    wait_for_block(&db, recv.await.unwrap(), holder_pid).await;
    t.commit().await.unwrap();
    assert!(
        task.await
            .unwrap()
            .unwrap_err()
            .contains("already reviewed")
    );
    assert_eq!(op(&db, "hold_review", &first).await.unwrap(), receipt);
    // Even an existing exact receipt requires fresh staff after the command wait.
    let holder = db.begin_transaction().await.unwrap();
    holder
        .txn()
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended('commercial-command:'||$1::text,0))",
            &[&first["command_id"].as_str().unwrap()],
        )
        .await
        .unwrap();
    let holder_pid: i32 = holder
        .txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let worker = db.clone();
    let (send, recv) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let t = worker.begin_transaction().await.unwrap();
        send.send(
            t.txn()
                .query_one("SELECT pg_backend_pid()", &[])
                .await
                .unwrap()
                .get::<_, i32>(0),
        )
        .unwrap();
        op_tx(&t, "hold_review", &first).await
    });
    wait_for_block(&db, recv.await.unwrap(), holder_pid).await;
    revoke_staff(&db, &b, "mfa").await;
    holder.commit().await.unwrap();
    assert!(
        task.await
            .unwrap()
            .unwrap_err()
            .contains("authority changed")
    );
    println!(
        "LC1_GROUP five real waits; admin/MFA/refresh loss; competing revision; exact replay after command-wait staff loss"
    );
}

#[tokio::test]
async fn hold_queue_full_ties_native_timestamps_readonly_and_live_cursor() {
    let db = setup().await;
    let (b, _) = seed(&db).await;
    let c: Uuid = serde_json::from_value(b["case_id"].clone()).unwrap();
    let o: Uuid = serde_json::from_value(b["subject"].clone()).unwrap();
    let t = db.begin_transaction().await.unwrap();
    for n in 0..121 {
        let key = format!("queue {n:03} / ä");
        t.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES($1,'credit_note',$2,'2000-01-01',ARRAY['unchanged original'])",&[&key,&o]).await.unwrap();
        let date = match n {
            0 => "-infinity",
            1 => "2000-01-01 00:00:00.000001+00",
            2 => "2000-01-01 00:00:00.000002+00",
            3 => "280000-01-01 00:00:00.123456+00",
            4 => "infinity",
            5 => "0001-01-01 00:00:00.123456+00 BC",
            _ => "2000-01-01 00:00:00+00",
        };
        t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,review_due_at) VALUES($1,$2,'specific existing basis',$3::text::timestamptz)",&[&c,&key,&date]).await.unwrap();
    }
    for _ in 0..2 {
        let c2 = Uuid::new_v4();
        let o2 = Uuid::new_v4();
        t.txn()
            .execute(
                "INSERT INTO commercial_cases(id,subject) VALUES($1,$2)",
                &[&c2, &o2],
            )
            .await
            .unwrap();
        t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,review_due_at) VALUES($1,'queue 005 / ä','independent exact case','2000-01-01')",&[&c2]).await.unwrap();
    }
    t.commit().await.unwrap();
    let original = fingerprint(&db).await;
    let expected = {
        let t = db.begin_transaction().await.unwrap();
        let v=query(&t,"WITH h AS (SELECT 1 r,case_id,number k,incarnation_id,review_due_at FROM commercial_document_holds UNION ALL SELECT 2,case_id,declaration_id::text,incarnation_id,review_due_at FROM commercial_contract_holds UNION ALL SELECT 3,case_id,agreement_id::text,incarnation_id,review_due_at FROM commercial_renewal_holds UNION ALL SELECT 4,case_id,user_id::text,incarnation_id,review_due_at FROM commercial_legacy_renewal_holds) SELECT jsonb_agg(jsonb_build_array(case_id,k,incarnation_id) ORDER BY review_due_at,r,case_id,k COLLATE \"C\",incarnation_id)::text FROM h",&[]).await;
        t.commit().await.unwrap();
        v
    };
    let mut cursor = Value::Null;
    let mut seen = vec![];
    let mut dates = vec![];
    let mut all_rows = vec![];
    let mut page_count = 0;
    loop {
        let page = queue(&db, &b, cursor, 100).await;
        page_count += 1;
        for h in page["rows"].as_array().unwrap() {
            seen.push(json!([
                h["case_id"],
                h["hold"]["record_id"],
                h["incarnation_id"]
            ]));
            dates.push(h["review_due_at"].clone());
            all_rows.push(h.clone());
        }
        cursor = page["next_cursor"].clone();
        if page["exhausted"] == true {
            assert!(cursor.is_null());
            break;
        }
        assert!(!cursor.is_null());
    }
    assert_eq!(json!(seen), expected);
    assert_eq!(seen.len(), 127);
    assert_eq!(page_count, 2);
    for v in [
        "-infinity",
        "infinity",
        "280000-01-01 00:00:00.123456+00",
        "0001-01-01 00:00:00.123456+00 BC",
        "2000-01-01 00:00:00.000001+00",
        "2000-01-01 00:00:00.000002+00",
    ] {
        assert!(dates.contains(&json!(v)));
    }
    for (i, h) in all_rows.iter().enumerate().filter(|(_, h)| {
        h["review_due_at"].as_str().unwrap().contains(".123456")
            || h["review_due_at"].as_str().unwrap().contains(".00000")
            || h["review_due_at"].as_str().unwrap().contains("infinity")
    }) {
        let cursor = json!({"review_due_at":h["review_due_at"],"kind":h["hold"]["kind"],"case_id":h["case_id"],"record_id":h["hold"]["record_id"],"incarnation_id":h["incarnation_id"]});
        let next = queue(&db, &b, cursor, 1).await;
        if i + 1 < all_rows.len() {
            assert_eq!(next["rows"][0], all_rows[i + 1]);
        } else {
            assert!(next["rows"].as_array().unwrap().is_empty());
            assert_eq!(next["exhausted"], true);
        }
    }
    let t = db.begin_transaction().await.unwrap();
    t.txn().batch_execute("SET TRANSACTION READ ONLY; SET LOCAL DateStyle='SQL,DMY'; SET LOCAL TimeZone='Europe/Berlin'").await.unwrap();
    let request = json!({"version":1,"limit":100,"cursor":null,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]});
    let q = op_tx(&t, "hold_queue", &request).await.unwrap();
    assert_eq!(q["rows"][0]["review_due_at"], "-infinity");
    assert_eq!(
        t.txn()
            .query_one("SHOW DateStyle", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        "SQL, DMY"
    );
    assert_eq!(
        t.txn()
            .query_one("SHOW TimeZone", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        "Europe/Berlin"
    );
    t.commit().await.unwrap();
    assert_eq!(fingerprint(&db).await, original);
    // Release the exact cursor row. Continuation uses its key, not a row lookup.
    let page = queue(&db, &b, Value::Null, 1).await;
    let cursor = page["next_cursor"].clone();
    let key = cursor["record_id"].as_str().unwrap();
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute(
            "DELETE FROM commercial_document_holds WHERE case_id=$1 AND number=$2",
            &[&c, &key],
        )
        .await
        .unwrap();
    t.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES('new live head','credit_note',$1,'2000-01-01',ARRAY['original'])",&[&o]).await.unwrap();
    t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,review_due_at) VALUES($1,'new live head','later live insertion','-infinity')",&[&c]).await.unwrap();
    t.commit().await.unwrap();
    let next = queue(&db, &b, cursor.clone(), 100).await;
    assert!(
        !next["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["hold"]["record_id"] == "new live head")
    );
    assert_eq!(
        queue(&db, &b, Value::Null, 1).await["rows"][0]["hold"]["record_id"],
        "new live head"
    );
    for date in [
        "now",
        "2000-01-01T00:00:00Z",
        "999999-01-01 00:00:00+00",
        "2026-02-31 00:00:00+00",
        "INF",
    ] {
        let mut q = request.clone();
        q["cursor"] = cursor.clone();
        q["cursor"]["review_due_at"] = json!(date);
        assert!(op(&db, "hold_queue", &q).await.is_err(), "{date}");
    }
    for limit in [
        json!(0),
        json!(101),
        json!(1.5),
        json!("100"),
        json!(999999999999999999u64),
    ] {
        let mut q = request.clone();
        q["limit"] = limit;
        assert!(op(&db, "hold_queue", &q).await.is_err());
    }
    for date in [
        "infinity",
        "-infinity",
        "now",
        "2500-01-01",
        "999999-01-01T00:00:00+00",
        "2500-02-31T00:00:00+00",
        "2000-01-01T00:00:00+00",
    ] {
        let mut r = review(&b, &queue(&db, &b, Value::Null, 1).await["rows"][0]);
        r["next_review_at"] = json!(date);
        assert!(op(&db, "hold_review", &r).await.is_err(), "{date}");
    }
    let erased = all_rows
        .iter()
        .find(|h| h["subject"] != b["subject"])
        .unwrap();
    let originals = original_only(fingerprint(&db).await);
    let receipt = op(&db, "hold_review", &review(&b, erased)).await.unwrap();
    assert_eq!(receipt["subject"], erased["subject"]);
    assert_eq!(original_only(fingerprint(&db).await), originals);
    let t = db.begin_transaction().await.unwrap();
    assert!(
        !t.txn()
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id=$1)",
                &[&serde_json::from_value::<Uuid>(erased["subject"].clone()).unwrap()]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    t.commit().await.unwrap();
    println!(
        "LC1_GROUP 127 rows/two pages; all full-key ties; native microseconds/infinities/BC/year280000; readonly unchanged tables; scoped settings; released cursor; live head; malformed/out-of-range normalization"
    );
}

#[tokio::test]
async fn hold_migration_backfill_insert_history_corruption_and_safe_down() {
    let db = common::setup_through(Some("2026-09-11-020000_pending_determination")).await;
    assert_eq!(
        db.revert_migrations(Some(1)).await.unwrap(),
        vec!["2026-09-11-020000_pending_determination"]
    );
    let t = db.begin_transaction().await.unwrap();
    let predecessor:String=t.txn().query_one("SELECT pg_get_functiondef('commercial_operation_before_hold_review(text,uuid,jsonb)'::regprocedure)",&[]).await.unwrap().get(0);
    let predecessor = predecessor.replacen(
        "FUNCTION public.commercial_operation_before_hold_review(",
        "FUNCTION public.commercial_operation(",
        1,
    );
    t.commit().await.unwrap();
    assert_eq!(db.revert_migrations(Some(1)).await.unwrap(), vec![FORWARD]);
    // The immediate predecessor staff read remains active during a safe empty downgrade.
    let t = db.begin_transaction().await.unwrap();
    let exists:bool=t.txn().query_one("SELECT position('admin_cash_capacity' in pg_get_functiondef('commercial_operation(text,uuid,jsonb)'::regprocedure))>0",&[]).await.unwrap().get(0);
    assert!(exists);
    assert_eq!(
        t.txn()
            .query_one(
                "SELECT pg_get_functiondef('commercial_operation(text,uuid,jsonb)'::regprocedure)",
                &[]
            )
            .await
            .unwrap()
            .get::<_, String>(0),
        predecessor
    );
    t.commit().await.unwrap();
    let c = Uuid::new_v4();
    let o = *academy_demo::user::BAR.user.id;
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO commercial_cases(id,subject) VALUES($1,$2)",
            &[&c, &o],
        )
        .await
        .unwrap();
    t.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES('pre-LC1','credit_note',$1,'2000-01-01',ARRAY['unchanged'])",&[&o]).await.unwrap();
    t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,review_due_at) VALUES($1,'pre-LC1','pre-existing original basis','-infinity')",&[&c]).await.unwrap();
    t.commit().await.unwrap();
    let before = original_only(fingerprint(&db).await);
    assert_eq!(
        common::apply_through(&db, Some("2026-09-11-020000_pending_determination")).await,
        vec![FORWARD, "2026-09-11-020000_pending_determination"]
    );
    let b = staff_body(&db, c, o).await;
    let capacity = op(&db, "admin_cash_capacity", &b).await.unwrap();
    assert_eq!(capacity["case_id"], json!(c));
    assert_eq!(capacity["subject"], json!(o));
    assert!(capacity["remaining_purchase_capacity"].is_null());
    let q = queue(&db, &b, Value::Null, 100).await;
    let h = &q["rows"][0];
    assert_eq!(h["review_version"], "0");
    assert!(h["last_review"].is_null());
    assert_eq!(h["review_due_at"], "-infinity");
    let mut after = original_only(fingerprint(&db).await);
    let mut before = before;
    for name in ["_migrations", "users", "sessions", "session_refresh_tokens"] {
        before.remove(name);
        after.remove(name);
    }
    assert_eq!(before, after);
    let t = db.begin_transaction().await.unwrap();
    t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,incarnation_id,review_version) VALUES($1,'pre-LC1','different supplied basis',$2,9) ON CONFLICT DO NOTHING",&[&c,&Uuid::new_v4()]).await.unwrap();
    t.commit().await.unwrap();
    assert_eq!(queue(&db, &b, Value::Null, 100).await["rows"][0], *h);
    let body = review(&b, h);
    let r = op(&db, "hold_review", &body).await.unwrap();
    assert_eq!(r["previous_review_due_at"], "-infinity");
    assert_eq!(
        db.revert_migrations(Some(1)).await.unwrap(),
        vec!["2026-09-11-020000_pending_determination"]
    );
    let original = fingerprint(&db).await;
    assert!(
        db.revert_migrations(Some(1))
            .await
            .unwrap_err()
            .to_string()
            .contains("Failed to revert")
    );
    assert_eq!(fingerprint(&db).await, original);
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .batch_execute("DELETE FROM commercial_document_holds")
        .await
        .unwrap();
    t.commit().await.unwrap();
    let released = fingerprint(&db).await;
    assert!(db.revert_migrations(Some(1)).await.is_err());
    assert_eq!(fingerprint(&db).await, released);
    let t = db.begin_transaction().await.unwrap();
    t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,review_due_at) VALUES($1,'pre-LC1','reinserted exact basis','2000-01-01')",&[&c]).await.unwrap();
    t.commit().await.unwrap();
    let h = queue(&db, &b, Value::Null, 100).await["rows"][0].clone();
    op(&db, "hold_review", &review(&b, &h)).await.unwrap();
    // Raw privileged corruption is a negative fixture only, not a supported writer.
    sql(&db,"ALTER TABLE commercial_document_holds DISABLE TRIGGER commercial_hold_update; UPDATE commercial_document_holds SET review_version=review_version+1; ALTER TABLE commercial_document_holds ENABLE TRIGGER commercial_hold_update").await;
    assert_eq!(
        common::apply_through(&db, Some("2026-09-11-020000_pending_determination")).await,
        vec!["2026-09-11-020000_pending_determination"]
    );
    let request = json!({"version":1,"limit":100,"cursor":null,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]});
    assert!(
        op(&db, "hold_queue", &request)
            .await
            .unwrap_err()
            .contains("history unavailable")
    );
    println!(
        "LC1_GROUP installation identity/version0; original preservation; conflict insert keeps incarnation; linked corrupt history unavailable; safe empty immediate-predecessor down/up; unsafe evidence-bearing down refuses unchanged"
    );
}
