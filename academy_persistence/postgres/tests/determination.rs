//! Ordinary synthetic SQL only. Every destructive reset follows parsed configuration
//! and live server identity checks against this unit's new marked fixture.
mod common;
use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{PostgresDatabase, PostgresTransaction};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use uuid::Uuid;
const FORWARD: &str = "2026-09-11-020000_pending_determination";
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
    Ok(row
        .get::<_, Option<String>>(0)
        .map(|s| serde_json::from_str(&s).unwrap())
        .unwrap_or(Value::Null))
}
async fn op(db: &PostgresDatabase, op: &str, b: &Value) -> Result<Value, String> {
    let t = db.begin_transaction().await.unwrap();
    let r = op_tx(&t, op, b).await;
    if r.is_ok() {
        t.commit().await.unwrap();
    }
    r
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

async fn seed(db: &PostgresDatabase) -> Value {
    let o = *academy_demo::user::BAR.user.id;
    let c = Uuid::new_v4();
    let id = Uuid::new_v4();
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO commercial_cases(id,subject,closed_at) VALUES($1,$2,clock_timestamp())",
            &[&c, &o],
        )
        .await
        .unwrap();
    t.txn().execute("INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,status,original) VALUES($1,$2,'events','preserved original','unknown original component',10,'pending_evidence','{\"big\":9007199254740993}')",&[&id,&c]).await.unwrap();
    t.commit().await.unwrap();
    let b = staff_body(db, c, o).await;
    json!({"command_id":Uuid::new_v4(),"case_id":c,"subject":o,"obligation_id":id,"units":"10","cash_units":null,"assessment":"Specific synthetic assessment for original evidence","evidence":{"original":"preserved evidence"},"expected_obligation":{"status":"pending_evidence","units":"10","cash_units":null,"determination_json":null},"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]})
}
fn status_body(b: &Value, command: Value) -> Value {
    json!({"case_id":b["case_id"],"subject":b["subject"],"obligation_id":b["obligation_id"],"command_id":command,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]})
}
fn originals(mut x: BTreeMap<String, String>) -> BTreeMap<String, String> {
    x.remove("commercial_obligations");
    x.remove("commercial_journal");
    x
}
async fn snapshot(db: &PostgresDatabase, label: &str) {
    let root = PathBuf::from(std::env::var("ACADEMY_UNIT_TEST_FIXTURE").unwrap());
    let t = db.begin_transaction().await.unwrap();
    let data=query(&t,"SELECT jsonb_build_object('cases',(SELECT jsonb_agg(to_jsonb(t)) FROM commercial_cases t),'obligations',(SELECT jsonb_agg(to_jsonb(t)) FROM commercial_obligations t),'journal',(SELECT jsonb_agg(to_jsonb(t)) FROM commercial_journal t))::text",&[]).await;
    t.commit().await.unwrap();
    let path = root
        .join("evidence")
        .join(format!("{label}-{}.json", Uuid::new_v4()));
    std::fs::write(&path, serde_json::to_vec_pretty(&data).unwrap()).unwrap();
    println!("PRESERVED_STATE {}", path.display());
}
#[tokio::test]
async fn determination_guarded_tuple_originals_and_exact_recovery() {
    let db = setup().await;
    let b = seed(&db).await;
    let original = fingerprint(&db).await;
    // The old implementation accepts this new command despite the stale tuple.
    let mut stale = b.clone();
    stale["expected_obligation"]["units"] = json!("9");
    assert!(
        op(&db, "determine", &stale)
            .await
            .unwrap_err()
            .contains("differs")
    );
    for (field, value) in [
        ("units", json!(10)),
        ("units", json!("9")),
        ("units", json!("01")),
        ("units", json!("-1")),
        ("units", json!("9223372036854775808")),
        ("cash_units", json!("11")),
        ("cash_units", json!("")),
        ("evidence", json!({})),
        ("subject", json!(Uuid::new_v4())),
        ("case_id", json!(Uuid::new_v4())),
        ("obligation_id", json!(Uuid::new_v4())),
    ] {
        let mut bad = b.clone();
        bad[field] = value;
        assert!(op(&db, "determine", &bad).await.is_err(), "{field}");
    }
    for key in ["subject", "cash_units", "expected_obligation"] {
        let mut bad = b.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(op(&db, "determine", &bad).await.is_err());
    }
    let mut bad = b.clone();
    bad["extra"] = json!(true);
    assert!(op(&db, "determine", &bad).await.is_err());
    let mut bad = b.clone();
    bad["cash_units"] = json!("0");
    assert!(op(&db, "determine", &bad).await.is_err());
    assert_eq!(fingerprint(&db).await, original);
    let receipt = op(&db, "determine", &b).await.unwrap();
    assert_eq!(
        receipt,
        json!({"obligation_id":b["obligation_id"],"status":"established","paid":false})
    );
    assert_eq!(op(&db, "determine", &b).await.unwrap(), receipt);
    let mut other = b.clone();
    other["command_id"] = json!(Uuid::new_v4());
    assert!(
        op(&db, "determine", &other)
            .await
            .unwrap_err()
            .contains("differs")
    );
    let mut other = b.clone();
    other["assessment"] = json!("Another specific synthetic human assessment");
    assert!(
        op(&db, "determine", &other)
            .await
            .unwrap_err()
            .contains("Conflicting")
    );
    assert_eq!(originals(fingerprint(&db).await), originals(original));
    let observed = op(
        &db,
        "admin_determination_status",
        &status_body(&b, b["command_id"].clone()),
    )
    .await
    .unwrap();
    assert_eq!(observed["obligation"]["units"], "10");
    assert!(
        observed["obligation"]["original_json"]
            .as_str()
            .unwrap()
            .contains("9007199254740993")
    );
    let stored: Value =
        serde_json::from_str(observed["journal"]["request_json"].as_str().unwrap()).unwrap();
    assert!(
        !stored
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.starts_with('_'))
    );
    sql(
        &db,
        "UPDATE commercial_obligations SET status='rejected',cash_units=0,determination='null'",
    )
    .await;
    assert_eq!(op(&db, "determine", &b).await.unwrap(), receipt);
    for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
        for command in [b["command_id"].clone(), json!(Uuid::new_v4())] {
            let t = db.begin_transaction().await.unwrap();
            t.txn()
                .batch_execute(&format!("SET TRANSACTION ISOLATION LEVEL {isolation}"))
                .await
                .unwrap();
            let mut q = b.clone();
            q["command_id"] = command;
            assert!(
                op_tx(&t, "determine", &q)
                    .await
                    .unwrap_err()
                    .contains("READ COMMITTED")
            );
        }
    }
    snapshot(&db, "guarded").await;
    println!(
        "DETERMINATION_GROUP exact new tuple/shape/ranges, all-table no-effect failures, known amount and originals, exact receipt after changed current state, RC new/replay"
    );
}
#[tokio::test]
async fn determination_legacy_receipt_and_erased_owner_without_adoption() {
    let db = setup().await;
    let mut b = seed(&db).await;
    let c: Uuid = serde_json::from_value(b["case_id"].clone()).unwrap();
    let o: Uuid = serde_json::from_value(b["subject"].clone()).unwrap();
    // Execute the actual unchanged complete LC1 predecessor, not a fabricated receipt.
    let mut legacy = b.clone();
    legacy.as_object_mut().unwrap().remove("subject");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("expected_obligation");
    legacy.as_object_mut().unwrap().remove("cash_units");
    legacy["units"] = json!(10);
    legacy["extra"] = json!("original admitted extra");
    let t = db.begin_transaction().await.unwrap();
    let actor = *academy_demo::user::FOO.user.id;
    let receipt=query(&t,"SELECT commercial_operation_before_pending_determination('determine',$1,$2::text::jsonb)::text",&[&actor,&legacy.to_string()]).await;
    t.commit().await.unwrap();
    sql(
        &db,
        "UPDATE commercial_obligations SET status='rejected',determination='[1,2]'",
    )
    .await;
    assert_eq!(op(&db, "determine", &legacy).await.unwrap(), receipt);
    for iso in ["REPEATABLE READ", "SERIALIZABLE"] {
        let t = db.begin_transaction().await.unwrap();
        t.txn()
            .batch_execute(&format!("SET TRANSACTION ISOLATION LEVEL {iso}"))
            .await
            .unwrap();
        assert!(
            op_tx(&t, "determine", &legacy)
                .await
                .unwrap_err()
                .contains("READ COMMITTED")
        );
    }
    let old_status = op(
        &db,
        "admin_determination_status",
        &status_body(&b, legacy["command_id"].clone()),
    )
    .await
    .unwrap();
    assert_eq!(old_status["obligation"]["status"], "rejected");
    assert_eq!(
        serde_json::from_str::<Value>(old_status["journal"]["result_json"].as_str().unwrap())
            .unwrap(),
        receipt
    );
    // An erased original owner has an existing case, no live user; no helper may adopt a learning case.
    let erased = Uuid::new_v4();
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute(
            "UPDATE commercial_cases SET subject=$1 WHERE id=$2",
            &[&erased, &c],
        )
        .await
        .unwrap();
    t.txn()
        .execute(
            "UPDATE commercial_obligations SET status='pending_evidence',determination=NULL",
            &[],
        )
        .await
        .unwrap();
    t.commit().await.unwrap();
    b["subject"] = json!(erased);
    b["command_id"] = json!(Uuid::new_v4());
    assert_eq!(op(&db, "determine", &b).await.unwrap()["paid"], false);
    assert!(
        op(
            &db,
            "admin_determination_status",
            &status_body(&b, Value::Null)
        )
        .await
        .unwrap()["journal"]
            .is_null()
    );
    let mut wrong = status_body(&b, Value::Null);
    wrong["subject"] = json!(o);
    assert!(
        op(&db, "admin_determination_status", &wrong)
            .await
            .unwrap()
            .is_null()
    );
    let t = db.begin_transaction().await.unwrap();
    let count: i64 = t
        .txn()
        .query_one("SELECT count(*) FROM commercial_cases", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 1);
    t.commit().await.unwrap();
    snapshot(&db, "legacy-erased").await;
    println!(
        "DETERMINATION_GROUP actual predecessor-created legacy replay, omitted/null distinctions and old extras, unsupported isolation, current erased owner exact case without reconstruction"
    );
}
#[tokio::test]
async fn determination_status_raw_legacy_integrity_and_readonly_projection() {
    let db = setup().await;
    let b = seed(&db).await;
    let before = fingerprint(&db).await;
    let first = op(
        &db,
        "admin_determination_status",
        &status_body(&b, Value::Null),
    )
    .await
    .unwrap();
    assert_eq!(first.as_object().unwrap().len(), 6);
    assert_eq!(first["obligation"].as_object().unwrap().len(), 9);
    assert!(first["journal"].is_null());
    assert_eq!(fingerprint(&db).await, before);
    let c: Uuid = serde_json::from_value(b["case_id"].clone()).unwrap();
    let id: Uuid = serde_json::from_value(b["obligation_id"].clone()).unwrap();
    let actor = *academy_demo::user::FOO.user.id;
    // One local statement observes neither part of an uncommitted decision.
    let writer = db.begin_transaction().await.unwrap();
    let receipt = op_tx(&writer, "determine", &b).await.unwrap();
    let query_body = status_body(&b, b["command_id"].clone());
    let pending = tokio::time::timeout(
        Duration::from_secs(2),
        op(&db, "admin_determination_status", &query_body),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(pending["obligation"]["status"], "pending_evidence");
    assert!(pending["journal"].is_null());
    writer.commit().await.unwrap();
    let committed = op(&db, "admin_determination_status", &query_body)
        .await
        .unwrap();
    assert_eq!(committed["obligation"]["status"], "established");
    assert_eq!(
        serde_json::from_str::<Value>(committed["journal"]["result_json"].as_str().unwrap())
            .unwrap(),
        receipt
    );
    for (i, mode) in [
        "valid",
        "noactor",
        "badcase",
        "badamount",
        "badresult",
        "otherkind",
        "othertarget",
    ]
    .iter()
    .enumerate()
    {
        let cmd = Uuid::new_v4();
        let mut request = b.clone();
        request
            .as_object_mut()
            .unwrap()
            .remove("expected_obligation");
        request.as_object_mut().unwrap().remove("_staff_session");
        request
            .as_object_mut()
            .unwrap()
            .remove("_staff_refresh_hash");
        request["command_id"] = json!(cmd);
        request["subject"] = json!("ignored old subject");
        request["units"] = json!("9223372036854775807");
        request["cash_units"] = json!("");
        request["legacy_extra"] = json!([true, null]);
        if *mode == "badcase" {
            request["case_id"] = json!(false);
        }
        if *mode == "badamount" {
            request["units"] = json!("9223372036854775808");
        }
        let result = if *mode == "badresult" {
            json!({"paid":true})
        } else {
            json!({"obligation_id":id,"status":"established","paid":false})
        };
        let t = db.begin_transaction().await.unwrap();
        let journal_actor = if *mode == "noactor" {
            None
        } else {
            Some(actor)
        };
        let kind = if *mode == "otherkind" {
            "review"
        } else {
            "determine"
        };
        let target = if *mode == "othertarget" {
            None
        } else {
            Some(id)
        };
        t.txn().execute("INSERT INTO commercial_journal(id,case_id,obligation_id,actor,command_id,kind,request,result,recorded_at) OVERRIDING SYSTEM VALUE VALUES($1,$2,$3,$4,$5,$6,$7::text::jsonb,$8::text::jsonb,'infinity')",&[&(-9007199254740993i64-i as i64),&c,&target,&journal_actor,&cmd,&kind,&request.to_string(),&result.to_string()]).await.unwrap();
        t.commit().await.unwrap();
        let t = db.begin_transaction().await.unwrap();
        let raw=t.txn().query_one("SELECT commercial_operation('admin_determination_status',$1,$2::text::jsonb)::text",&[&actor,&status_body(&b,json!(cmd)).to_string()]).await;
        if ["noactor", "badcase", "badamount", "badresult"].contains(mode) {
            assert_eq!(
                raw.unwrap_err().as_db_error().unwrap().code().code(),
                "22000"
            );
            drop(t);
            use academy_persistence_contracts::moderation::ModerationRepository;
            let mut t = db.begin_transaction().await.unwrap();
            let error = academy_persistence_postgres::moderation::PostgresModerationRepository
                .commercial_operation(
                    &mut t,
                    "admin_determination_status",
                    Some(actor.into()),
                    &status_body(&b, json!(cmd)),
                )
                .await
                .unwrap_err();
            assert!(
                error
                    .downcast_ref::<academy_persistence_contracts::moderation::ModerationConflict>()
                    .is_none()
            );
            assert_eq!(
                error
                    .downcast_ref::<bb8_postgres::tokio_postgres::Error>()
                    .unwrap()
                    .as_db_error()
                    .unwrap()
                    .code()
                    .code(),
                "22000"
            );
        } else {
            let v: Value = serde_json::from_str(&raw.unwrap().get::<_, String>(0)).unwrap();
            if *mode == "valid" {
                assert_eq!(v["journal"]["id"], "-9007199254740993");
                assert_eq!(v["journal"]["recorded_at"], "infinity");
                assert!(!v["journal"].as_object().unwrap().contains_key("subject"));
                assert!(
                    v["journal"]["request_json"]
                        .as_str()
                        .unwrap()
                        .contains("ignored old subject")
                );
            } else {
                assert!(v["journal"].is_null());
            }
            t.commit().await.unwrap();
        }
    }
    sql(&db,"UPDATE commercial_obligations SET units=NULL,cash_units=7,original='9007199254740993',determination='null',source='',source_key='  ',component='unknown'").await;
    let before = fingerprint(&db).await;
    let t = db.begin_transaction().await.unwrap();
    t.txn().batch_execute("SET TRANSACTION READ ONLY;SET LOCAL TIME ZONE 'Pacific/Auckland';SET LOCAL DateStyle='German,DMY'").await.unwrap();
    let v = op_tx(
        &t,
        "admin_determination_status",
        &status_body(&b, Value::Null),
    )
    .await
    .unwrap();
    assert!(v["obligation"]["units"].is_null());
    assert_eq!(v["obligation"]["cash_units"], "7");
    assert_eq!(v["obligation"]["original_json"], "9007199254740993");
    assert_eq!(v["obligation"]["determination_json"], "null");
    assert_eq!(v["obligation"]["source"], "");
    assert!(v["observed_at"].as_str().unwrap().ends_with("+00"));
    t.commit().await.unwrap();
    assert_eq!(fingerprint(&db).await, before);
    snapshot(&db, "status").await;
    println!(
        "DETERMINATION_GROUP bounded raw JSON/decimal/null/native UTC, signed journal IDs, matching malformed22000 vs observed nonmatching absence, READ ONLY/all-table unchanged"
    );
}

async fn start_decision(
    db: &PostgresDatabase,
    b: Value,
) -> (i32, tokio::task::JoinHandle<Result<Value, String>>) {
    let db = db.clone();
    let (send, recv) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let t = db.begin_transaction().await.unwrap();
        send.send(
            t.txn()
                .query_one("SELECT pg_backend_pid()", &[])
                .await
                .unwrap()
                .get::<_, i32>(0),
        )
        .unwrap();
        let r = op_tx(&t, "determine", &b).await;
        if r.is_ok() {
            t.commit().await.unwrap();
        }
        r
    });
    (recv.await.unwrap(), task)
}
#[tokio::test]
async fn determination_actual_supported_waits_staff_loss_and_competing_decisions() {
    let db = setup().await;
    let original = seed(&db).await;
    let c: Uuid = serde_json::from_value(original["case_id"].clone()).unwrap();
    let o: Uuid = serde_json::from_value(original["subject"].clone()).unwrap();
    let id: Uuid = serde_json::from_value(original["obligation_id"].clone()).unwrap();
    for (i, kind) in ["command", "user", "case", "obligation"].iter().enumerate() {
        let proof = staff_body(&db, c, o).await;
        let mut b = original.clone();
        b["command_id"] = json!(Uuid::new_v4());
        b["_staff_session"] = proof["_staff_session"].clone();
        let holder = db.begin_transaction().await.unwrap();
        let holder_pid: i32 = holder
            .txn()
            .query_one("SELECT pg_backend_pid()", &[])
            .await
            .unwrap()
            .get(0);
        match *kind {
            "command" => {
                holder.txn().query_one("SELECT pg_advisory_xact_lock(hashtextextended('commercial-command:'||$1::text,0))",&[&b["command_id"].as_str().unwrap()]).await.unwrap();
            }
            "user" => {
                holder
                    .txn()
                    .query_one("SELECT id FROM users WHERE id=$1 FOR UPDATE", &[&o])
                    .await
                    .unwrap();
            }
            "case" => {
                holder
                    .txn()
                    .query_one(
                        "SELECT id FROM commercial_cases WHERE id=$1 FOR UPDATE",
                        &[&c],
                    )
                    .await
                    .unwrap();
            }
            _ => {
                holder
                    .txn()
                    .query_one(
                        "SELECT id FROM commercial_obligations WHERE id=$1 FOR UPDATE",
                        &[&id],
                    )
                    .await
                    .unwrap();
            }
        }
        let (pid, task) = start_decision(&db, b.clone()).await;
        wait_for_block(&db, pid, holder_pid).await;
        revoke_staff(&db, &b, ["mfa", "refresh", "admin"][i % 3]).await;
        holder.commit().await.unwrap();
        let e = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(e.contains("authority"), "{kind}: {e}");
        println!("DETERMINATION_WAIT_STAFF_REFUSAL {kind}");
    }
    let proof = staff_body(&db, c, o).await;
    let mut first = original.clone();
    first["_staff_session"] = proof["_staff_session"].clone();
    let mut second = first.clone();
    second["command_id"] = json!(Uuid::new_v4());
    let holder = db.begin_transaction().await.unwrap();
    let receipt = op_tx(&holder, "determine", &first).await.unwrap();
    let holder_pid: i32 = holder
        .txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let (pid, task) = start_decision(&db, second).await;
    wait_for_block(&db, pid, holder_pid).await;
    holder.commit().await.unwrap();
    assert!(task.await.unwrap().unwrap_err().contains("differs"));
    assert_eq!(op(&db, "determine", &first).await.unwrap(), receipt);
    // A genuine replay still observes authority loss while waiting for its command.
    let holder = db.begin_transaction().await.unwrap();
    holder
        .txn()
        .query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended('commercial-command:'||$1::text,0))",
            &[&first["command_id"].as_str().unwrap()],
        )
        .await
        .unwrap();
    let hp: i32 = holder
        .txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    let (pid, task) = start_decision(&db, first.clone()).await;
    wait_for_block(&db, pid, hp).await;
    revoke_staff(&db, &first, "mfa").await;
    holder.commit().await.unwrap();
    assert!(task.await.unwrap().unwrap_err().contains("authority"));
    // Same actor with a new current session retains its exact receipt.
    let proof = staff_body(&db, c, o).await;
    first["_staff_session"] = proof["_staff_session"].clone();
    assert_eq!(op(&db, "determine", &first).await.unwrap(), receipt);
    let t = db.begin_transaction().await.unwrap();
    let n: i64 = t
        .txn()
        .query_one(
            "SELECT count(*) FROM commercial_journal WHERE kind='determine'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 1);
    t.commit().await.unwrap();
    let t = db.begin_transaction().await.unwrap();
    let second_session = Uuid::new_v4();
    let second_actor = Uuid::new_v4();
    t.txn().execute("INSERT INTO users(id,name,email_verified,created_at,enabled,admin) VALUES($1,'determination-second-staff',false,clock_timestamp(),true,true)",&[&second_actor]).await.unwrap();
    t.txn().execute("INSERT INTO user_profiles(user_id,display_name,bio,tags) VALUES($1,'Synthetic second staff','','{}')",&[&second_actor]).await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO user_invoice_info(user_id) VALUES($1)",
            &[&second_actor],
        )
        .await
        .unwrap();
    t.txn().execute("INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified) VALUES($1,$2,clock_timestamp(),clock_timestamp(),true)",&[&second_session,&second_actor]).await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO session_refresh_tokens(session_id,refresh_token_hash) VALUES($1,$2)",
            &[&second_session, &vec![0x5au8; 32]],
        )
        .await
        .unwrap();
    t.commit().await.unwrap();
    let mut other_actor = first.clone();
    other_actor["_staff_session"] = json!(second_session);
    let t = db.begin_transaction().await.unwrap();
    let e = t
        .txn()
        .query_one(
            "SELECT commercial_operation('determine',$1,$2::text::jsonb)::text",
            &[&second_actor, &other_actor.to_string()],
        )
        .await
        .unwrap_err();
    assert!(format!("{e:?}").contains("Conflicting"));
    drop(t);
    snapshot(&db, "waits").await;
    println!(
        "DETERMINATION_GROUP actual command/user/case/obligation waits and fresh staff, competing current tuple, replay wait loss and same-actor new-session exact receipt"
    );
}
#[tokio::test]
async fn determination_additive_history_legacy_casts_and_delegation() {
    let db = common::setup_through(Some("2026-09-11-020000_pending_determination")).await;
    let b = seed(&db).await;
    let c: Uuid = "10000000-0000-4000-8000-000000000001".parse().unwrap();
    let id: Uuid = "10000000-0000-4000-8000-000000000002".parse().unwrap();
    let owner = Uuid::new_v4();
    let actor = *academy_demo::user::FOO.user.id;
    let t = db.begin_transaction().await.unwrap();
    t.txn()
        .execute(
            "INSERT INTO commercial_cases(id,subject) VALUES($1,$2)",
            &[&c, &owner],
        )
        .await
        .unwrap();
    t.txn().execute("INSERT INTO commercial_obligations(id,case_id,source,source_key,component,status,original) VALUES($1,$2,'','','','pending_evidence','null')",&[&id,&c]).await.unwrap();
    t.commit().await.unwrap();
    let mut last_command = Uuid::nil();
    for cash in [
        "",
        " ,\"cash_units\":null",
        " ,\"cash_units\":\"\"",
        " ,\"cash_units\":0",
    ] {
        let cmd = Uuid::new_v4();
        last_command = cmd;
        let raw = format!(
            r#"{{"command_id":"{cmd}","case_id":10000000000040008000000000000001,"obligation_id":10000000000040008000000000000002,"units":9007199254740993{cash},"assessment":"Original exact sufficient assessment","evidence":{{"cash_basis":123456789012345678901234567890}},"subject":{{"ignored":true}},"_staff_session":{},"_staff_refresh_hash":{}}}"#,
            b["_staff_session"], b["_staff_refresh_hash"]
        );
        let t = db.begin_transaction().await.unwrap();
        let receipt=query(&t,"SELECT commercial_operation_before_pending_determination('determine',$1,$2::text::jsonb)::text",&[&actor,&raw]).await;
        t.commit().await.unwrap();
        let t = db.begin_transaction().await.unwrap();
        assert_eq!(
            query(
                &t,
                "SELECT commercial_operation('determine',$1,$2::text::jsonb)::text",
                &[&actor, &raw]
            )
            .await,
            receipt
        );
        t.commit().await.unwrap();
        let q = json!({"case_id":c,"subject":owner,"obligation_id":id,"command_id":cmd,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]});
        let v = op(&db, "admin_determination_status", &q).await.unwrap();
        let stored = v["journal"]["request_json"].as_str().unwrap();
        assert!(stored.contains("10000000000040008000000000000001"));
        assert!(stored.contains("9007199254740993"));
        assert!(stored.contains("123456789012345678901234567890"));
        assert_eq!(v["obligation"]["units"], "9007199254740993");
    }
    let t = db.begin_transaction().await.unwrap();
    let before_wrapper:String=t.txn().query_one("SELECT pg_get_functiondef('commercial_operation_before_pending_determination(text,uuid,jsonb)'::regprocedure)",&[]).await.unwrap().get(0);
    t.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES('determination-hold','credit_note',NULL,'2000-01-01',ARRAY['original'])",&[]).await.unwrap();
    t.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis,review_due_at) VALUES($1,'determination-hold','preserved original hold basis','-infinity')",&[&c]).await.unwrap();
    t.commit().await.unwrap();
    let mut before = fingerprint(&db).await;
    before.remove("_migrations");
    assert_eq!(db.revert_migrations(Some(1)).await.unwrap(), vec![FORWARD]);
    let t = db.begin_transaction().await.unwrap();
    let restored: String = t
        .txn()
        .query_one(
            "SELECT pg_get_functiondef('commercial_operation(text,uuid,jsonb)'::regprocedure)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        restored,
        before_wrapper.replace(
            "commercial_operation_before_pending_determination(",
            "commercial_operation("
        )
    );
    t.commit().await.unwrap();
    let queue = json!({"version":1,"limit":100,"cursor":null,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]});
    assert_eq!(
        op(&db, "hold_queue", &queue).await.unwrap()["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        common::apply_through(&db, Some("2026-09-11-020000_pending_determination")).await,
        vec![FORWARD]
    );
    let mut after = fingerprint(&db).await;
    after.remove("_migrations");
    assert_eq!(after, before);
    assert_eq!(
        op(&db, "hold_queue", &queue).await.unwrap()["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let capacity=op(&db,"admin_cash_capacity",&json!({"case_id":c,"subject":owner,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]})).await.unwrap();
    assert_eq!(capacity["subject"], json!(owner));
    let q = json!({"case_id":c,"subject":owner,"obligation_id":id,"command_id":last_command,"_staff_session":b["_staff_session"],"_staff_refresh_hash":b["_staff_refresh_hash"]});
    assert!(op(&db, "admin_determination_status", &q).await.unwrap()["journal"].is_object());
    snapshot(&db, "migration-legacy").await;
    println!(
        "DETERMINATION_GROUP actual legacy numeric UUID/amount/cash casts with ignored subject and extra evidence; additive down/up preserves original history/holds and immediate LC1 plus AD1 delegation"
    );
}
