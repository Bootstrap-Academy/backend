//! Staff observations against a newly owned, verified synthetic SQL target only.
mod common;
use academy_persistence_contracts::{Database, Transaction, moderation::ModerationRepository};
use academy_persistence_postgres::{
    PostgresDatabase, moderation::PostgresModerationRepository as Repo,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;
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
async fn read(db: &PostgresDatabase, body: &Value) -> Value {
    let mut tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    let result = Repo
        .commercial_operation(
            &mut tx,
            "admin_cash_capacity",
            Some(academy_demo::user::FOO.user.id),
            body,
        )
        .await
        .unwrap();
    assert_eq!(
        tx.txn()
            .query_one("SHOW transaction_read_only", &[])
            .await
            .unwrap()
            .get::<_, &str>(0),
        "on"
    );
    tx.commit().await.unwrap();
    result
}
async fn case(db: &PostgresDatabase, subject: Uuid) -> Uuid {
    let c = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .execute(
            "INSERT INTO commercial_cases(id,subject) VALUES($1,$2)",
            &[&c, &subject],
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    c
}
async fn basis(db: &PostgresDatabase, c: Uuid, prior: i64) {
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO commercial_cash_basis(case_id,prior_refund_units,assessment) VALUES($1,$2,'{\"synthetic\":true}') ON CONFLICT(case_id) DO UPDATE SET prior_refund_units=excluded.prior_refund_units,assessment=excluded.assessment,reviewed_at=clock_timestamp()",&[&c,&prior]).await.unwrap();
    tx.commit().await.unwrap();
}
async fn capture(db: &PostgresDatabase, o: Uuid, n: i64, units: i64) {
    let tx = db.begin_transaction().await.unwrap();
    let id = format!("staff-read-{n}");
    tx.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,captured_at,coins,invoice_number) VALUES($1,$2,'2026-09-01','2026-09-02',$3,$4)",&[&id,&o,&units,&n]).await.unwrap();
    tx.commit().await.unwrap();
}
async fn obligation(
    db: &PostgresDatabase,
    c: Uuid,
    units: Option<i64>,
    cash: Option<i64>,
    status: &str,
) -> Uuid {
    let id = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    let key = id.to_string();
    tx.txn().execute("INSERT INTO commercial_obligations(id,case_id,source,source_key,component,units,cash_units,status,original) VALUES($1,$2,'events',$3,'service',$4,$5,$6,'{\"private_original\":true}')",&[&id,&c,&key,&units,&cash,&status]).await.unwrap();
    tx.commit().await.unwrap();
    id
}
async fn reserve(
    db: &PostgresDatabase,
    o: Uuid,
    units: i64,
    mode: &str,
    state: &str,
    cap: Option<i64>,
) -> Uuid {
    let id = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    let request=json!({"purchase_capacity_units":cap,"capacity_basis":"Synthetic assessed independent service remedy with exact capacity; not payment evidence"}).to_string();
    tx.txn().execute("INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request) VALUES($1,$2,$3,$4,$5,$6::text::jsonb)",&[&id,&o,&units,&mode,&state,&request]).await.unwrap();
    tx.commit().await.unwrap();
    id
}
fn find(value: &Value, id: Uuid) -> &Value {
    value["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id.to_string())
        .unwrap()
}

#[tokio::test]
async fn staff_reads_exact_owner_nulls_and_fresh_observations_are_read_only() {
    let db = setup().await;
    let a = *academy_demo::user::FOO.user.id;
    let c = case(&db, a).await;
    let body = staff_body(&db, c, a).await;
    let unknown = obligation(&db, c, None, None, "pending_evidence").await;
    let rejected = obligation(&db, c, Some(9), Some(7), "rejected").await;
    let before = fingerprint(&db).await;
    let first = read(&db, &body).await;
    assert_eq!(first["captured_purchase_units"], "0");
    assert_eq!(first["captured_basis"]["live_captured_record_count"], "0");
    assert_eq!(first["historic_prior_refund_status"], "unknown");
    assert!(first["remaining_purchase_capacity"].is_null());
    assert!(find(&first, unknown)["remaining_units"].is_null());
    assert!(find(&first, unknown)["remaining_cash_units"].is_null());
    assert_eq!(find(&first, rejected)["remaining_units"], "0");
    assert_eq!(find(&first, rejected)["remaining_cash_units"], "7");
    assert_eq!(before, fingerprint(&db).await);
    println!("AD1 NULL/LIVEZERO {first}");
    let mut wrong = body.clone();
    wrong["subject"] = json!(Uuid::new_v4());
    assert!(read(&db, &wrong).await.is_null());
    wrong["case_id"] = json!(Uuid::new_v4());
    assert!(read(&db, &wrong).await.is_null());
    basis(&db, c, 0).await;
    assert_eq!(read(&db, &body).await["remaining_purchase_capacity"], "0");
    capture(&db, a, 91000001, 9007199254740993).await;
    let refreshed = read(&db, &body).await;
    assert_eq!(refreshed["captured_purchase_units"], "9007199254740993");
    assert_eq!(refreshed["remaining_purchase_capacity"], "9007199254740993");
    basis(&db, c, 3).await;
    assert_eq!(
        read(&db, &body).await["remaining_purchase_capacity"],
        "9007199254740990"
    );
    // A closed case of an absent original user remains a narrow original lookup.
    let erased = Uuid::new_v4();
    let ec = case(&db, erased).await;
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("UPDATE commercial_cases SET erased_at=clock_timestamp(),closed_at=clock_timestamp() WHERE id=$1",&[&ec]).await.unwrap();
    tx.commit().await.unwrap();
    let eb = staff_body(&db, ec, erased).await;
    let mut tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    assert_eq!(
        Repo.commercial_case_subject(&mut tx, ec).await.unwrap(),
        Some(erased.into())
    );
    assert_eq!(
        Repo.commercial_case_subject(&mut tx, Uuid::new_v4())
            .await
            .unwrap(),
        None
    );
    tx.commit().await.unwrap();
    basis(&db, ec, 0).await;
    assert!(read(&db, &eb).await["captured_purchase_units"].is_null());
    assert!(read(&db, &eb).await["remaining_purchase_capacity"].is_null());
    let tx = db.begin_transaction().await.unwrap();
    let eid = Uuid::new_v4();
    tx.txn().execute("INSERT INTO commercial_evidence(id,case_id,category,source_key,evidence) VALUES($1,$2,'wallet_boundary',$3,'{\"captured_purchase_units\":1234,\"private\":\"not projected\"}')",&[&eid,&ec,&erased.to_string()]).await.unwrap();
    tx.commit().await.unwrap();
    let ebefore = fingerprint(&db).await;
    let observed = read(&db, &eb).await;
    assert_eq!(observed["captured_purchase_units"], "1234");
    assert_eq!(observed["captured_basis"]["evidence_id"], eid.to_string());
    assert!(observed["captured_basis"]["live_captured_record_count"].is_null());
    assert!(!observed.to_string().contains("not projected"));
    assert_eq!(ebefore, fingerprint(&db).await);
    println!("AD2 EXACT CLOSED/ERASED CASE; AD1 PRESERVED {observed}");
    // Invalid internal shape and current staff revocation must not return a read.
    for kind in ["extra", "null", "wrong_staff", "mfa"] {
        let mut b = body.clone();
        if kind == "extra" {
            b["command_id"] = json!(Uuid::new_v4());
        }
        if kind == "null" {
            b["subject"] = Value::Null;
        }
        if kind == "wrong_staff" {
            b["_staff_session"] = json!(Uuid::new_v4());
        }
        if kind == "mfa" {
            let tx = db.begin_transaction().await.unwrap();
            tx.txn()
                .execute(
                    "UPDATE sessions SET mfa_verified=false WHERE user_id=$1",
                    &[&a],
                )
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
        let mut tx = db.begin_transaction().await.unwrap();
        assert!(
            Repo.commercial_operation(&mut tx, "admin_cash_capacity", Some(a.into()), &b)
                .await
                .is_err()
        );
    }
    common::fixture::preserve(&db, "staff-null-owner-final").await;
}
async fn split(
    db: &PostgresDatabase,
    body: &Value,
    parent: Uuid,
    parts: &[(Uuid, i64, Option<i64>)],
) {
    let children: Vec<Value> = parts
        .iter()
        .map(|(id, units, cap)| json!({"id":id,"units":units,"purchase_capacity_units":cap}))
        .collect();
    let mut b = body.clone();
    b["command_id"] = json!(Uuid::new_v4());
    b["reservation_id"] = json!(parent);
    b["children"] = json!(children);
    b["assessment"] = json!("Synthetic exact partition retains original known or unknown capacity");
    let mut tx = db.begin_transaction().await.unwrap();
    let r = Repo
        .commercial_operation(
            &mut tx,
            "split_cash",
            Some(academy_demo::user::FOO.user.id),
            &b,
        )
        .await
        .unwrap();
    assert_eq!(r["state"], "split");
    tx.commit().await.unwrap();
}
#[tokio::test]
async fn staff_capacity_mixed_and_nested_split_totals_and_upgrade_history() {
    let db = common::setup_through(Some("2026-09-11-020000_pending_determination")).await;
    let a = *academy_demo::user::FOO.user.id;
    let c = case(&db, a).await;
    let body = staff_body(&db, c, a).await;
    basis(&db, c, 10).await;
    capture(&db, a, 91000002, 1000).await;
    let o = obligation(&db, c, Some(100), Some(80), "established").await;
    let parent = reserve(&db, o, 20, "cash", "reserved", Some(20)).await;
    reserve(&db, o, 10, "cash", "uncertain", Some(5)).await;
    reserve(&db, o, 7, "cash", "completed", Some(7)).await;
    reserve(&db, o, 9, "cash", "failed", Some(0)).await;
    reserve(&db, o, 4, "wallet", "completed", None).await;
    reserve(&db, o, 3, "redemption", "reserved", None).await;
    let other = case(&db, Uuid::new_v4()).await;
    obligation(&db, other, Some(99999), Some(99999), "established").await;
    let initial = read(&db, &body).await;
    assert_eq!(initial["remaining_purchase_capacity"], "958");
    assert_eq!(initial["known_reserved_purchase_capacity_units"], "20");
    assert_eq!(initial["known_uncertain_purchase_capacity_units"], "5");
    assert_eq!(
        initial["known_recorded_completed_purchase_capacity_units"],
        "7"
    );
    assert_eq!(find(&initial, o)["counted_reservation_units"], "44");
    assert_eq!(find(&initial, o)["counted_cash_reservation_units"], "37");
    assert_eq!(find(&initial, o)["remaining_units"], "56");
    assert_eq!(find(&initial, o)["remaining_cash_units"], "43");
    let p1 = Uuid::new_v4();
    let p2 = Uuid::new_v4();
    split(&db, &body, parent, &[(p1, 12, Some(12)), (p2, 8, Some(8))]).await;
    split(
        &db,
        &body,
        p2,
        &[(Uuid::new_v4(), 3, Some(3)), (Uuid::new_v4(), 5, Some(5))],
    )
    .await;
    let nested = read(&db, &body).await;
    assert_eq!(nested["remaining_purchase_capacity"], "958");
    assert_eq!(find(&nested, o), find(&initial, o));
    assert_eq!(
        nested["reservations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["state"] == "split")
            .count(),
        2
    );
    // Represent an already retained pre-capacity reservation. The current writer
    // refuses new unknown service capacity; only this synthetic seed skips its
    // INSERT guard. Re-enable it before the actual split and every reader call.
    let legacy = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute(
            "ALTER TABLE commercial_reservations DISABLE TRIGGER commercial_cash_reservation_guard",
        )
        .await
        .unwrap();
    tx.txn().execute("INSERT INTO commercial_reservations(id,obligation_id,units,mode,state,request,purchase_capacity_units) VALUES($1,$2,6,'cash','uncertain','{\"legacy_unknown_fixture\":true}',NULL)",&[&legacy,&o]).await.unwrap();
    tx.txn()
        .batch_execute(
            "ALTER TABLE commercial_reservations ENABLE TRIGGER commercial_cash_reservation_guard",
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let one = read(&db, &body).await;
    assert!(one["remaining_purchase_capacity"].is_null());
    assert_eq!(one["unknown_reservation_capacity_count"], "1");
    split(
        &db,
        &body,
        legacy,
        &[(Uuid::new_v4(), 2, None), (Uuid::new_v4(), 4, None)],
    )
    .await;
    let before = fingerprint(&db).await;
    let observed = read(&db, &body).await;
    assert!(observed["remaining_purchase_capacity"].is_null());
    assert_eq!(observed["unknown_reservation_capacity_count"], "2");
    assert_eq!(find(&observed, o)["counted_reservation_units"], "50");
    assert_eq!(find(&observed, o)["counted_cash_reservation_units"], "43");
    assert_eq!(find(&observed, o)["remaining_units"], "50");
    assert_eq!(find(&observed, o)["remaining_cash_units"], "37");
    assert!(!observed.to_string().contains("private_original"));
    assert!(!observed.to_string().contains("capacity_basis"));
    assert_eq!(before, fingerprint(&db).await);
    println!("AD1 MIXED/NESTED KNOWN AND UNKNOWN; NO FINANCIAL EXECUTION CLAIM {observed}");
    let tx = db.begin_transaction().await.unwrap();
    let row=tx.txn().query_one("SELECT provolatile::text,prosrc FROM pg_proc WHERE oid='commercial_admin_cash_capacity(uuid,uuid)'::regprocedure",&[]).await.unwrap();
    assert_eq!(row.get::<_, &str>(0), "s");
    let source: String = row.get(1);
    assert!(!source.contains("commercial_cash_capacity("));
    assert!(!source.contains("FOR UPDATE"));
    let prior:String=tx.txn().query_one("SELECT prosrc FROM pg_proc WHERE oid='commercial_operation_before_staff_reads(text,uuid,jsonb)'::regprocedure",&[]).await.unwrap().get(0);
    assert!(prior.contains("commercial_operation_before_invoice_identity"));
    let plan: Vec<String> = tx
        .txn()
        .query(
            "EXPLAIN SELECT commercial_admin_cash_capacity($1,$2)",
            &[&c, &a],
        )
        .await
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    println!("AD1 STABLE QUERY PLAN {plan:?}");
    tx.commit().await.unwrap();
    assert_eq!(
        db.revert_migrations(Some(3)).await.unwrap(),
        vec![
            "2026-09-11-020000_pending_determination",
            "2026-09-11-010000_retained_hold_review",
            "2026-09-10-040000_staff_commercial_reads"
        ]
    );
    let tx = db.begin_transaction().await.unwrap();
    let reverted:String=tx.txn().query_one("SELECT prosrc FROM pg_proc WHERE oid='commercial_operation(text,uuid,jsonb)'::regprocedure",&[]).await.unwrap().get(0);
    assert_eq!(reverted, prior);
    assert!(
        tx.txn()
            .query_one(
                "SELECT to_regprocedure('commercial_admin_cash_capacity(uuid,uuid)') IS NULL",
                &[]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    tx.commit().await.unwrap();
    assert_eq!(
        common::apply_through(&db, Some("2026-09-11-020000_pending_determination")).await,
        vec![
            "2026-09-10-040000_staff_commercial_reads",
            "2026-09-11-010000_retained_hold_review",
            "2026-09-11-020000_pending_determination"
        ]
    );
    assert_eq!(before, fingerprint(&db).await);
    let final_value = read(&db, &body).await;
    let mut x = observed.clone();
    let mut y = final_value.clone();
    x.as_object_mut().unwrap().remove("observed_at");
    y.as_object_mut().unwrap().remove("observed_at");
    assert_eq!(x, y);
    println!("AD1 UPGRADE/DOWNGRADE RESTORED EXACT IMMEDIATE IF1 WRAPPER AND DATA");
    common::fixture::preserve(&db, "staff-mixed-history-final").await;
}
