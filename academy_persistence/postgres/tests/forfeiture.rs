//! Synthetic SQL only: unpaid wallet value of an erased account lapses on an
//! administrator's exact, replayable command and leaves the due staff queue.
mod common;
use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{PostgresDatabase, PostgresTransaction};
use serde_json::{Value, json};
use uuid::Uuid;

const DECISION: &str = "Owner decided that the gifted, never paid coin lapses";
const ASSESSMENT: &str = "Never purchased reward value of an erased account; no payment owed";

async fn staff(db: &PostgresDatabase) -> Value {
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
    tx.commit().await.unwrap();
    json!({"_staff_session":session,"_staff_refresh_hash":"5a".repeat(32)})
}

async fn op(db: &PostgresDatabase, op: &str, b: &Value) -> Result<Value, String> {
    let t = db.begin_transaction().await.unwrap();
    let r = op_tx(&t, op, b).await;
    if r.is_ok() {
        t.commit().await.unwrap();
    }
    r
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

async fn scalar(db: &PostgresDatabase, sql: &str, id: Uuid) -> String {
    let t = db.begin_transaction().await.unwrap();
    let v: String = t.txn().query_one(sql, &[&id]).await.unwrap().get(0);
    t.commit().await.unwrap();
    v
}

async fn state(db: &PostgresDatabase) -> String {
    let t = db.begin_transaction().await.unwrap();
    let v:String=t.txn().query_one("SELECT jsonb_build_object('cases',(SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) FROM commercial_cases x),'obligations',(SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) FROM commercial_obligations x),'journal',(SELECT count(*) FROM commercial_journal))::text",&[]).await.unwrap().get(0);
    t.commit().await.unwrap();
    v
}

struct Case {
    case_id: Uuid,
    subject: Uuid,
    obligation_id: Uuid,
}

/// An erased account's withheld reward coins as left by account erasure; the
/// case review already lies at infinity as after the support close-out.
async fn erased_case(
    db: &PostgresDatabase,
    source: &str,
    component: &str,
    units: i64,
    erased: bool,
) -> Case {
    let case = Case {
        case_id: Uuid::new_v4(),
        subject: Uuid::new_v4(),
        obligation_id: Uuid::new_v4(),
    };
    let t = db.begin_transaction().await.unwrap();
    t.txn().execute("INSERT INTO commercial_cases(id,subject,erased_at,review_due_at) VALUES($1,$2,CASE WHEN $3 THEN clock_timestamp() END,'infinity')",&[&case.case_id,&case.subject,&erased]).await.unwrap();
    t.txn().execute("INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,status,original) VALUES($1,$2,$3,$4,$5,$6,'pending_evidence','{\"withheld\":1}')",&[&case.obligation_id,&case.case_id,&source,&case.subject.to_string(),&component,&units]).await.unwrap();
    t.commit().await.unwrap();
    case
}

async fn determine(db: &PostgresDatabase, s: &Value, c: &Case, units: &str, cash: Value) {
    let mut b = json!({"command_id":Uuid::new_v4(),"case_id":c.case_id,"subject":c.subject,"obligation_id":c.obligation_id,"units":units,"cash_units":cash,"assessment":"Specific synthetic assessment of the withheld reward value","evidence":{"cash_basis":"Never purchased; reward only, no payment received"},"expected_obligation":{"status":"pending_evidence","units":units,"cash_units":null,"determination_json":null}});
    if cash.is_null() {
        b["evidence"] = json!({"original":"withheld reward value"});
    }
    merge(&mut b, s);
    op(db, "determine", &b).await.unwrap();
}

fn merge(b: &mut Value, s: &Value) {
    for (k, v) in s.as_object().unwrap() {
        b[k] = v.clone();
    }
}

fn forfeit_body(s: &Value, c: &Case, units: &str) -> Value {
    let mut b = json!({"command_id":Uuid::new_v4(),"case_id":c.case_id,"subject":c.subject,"obligation_id":c.obligation_id,"units":units,"assessment":ASSESSMENT,"owner_decision":DECISION});
    merge(&mut b, s);
    b
}

async fn due(db: &PostgresDatabase, s: &Value, c: &Case) -> String {
    let queue = op(db, "queue", s).await.unwrap();
    let entry = queue
        .as_array()
        .unwrap()
        .iter()
        .find(|q| q["id"] == json!(c.case_id))
        .unwrap()
        .clone();
    entry["due_at"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn unpaid_withheld_remainder_lapses_once_and_leaves_due_queue() {
    let db = common::setup().await;
    let s = staff(&db).await;
    let c = erased_case(&db, "backend", "wallet_withheld", 1, true).await;
    determine(&db, &s, &c, "1", json!("0")).await;
    let remaining = "SELECT commercial_remaining($1)::text";
    assert_eq!(scalar(&db, remaining, c.obligation_id).await, "1");
    assert_ne!(due(&db, &s, &c).await, "infinity");

    let b = forfeit_body(&s, &c, "1");
    let receipt = op(&db, "forfeit", &b).await.unwrap();
    assert_eq!(
        receipt,
        json!({"obligation_id":c.obligation_id,"status":"rejected","forfeited_units":"1","paid":false})
    );
    // Exact replay returns the stored receipt; a changed or new command does not lapse twice.
    assert_eq!(op(&db, "forfeit", &b).await.unwrap(), receipt);
    let mut changed = b.clone();
    changed["assessment"] = json!("A different synthetic assessment of the same value");
    assert!(
        op(&db, "forfeit", &changed)
            .await
            .unwrap_err()
            .contains("Conflicting")
    );
    let again = forfeit_body(&s, &c, "1");
    assert!(
        op(&db, "forfeit", &again)
            .await
            .unwrap_err()
            .contains("Only established")
    );

    assert_eq!(scalar(&db, remaining, c.obligation_id).await, "0");
    assert_eq!(due(&db, &s, &c).await, "infinity");
    let o: Value = serde_json::from_str(
        &scalar(
            &db,
            "SELECT to_jsonb(o)::text FROM commercial_obligations o WHERE id=$1",
            c.obligation_id,
        )
        .await,
    )
    .unwrap();
    assert_eq!(o["status"], "rejected");
    assert_eq!(o["units"], 1);
    assert_eq!(o["cash_units"], 0);
    assert_eq!(o["determination"]["kind"], "forfeited_unpaid_remainder");
    assert_eq!(o["determination"]["forfeiture"]["owner_decision"], DECISION);
    assert!(o["determination"]["forfeiture"]["_staff_session"].is_null());
    assert_eq!(o["determination"]["prior_determination"]["cash_units"], "0");
    let journal = scalar(
        &db,
        "SELECT string_agg(kind,',' ORDER BY id) FROM commercial_journal WHERE obligation_id=$1",
        c.obligation_id,
    )
    .await;
    assert_eq!(journal, "determine,forfeit");
}

#[tokio::test]
async fn forfeiture_refuses_everything_but_exact_unpaid_unreserved_wallet_value() {
    let db = common::setup().await;
    let s = staff(&db).await;
    let c = erased_case(&db, "backend", "wallet_withheld", 2, true).await;
    determine(&db, &s, &c, "2", json!("0")).await;
    let before = state(&db).await;
    let good = forfeit_body(&s, &c, "2");
    let refused = |field: &str, value: Value| {
        let mut b = good.clone();
        b["command_id"] = json!(Uuid::new_v4());
        b[field] = value;
        b
    };
    for b in [
        refused("units", json!("1")),
        refused("units", json!("3")),
        refused("units", json!("02")),
        refused("units", json!("0")),
        refused("units", json!(2)),
        refused("subject", json!(Uuid::new_v4())),
        refused("case_id", json!(Uuid::new_v4())),
        refused("obligation_id", json!(Uuid::new_v4())),
        refused("owner_decision", json!("too short")),
        refused("owner_decision", json!(42)),
        refused("assessment", json!("too short")),
        refused("extra", json!(true)),
    ] {
        assert!(op(&db, "forfeit", &b).await.is_err(), "{b}");
    }
    for key in ["owner_decision", "subject", "units"] {
        let mut b = good.clone();
        b["command_id"] = json!(Uuid::new_v4());
        b.as_object_mut().unwrap().remove(key);
        assert!(op(&db, "forfeit", &b).await.is_err(), "{key}");
    }
    let mut lost = good.clone();
    lost["_staff_refresh_hash"] = json!("00".repeat(32));
    assert!(
        op(&db, "forfeit", &lost)
            .await
            .unwrap_err()
            .contains("MFA authority")
    );
    assert_eq!(state(&db).await, before);

    // Tender unknown, purchased value, live accounts, other claims and reservations keep their paths.
    let unknown = erased_case(&db, "backend", "wallet_withheld", 1, true).await;
    determine(&db, &s, &unknown, "1", Value::Null).await;
    let paid = erased_case(&db, "backend", "wallet_available", 1, true).await;
    determine(&db, &s, &paid, "1", json!("1")).await;
    let live = erased_case(&db, "backend", "wallet_withheld", 1, false).await;
    determine(&db, &s, &live, "1", json!("0")).await;
    let event = erased_case(&db, "events", "student_refund", 1, true).await;
    determine(&db, &s, &event, "1", json!("0")).await;
    let pending = erased_case(&db, "backend", "wallet_withheld", 1, true).await;
    let reserved = erased_case(&db, "backend", "wallet_available", 1, true).await;
    determine(&db, &s, &reserved, "1", json!("0")).await;
    let t = db.begin_transaction().await.unwrap();
    t.txn().execute("INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request) VALUES($1,$2,1,'wallet','uncertain','{\"synthetic_transfer\":true}')",&[&Uuid::new_v4(),&reserved.obligation_id]).await.unwrap();
    t.commit().await.unwrap();
    let before = state(&db).await;
    for (case, error) in [
        (&unknown, "Only established"),
        (&paid, "Only established"),
        (&live, "erased account"),
        (&event, "retained wallet"),
        (&pending, "Only established"),
        (&reserved, "Reserved or applied"),
    ] {
        let err = op(&db, "forfeit", &forfeit_body(&s, case, "1"))
            .await
            .unwrap_err();
        assert!(err.contains(error), "{err}");
    }
    assert_eq!(state(&db).await, before);

    // A failed reservation left nothing applied, so that value can still lapse.
    let failed = erased_case(&db, "backend", "wallet_available", 1, true).await;
    determine(&db, &s, &failed, "1", json!("0")).await;
    let t = db.begin_transaction().await.unwrap();
    t.txn().execute("INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request) VALUES($1,$2,1,'wallet','failed','{\"synthetic_failed\":true}')",&[&Uuid::new_v4(),&failed.obligation_id]).await.unwrap();
    t.commit().await.unwrap();
    assert_eq!(
        op(&db, "forfeit", &forfeit_body(&s, &failed, "1"))
            .await
            .unwrap()["status"],
        "rejected"
    );
}

#[tokio::test]
async fn revert_keeps_lapsed_value_and_removes_only_the_operation() {
    const NAME: &str = "2026-11-07-120000_forfeit_unpaid_wallet_remainder";
    let db = common::setup().await;
    let s = staff(&db).await;
    let c = erased_case(&db, "backend", "wallet_withheld", 1, true).await;
    determine(&db, &s, &c, "1", json!("0")).await;
    let b = forfeit_body(&s, &c, "1");
    op(&db, "forfeit", &b).await.unwrap();
    let lapsed = state(&db).await;
    assert_eq!(db.revert_migrations(Some(1)).await.unwrap(), vec![NAME]);
    assert_eq!(state(&db).await, lapsed);
    let remaining = "SELECT commercial_remaining($1)::text";
    assert_eq!(scalar(&db, remaining, c.obligation_id).await, "0");
    assert!(
        op(&db, "forfeit", &forfeit_body(&s, &c, "1"))
            .await
            .unwrap_err()
            .contains("Unsupported commercial operation")
    );
    assert_eq!(db.run_migrations(None).await.unwrap(), vec![NAME]);
    assert_eq!(op(&db, "forfeit", &b).await.unwrap()["status"], "rejected");
}
