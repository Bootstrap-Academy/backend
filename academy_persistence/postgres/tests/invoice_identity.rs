//! Normal synthetic SQL validation only. Every reset requires parsed and actual
//! ownership checks against a newly initialized, marked disposable server.
use academy_models::finance::FinancialDocumentKind;
mod common;
use academy_persistence_contracts::{Database, Transaction, finance::FinancialDocumentRepository};
use academy_persistence_postgres::{
    MIGRATIONS, PostgresDatabase, PostgresTransaction,
    finance::PostgresFinancialDocumentRepository as Finance,
};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;
const FORWARD: &str = "2026-09-10-020000_invoice_identity";
const CAPTURE: &str = "2026-09-03-200000_create_financial_documents";
const CONSENT: &str = "2026-09-07-100000_add_withdrawal_consent_to_financial_documents";
const A: Uuid = Uuid::from_u128(0x11111111111111111111111111111111);
const B: Uuid = Uuid::from_u128(0x22222222222222222222222222222222);
async fn setup(expected: &str) -> PostgresDatabase {
    println!("HISTORICAL_CASE {expected}");
    common::fixture::fresh_history().await
}
async fn before(db: &PostgresDatabase, name: &str) {
    let n = MIGRATIONS.iter().position(|m| m.name == name).unwrap();
    assert_eq!(db.run_migrations(Some(n)).await.unwrap().len(), n);
}
async fn user(tx: &PostgresTransaction, id: Uuid, name: &str) {
    tx.txn().execute("INSERT INTO users(id,name,email_verified,created_at,enabled,admin) VALUES($1,$2,false,'2020-01-01',true,false)",&[&id,&name]).await.unwrap();
    tx.txn()
        .execute(
            "INSERT INTO user_profiles(user_id,display_name,bio,tags) VALUES($1,$2,'','{}')",
            &[&id, &name],
        )
        .await
        .unwrap();
    tx.txn()
        .execute("INSERT INTO user_invoice_info(user_id) VALUES($1)", &[&id])
        .await
        .unwrap();
}
async fn payment(tx: &PostgresTransaction, id: &str, n: i64, owner: Uuid, fulfilled: bool) {
    tx.txn().execute("INSERT INTO paypal_payments(order_id,invoice_number,user_id,request_id,snapshot,capture_id,capture,balance,withheld_balance,fulfilled_at) VALUES($1,$2,$3,$4,'{}',CASE WHEN $5 THEN $1 END,CASE WHEN $5 THEN '{}' END,CASE WHEN $5 THEN 0 END,CASE WHEN $5 THEN 0 END,CASE WHEN $5 THEN '2026-09-01'::timestamptz END)",&[&id,&n,&owner,&Uuid::new_v4(),&fulfilled]).await.unwrap();
}
async fn document(tx: &PostgresTransaction, n: &str, kind: &str, owner: Option<Uuid>) {
    tx.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES($1,$2,$3,'2000-01-01',ARRAY['SYNTHETIC ORIGINAL'])",&[&n,&kind,&owner]).await.unwrap();
}
async fn original(tx: &PostgresTransaction, n: &str) {
    tx.txn().execute("INSERT INTO invoice_originals(invoice_number,pdf,provenance) VALUES($1,$2,'synthetic_original')",&[&n,&format!("%PDF synthetic original {n}").into_bytes()]).await.unwrap();
}
async fn export(tx: &PostgresTransaction, id: Uuid) -> Value {
    serde_json::from_str(
        &tx.txn()
            .query_one(
                "SELECT backend_moderation('retained_records',$1,'{}')::text",
                &[&id],
            )
            .await
            .unwrap()
            .get::<_, String>(0),
    )
    .unwrap()
}
async fn owned(tx: &mut PostgresTransaction, id: Uuid, n: u64) -> bool {
    Finance
        .owned_original_number(tx, id.into(), FinancialDocumentKind::Invoice, n, 0)
        .await
        .unwrap()
        .is_some()
}
async fn retention(tx: &PostgresTransaction, id: Uuid, n: &str, kind: &str) -> bool {
    tx.txn()
        .query_one(
            "SELECT commercial_retention_owned($1,$2,$3)",
            &[&id, &n, &kind],
        )
        .await
        .unwrap()
        .get(0)
}
async fn fingerprint(tx: &PostgresTransaction) -> String {
    tx.txn().query_one("SELECT encode(sha256(convert_to(jsonb_build_object('documents',(SELECT jsonb_agg(to_jsonb(d) ORDER BY number) FROM financial_documents d),'originals',(SELECT jsonb_agg(to_jsonb(i) ORDER BY invoice_number) FROM invoice_originals i),'payments',(SELECT jsonb_agg(to_jsonb(p) ORDER BY order_id) FROM paypal_payments p),'orders',(SELECT jsonb_agg(to_jsonb(o) ORDER BY id) FROM paypal_coin_orders o),'associations',(SELECT jsonb_agg(to_jsonb(o) ORDER BY number,kind,subject) FROM commercial_retention_owners o))::text,'UTF8')),'hex')",&[]).await.unwrap().get(0)
}
#[tokio::test]
async fn invoice_identity_forward_controls() {
    let baseline = std::env::var("IF1_BASELINE").as_deref() == Ok("true");
    let db = setup(if baseline {
        "l3_if1_baseline"
    } else {
        "l3_if1_corrected"
    })
    .await;
    before(&db, FORWARD).await;
    let tx = db.begin_transaction().await.unwrap();
    user(&tx, A, "IF1_A").await;
    user(&tx, B, "IF1_B").await;
    payment(&tx, "long-A", 10000000, A, true).await;
    payment(&tx, "short-B", 1000000, B, true).await;
    payment(&tx, "false-capture-A", 20000000, A, true).await;
    payment(&tx, "negative-B", 3000000, B, false).await;
    document(&tx, "R10000000", "invoice", Some(A)).await;
    document(&tx, "R1000000", "invoice", Some(B)).await;
    original(&tx, "R10000000").await;
    original(&tx, "R1000000").await;
    tx.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,captured_at,coins,invoice_number,withdrawal_consent_at) VALUES('historical-long', $1,'2020-01-01','2020-01-02',0,10000001,'2020-01-01')",&[&A]).await.unwrap();
    tx.txn()
        .execute(
            "SELECT commercial_capture_retention_owner('R2000000','invoice')",
            &[],
        )
        .await
        .unwrap();
    document(&tx, "R3000000", "invoice", Some(A)).await;
    original(&tx, "R3000000").await;
    document(&tx, "R4000000", "invoice", None).await;
    tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document','R4000000')",&[&A]).await.unwrap();
    document(&tx, "G202601-42", "credit_note", Some(A)).await;
    let old = fingerprint(&tx).await;
    println!("BEFORE_FORWARD original-evidence SHA256 {old}");
    let observed = export(&tx, A).await;
    println!("BASELINE OWNED EXPORT: {}", observed);
    assert!(
        observed["original_invoices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["invoice_number"] == "R1000000")
    );
    assert!(retention(&tx, A, "R2000000", "invoice").await);
    tx.commit().await.unwrap();
    if !baseline {
        assert_eq!(
            db.run_migrations(None).await.unwrap(),
            vec![
                FORWARD,
                RESIDUAL,
                "2026-09-10-040000_staff_commercial_reads",
                "2026-09-11-010000_retained_hold_review",
                "2026-09-11-020000_pending_determination",
                "2026-09-11-030000_invoice_disposal_preservation",
                "2026-09-11-040000_retention_family_paging",
                "2026-09-11-050000_wallet_restore_target"
            ]
        );
    }
    let mut tx = db.begin_transaction().await.unwrap();
    assert_eq!(
        fingerprint(&tx).await,
        old,
        "forward migration must preserve original rows, PDF, consent and old associations"
    );
    let current = export(&tx, A).await;
    println!("AFTER_SELECTED_SOURCE OWNED EXPORT: {current}");
    assert!(
        !current["original_invoices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["invoice_number"] == "R1000000"),
        "IF1 red gate: long A must not receive shorter B original"
    );
    assert!(
        current["original_invoices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["invoice_number"] == "R10000000")
    );
    assert!(!current.to_string().contains(&B.to_string()));
    assert!(!retention(&tx, A, "R2000000", "invoice").await);
    assert!(!owned(&mut tx, A, 3000000).await);
    assert!(!owned(&mut tx, B, 3000000).await);
    assert!(owned(&mut tx, A, 4000000).await);
    assert!(owned(&mut tx, B, 1000000).await);
    assert!(!owned(&mut tx, A, 1000000).await);
    assert!(
        Finance
            .lock_archive(&mut tx, &"R1000000".try_into().unwrap())
            .await
            .unwrap_err()
            .to_string()
            .contains("pending independent review")
    );
    assert!(
        Finance
            .original_invoice(&mut tx, &"R1000000".try_into().unwrap())
            .await
            .is_err()
    );
    assert_eq!(
        Finance
            .original_invoice(&mut tx, &"R10000000".try_into().unwrap())
            .await
            .unwrap(),
        Some(b"%PDF synthetic original R10000000".to_vec())
    );
    assert!(retention(&tx, A, "G202601-42", "credit_note").await);
    assert!(
        Finance
            .lock_archive(&mut tx, &"G202601-42".try_into().unwrap())
            .await
            .unwrap()
    );
    for (n, expected) in [
        (0_i64, "R0000000"),
        (9999999, "R9999999"),
        (10000000, "R10000000"),
        (10000001, "R10000001"),
        (9223372036854775807, "R9223372036854775807"),
    ] {
        let r = tx
            .txn()
            .query_one(
                "SELECT commercial_invoice_number($1),commercial_invoice_numeric($2)",
                &[&n, &expected],
            )
            .await
            .unwrap();
        assert_eq!(r.get::<_, &str>(0), expected);
        assert_eq!(r.get::<_, Option<i64>>(1), Some(n));
    }
    for bad in [
        "R010000000",
        "R9223372036854775808",
        "R-000001",
        "R123",
        "R1.000000",
    ] {
        assert_eq!(
            tx.txn()
                .query_one("SELECT commercial_invoice_numeric($1)", &[&bad])
                .await
                .unwrap()
                .get::<_, Option<i64>>(0),
            None
        );
    }
    println!(
        "GROUP 1: canonical full bigint identities, exact numeric negative precedence, genuine retained mapping, non-invoice compatibility and original fingerprints passed"
    );
    // A new passed capture is explicitly denied while the higher exact source exists.
    payment(&tx, "denying-source", 6000000, B, true).await;
    tx.txn()
        .execute(
            "SELECT commercial_capture_retention_owner('R6000000','invoice',$1)",
            &[&A],
        )
        .await
        .unwrap();
    assert!(!retention(&tx, A, "R6000000", "invoice").await);
    tx.txn()
        .execute(
            "DELETE FROM paypal_payments WHERE order_id='denying-source'",
            &[],
        )
        .await
        .unwrap();
    assert!(!retention(&tx, A, "R6000000", "invoice").await);
    assert!(!owned(&mut tx, A, 6000000).await);
    assert!(!tx.txn().query_one("SELECT bool_or(qualified) FROM commercial_invoice_owner_observations WHERE number='R6000000' AND subject=$1",&[&A]).await.unwrap().get::<_,bool>(0));
    // Actual document DELETE trigger keeps new metadata evidence, not byte authority.
    document(&tx, "R5000000", "invoice", Some(A)).await;
    tx.txn()
        .execute(
            "DELETE FROM financial_documents WHERE number='R5000000'",
            &[],
        )
        .await
        .unwrap();
    assert!(retention(&tx, A, "R5000000", "invoice").await);
    assert!(!owned(&mut tx, A, 5000000).await);
    let witnesses: Value = serde_json::from_str(
        &tx.txn()
            .query_one(
                "SELECT commercial_operation('retention_queue',$1,'{}')::text",
                &[&A],
            )
            .await
            .unwrap()
            .get::<_, String>(0),
    )
    .unwrap();
    assert!(
        !witnesses["invoice_identity_reviews"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        witnesses["unqualified_invoice_owner_observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["number"] == "R2000000")
    );
    let current = export(&tx, A).await;
    assert!(!current.to_string().contains("legacy_branch_unknown"));
    assert!(!current.to_string().contains(&B.to_string()));
    println!(
        "GROUP 2: permanently unqualified denied capture, actual deletion metadata, no witness download grant and staff-only pending observations passed"
    );
    // Reconciliation must not mistake a complete long identifier for an orphan.
    tx.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,coins,invoice_number) VALUES('canonical-live',$1,'2020-01-01',0,80000000)",&[&A]).await.unwrap();
    document(&tx, "R80000000", "invoice", Some(A)).await;
    Finance.invoice_reconciliation(&mut tx).await.unwrap();
    assert!(!tx.txn().query_one("SELECT EXISTS(SELECT 1 FROM invoice_reconciliation WHERE invoice_number='R80000000')",&[]).await.unwrap().get::<_,bool>(0));
    println!("GROUP 3: current Rust reconciliation uses full original identifier");
    tx.commit().await.unwrap();
}
#[tokio::test]
async fn pending_capture_guard_after_real_source_wait() {
    let db = setup("l3_if1_history_capture").await;
    before(&db, CAPTURE).await;
    let tx = db.begin_transaction().await.unwrap();
    user(&tx, A, "IF1_HISTORY_A").await;
    tx.commit().await.unwrap();
    let writer = db.begin_transaction().await.unwrap();
    let writer_pid: i32 = writer
        .txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0);
    writer.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,captured_at,coins,invoice_number) VALUES('long-pending',$1,'2020-01-01','2020-01-02',0,10000000)",&[&A]).await.unwrap();
    let runner = db.clone();
    let run = tokio::spawn(async move { runner.run_migrations(Some(1)).await });
    let observer = db.begin_transaction().await.unwrap();
    let mut waited = false;
    for _ in 0..100 {
        let r=observer.txn().query_opt("SELECT a.pid,a.query,a.wait_event_type,a.wait_event FROM pg_stat_activity a JOIN pg_locks l ON l.pid=a.pid WHERE l.relation='paypal_coin_orders'::regclass AND l.mode='ShareLock' AND NOT l.granted AND $1=ANY(pg_blocking_pids(a.pid))",&[&writer_pid]).await.unwrap();
        if let Some(r) = r {
            println!(
                "ACTUAL RUNNER WAIT: holder {writer_pid}, waiter {}, SQL {}, event {}/{}",
                r.get::<_, i32>(0),
                r.get::<_, String>(1),
                r.get::<_, String>(2),
                r.get::<_, String>(3)
            );
            waited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(waited, "actual runner must wait on the source writer");
    observer.commit().await.unwrap();
    writer.commit().await.unwrap();
    let err = run.await.unwrap().unwrap_err().to_string();
    assert!(err.contains(CAPTURE) && err.contains("remains pending"));
    let tx = db.begin_transaction().await.unwrap();
    assert!(
        !tx.txn()
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name=$1)",
                &[&CAPTURE]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    assert!(
        tx.txn()
            .query_one("SELECT to_regclass('financial_documents') IS NULL", &[])
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    tx.commit().await.unwrap();
    println!(
        "GROUP 4: committed source after actual SHARE wait refuses before old DDL/marker; input preserved"
    );
}
#[tokio::test]
async fn pending_consent_guard_preserves_prior_history() {
    let db = setup("l3_if1_history_consent").await;
    before(&db, CONSENT).await;
    let tx = db.begin_transaction().await.unwrap();
    user(&tx, A, "IF1_CONSENT_A").await;
    tx.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,coins,invoice_number,withdrawal_consent_at) VALUES('consent-long',$1,'2020-01-01',0,10000000,'2020-01-01')",&[&A]).await.unwrap();
    tx.commit().await.unwrap();
    let err = db.run_migrations(Some(1)).await.unwrap_err().to_string();
    assert!(err.contains(CONSENT) && err.contains("remains pending"));
    let tx = db.begin_transaction().await.unwrap();
    assert!(
        !tx.txn()
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name=$1)",
                &[&CONSENT]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    assert!(!tx.txn().query_one("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_name='financial_documents' AND column_name='withdrawal_consent_at')",&[]).await.unwrap().get::<_,bool>(0));
    assert!(
        tx.txn()
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name=$1)",
                &[&CAPTURE]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    tx.commit().await.unwrap();
    println!(
        "GROUP 5: long consent source refused independently of join/capture; earlier safe migration remains applied"
    );
}
#[tokio::test]
async fn unqualified_long_source_and_empty_history_proceed() {
    let db = setup("l3_if1_history_safe").await;
    before(&db, CAPTURE).await;
    let tx = db.begin_transaction().await.unwrap();
    user(&tx, A, "IF1_SAFE_A").await;
    tx.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,coins,invoice_number) VALUES('uncaptured-long',$1,'2020-01-01',0,10000000)",&[&A]).await.unwrap();
    tx.commit().await.unwrap();
    let applied = db.run_migrations(None).await.unwrap();
    assert!(
        applied.contains(&CAPTURE)
            && applied.contains(&CONSENT)
            && applied.contains(&FORWARD)
            && applied.contains(&RESIDUAL)
    );
    let tx = db.begin_transaction().await.unwrap();
    assert_eq!(
        tx.txn()
            .query_one("SELECT count(*) FROM financial_documents", &[])
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    tx.commit().await.unwrap();
    println!(
        "GROUP 6: actual old pending backfills allow nonqualifying long source and complete forward migration"
    );
}

const RESIDUAL: &str = "2026-09-10-030000_invoice_retention_identity";
async fn pending(tx: &PostgresTransaction, number: &str) -> bool {
    tx.txn()
        .query_one("SELECT commercial_invoice_identity_pending($1)", &[&number])
        .await
        .unwrap()
        .get(0)
}
async fn rich(tx: &PostgresTransaction, number: &str, kind: &str) {
    tx.txn().execute("INSERT INTO commercial_archive_work(number,kind,source,assessment) VALUES($1,$2,'unrecorded_archive',jsonb_build_object('private_assessment',$1::text)) ON CONFLICT(number,kind) DO UPDATE SET assessment=EXCLUDED.assessment", &[&number,&kind]).await.unwrap();
    tx.txn().execute("INSERT INTO commercial_journal(command_id,kind,request,result) VALUES($1,'archive_review',jsonb_build_object('number',$2::text,'kind',$3::text),jsonb_build_object('private_review',$2::text))", &[&Uuid::new_v4(),&number,&kind]).await.unwrap();
}
async fn retention_exports(tx: &PostgresTransaction, subject: Uuid) -> (Value, Value) {
    let r = tx
        .txn()
        .query_one(
            "SELECT commercial_retention_export($1)::text,commercial_case_export($1)::text",
            &[&subject],
        )
        .await
        .unwrap();
    (
        serde_json::from_str(&r.get::<_, String>(0)).unwrap(),
        serde_json::from_str(&r.get::<_, String>(1)).unwrap(),
    )
}
fn has_rich(value: &Value, number: &str) -> bool {
    value["archive_work"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["number"] == number)
        || value["history"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["request"]["number"] == number)
        || value["owner_associations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["number"] == number)
}
async fn correction_fingerprint(tx: &PostgresTransaction) -> String {
    tx.txn().query_one("SELECT encode(sha256(convert_to(jsonb_build_object('original', $1::text, 'retained_inventory',(SELECT jsonb_agg(to_jsonb(o) ORDER BY kind,record_id) FROM moderation_retained_record_owners o),'observations',(SELECT jsonb_agg(to_jsonb(o) ORDER BY number,subject,basis,evidence_hash) FROM commercial_invoice_owner_observations o),'archive_work',(SELECT jsonb_agg(to_jsonb(w) ORDER BY number,kind) FROM commercial_archive_work w),'journal',(SELECT jsonb_agg(to_jsonb(j) ORDER BY id) FROM commercial_journal j))::text,'UTF8')),'hex')",&[&fingerprint(tx).await]).await.unwrap().get(0)
}
#[tokio::test]
async fn pending_invoice_rich_export_controls() {
    let baseline = std::env::var("IF1_RESIDUAL_BASELINE").as_deref() == Ok("true");
    let db = setup(if baseline {
        "l3_if1_r1_red"
    } else {
        "l3_if1_r1_fixed"
    })
    .await;
    before(&db, FORWARD).await;
    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), vec![FORWARD]);
    let tx = db.begin_transaction().await.unwrap();
    user(&tx, A, "RESIDUAL_R1_A").await;
    tx.txn()
        .execute("INSERT INTO commercial_cases(subject) VALUES($1)", &[&A])
        .await
        .unwrap();
    payment(&tx, "R1-current", 7000001, A, true).await;
    document(&tx, "R7000001", "invoice", Some(A)).await;
    original(&tx, "R7000001").await;
    for n in ["R7000002", "R7000003"] {
        document(&tx, n, "invoice", Some(A)).await;
        tx.txn()
            .execute("DELETE FROM financial_documents WHERE number=$1", &[&n])
            .await
            .unwrap();
        assert!(
            retention(&tx, A, n, "invoice").await,
            "actual disposal establishes qualified metadata witness"
        );
    }
    tx.txn().execute("INSERT INTO commercial_retention_owners(number,kind,subject,source) VALUES('R7000004','invoice',$1,'unqualified_old_observation')",&[&A]).await.unwrap();
    document(&tx, "G202601-71", "credit_note", Some(A)).await;
    document(&tx, "S71", "final_statement", Some(A)).await;
    for (n, k) in [
        ("R7000001", "invoice"),
        ("R7000002", "invoice"),
        ("R7000003", "invoice"),
        ("R7000004", "invoice"),
        ("G202601-71", "credit_note"),
        ("S71", "final_statement"),
    ] {
        rich(&tx, n, k).await;
    }
    tx.txn().execute("INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence) VALUES('R7000001','synthetic_pending','r1-current','{}'),('R7000002','synthetic_pending','r1-qualified','{}')",&[]).await.unwrap();
    let old = correction_fingerprint(&tx).await;
    let reviews:String=tx.txn().query_one("SELECT jsonb_agg(to_jsonb(r) ORDER BY number)::text FROM commercial_invoice_identity_reviews r",&[]).await.unwrap().get(0);
    let (direct, outer) = retention_exports(&tx, A).await;
    println!("R1 INITIAL020000 DIRECT {direct}; OUTER {outer}");
    assert!(has_rich(&direct, "R7000001") && has_rich(&direct, "R7000002"));
    assert_eq!(outer["retention_reviews"], direct);
    tx.commit().await.unwrap();
    if !baseline {
        assert_eq!(
            db.run_migrations(None).await.unwrap(),
            vec![
                RESIDUAL,
                "2026-09-10-040000_staff_commercial_reads",
                "2026-09-11-010000_retained_hold_review",
                "2026-09-11-020000_pending_determination",
                "2026-09-11-030000_invoice_disposal_preservation",
                "2026-09-11-040000_retention_family_paging",
                "2026-09-11-050000_wallet_restore_target"
            ]
        );
    }
    let tx = db.begin_transaction().await.unwrap();
    let (direct, outer) = retention_exports(&tx, A).await;
    let exposed = [
        has_rich(&direct, "R7000001"),
        has_rich(&direct, "R7000002"),
        has_rich(&outer["retention_reviews"], "R7000001"),
        has_rich(&outer["retention_reviews"], "R7000002"),
    ];
    println!(
        "R1 SELECTED SOURCE exposed numeric/qualified direct/outer: {exposed:?}; DIRECT {direct}; OUTER {outer}"
    );
    assert_eq!(
        exposed, [false; 4],
        "IF1-R1: pending invoices must not expose rich direct or current outer-case retention metadata"
    );
    assert!(
        !retention(&tx, A, "R7000001", "invoice").await
            && !retention(&tx, A, "R7000002", "invoice").await
    );
    assert!(has_rich(&direct, "R7000003") && has_rich(&direct, "G202601-71"));
    assert!(!has_rich(&direct, "R7000004"));
    assert!(
        has_rich(&direct, "S71")
            && direct["statement_reviews"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["number"] == "S71")
    );
    let retained = export(&tx, A).await;
    assert_eq!(
        retained["unavailable_invoice_references"],
        serde_json::json!([{"number":"R7000001","reason":"identity_pending_review"}])
    );
    assert!(retained["original_invoices"].as_array().unwrap().is_empty());
    assert!(
        !retained["financial_documents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["number"] == "R7000001")
    );
    assert_eq!(correction_fingerprint(&tx).await, old);
    assert_eq!(tx.txn().query_one("SELECT jsonb_agg(to_jsonb(r) ORDER BY number)::text FROM commercial_invoice_identity_reviews r",&[]).await.unwrap().get::<_,String>(0),reviews);
    // A case is not necessary for the same retention export path.
    let case_id: Uuid = tx
        .txn()
        .query_one("SELECT id FROM commercial_cases WHERE subject=$1", &[&A])
        .await
        .unwrap()
        .get(0);
    assert_eq!(outer["case"]["id"], case_id.to_string());
    tx.commit().await.unwrap();
    println!(
        "RESIDUAL GROUP R1: current numeric and qualified fallback pending gates, exact minimal refs, outer case export and noninvoice/genuine/unsupported preservation passed"
    );
}

#[tokio::test]
async fn erased_owner_identity_conflict_controls() {
    let baseline = std::env::var("IF1_RESIDUAL_BASELINE").as_deref() == Ok("true");
    let db = setup(if baseline {
        "l3_if1_r2_red"
    } else {
        "l3_if1_r2_fixed"
    })
    .await;
    before(&db, FORWARD).await;
    let tx = db.begin_transaction().await.unwrap();
    user(&tx, A, "RESIDUAL_R2_A").await;
    let live_b = Uuid::from_u128(0x33333333333333333333333333333333);
    user(&tx, live_b, "RESIDUAL_R2_LIVE_B").await;
    // B has no live user row: the exact inventory is durable pre-erasure ownership evidence.
    for (n, owner, retained_owner) in [
        ("R7000000", None, Some(B)),
        ("R7000005", None, Some(B)),
        ("R7000006", Some(A), Some(B)),
        ("R7000007", Some(live_b), None),
        ("R7000008", None, Some(B)),
        ("R10000000", Some(A), None),
    ] {
        document(&tx, n, "invoice", owner).await;
        original(&tx, n).await;
        rich(&tx, n, "invoice").await;
        if let Some(owner) = retained_owner {
            tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document',$2)",&[&owner,&n]).await.unwrap();
        }
    }
    payment(&tx, "R2-conflict", 7000000, A, true).await;
    payment(&tx, "R2-live-precedence", 7000006, A, true).await;
    payment(&tx, "R2-existing-conflict", 7000007, A, true).await;
    payment(&tx, "R2-long", 10000000, A, true).await;
    tx.txn().execute("INSERT INTO paypal_coin_orders(id,user_id,created_at,coins,invoice_number) VALUES('R2-legacy-conflict',$1,'2020-01-01',0,7000008)",&[&A]).await.unwrap();
    document(&tx, "G202601-72", "credit_note", None).await;
    tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document','G202601-72')",&[&B]).await.unwrap();
    tx.txn()
        .execute(
            "SELECT commercial_capture_retention_owner('G202601-72','credit_note')",
            &[],
        )
        .await
        .unwrap();
    assert!(
        retention(&tx, B, "G202601-72", "credit_note").await,
        "unchanged noninvoice capture establishes existing retained authority"
    );
    rich(&tx, "G202601-72", "credit_note").await;
    tx.txn().execute("INSERT INTO commercial_retention_owners(number,kind,subject,source) VALUES('R7999999','invoice',$1,'observed_original_record_owner')",&[&A]).await.unwrap();
    rich(&tx, "R7999999", "invoice").await;
    tx.commit().await.unwrap();
    assert_eq!(db.run_migrations(Some(1)).await.unwrap(), vec![FORWARD]);
    let mut tx = db.begin_transaction().await.unwrap();
    assert!(
        !pending(&tx, "R7000000").await && !pending(&tx, "R7000008").await,
        "initial seed misses both exact erased-owner contradictions"
    );
    assert!(
        pending(&tx, "R7000007").await,
        "initial live-owner conflict remains independently observed"
    );
    assert!(owned(&mut tx, A, 7000000).await && !owned(&mut tx, B, 7000000).await);
    assert!(
        Finance
            .original_invoice(&mut tx, &"R7000000".try_into().unwrap())
            .await
            .unwrap()
            .is_some()
    );
    let initial_review:String=tx.txn().query_one("SELECT to_jsonb(r)::text FROM commercial_invoice_identity_reviews r WHERE number='R7000007'",&[]).await.unwrap().get(0);
    let old = correction_fingerprint(&tx).await;
    println!(
        "R2 INITIAL020000: numeric A owns B-inventory original; no pending fence, original byte reader returned PDF; original fingerprint {old}"
    );
    tx.commit().await.unwrap();
    if !baseline {
        assert_eq!(
            db.run_migrations(None).await.unwrap(),
            vec![
                RESIDUAL,
                "2026-09-10-040000_staff_commercial_reads",
                "2026-09-11-010000_retained_hold_review",
                "2026-09-11-020000_pending_determination",
                "2026-09-11-030000_invoice_disposal_preservation",
                "2026-09-11-040000_retention_family_paging",
                "2026-09-11-050000_wallet_restore_target"
            ]
        );
    }
    let mut tx = db.begin_transaction().await.unwrap();
    let states = [
        pending(&tx, "R7000000").await,
        pending(&tx, "R7000008").await,
    ];
    println!(
        "R2 SELECTED SOURCE pending durable-payment/legacy-order erased-owner contradictions: {states:?}"
    );
    assert_eq!(
        states, [true; 2],
        "IF1-R2: forward observation must fence exact erased-owner contradiction"
    );
    assert!(
        owned(&mut tx, A, 7000000).await && !owned(&mut tx, B, 7000000).await,
        "numeric original authority unchanged"
    );
    for n in ["R7000000", "R7000008"] {
        let doc = n.try_into().unwrap();
        assert!(
            Finance
                .lock_archive(&mut tx, &doc)
                .await
                .unwrap_err()
                .to_string()
                .contains("pending independent review")
        );
        assert!(Finance.original_invoice(&mut tx, &doc).await.is_err());
        let r:Value=serde_json::from_str(&tx.txn().query_one("SELECT evidence::text FROM commercial_invoice_identity_reviews WHERE number=$1 AND reason='numeric_recorded_owner_conflict'",&[&n]).await.unwrap().get::<_,String>(0)).unwrap();
        assert_eq!(r["recorded_subject"], B.to_string());
        assert_eq!(r["recorded_owner_basis"], "exact_retained_document_owner");
        assert_eq!(r["retained_witness"]["record_id"], n);
        assert_eq!(r["retained_witness"]["subject"], B.to_string());
        assert_eq!(r["retained_witness"]["kind"], "financial_document");
        let exact:String=tx.txn().query_one("SELECT encode(sha256(convert_to(to_jsonb(o)::text,'UTF8')),'hex') FROM moderation_retained_record_owners o WHERE kind='financial_document' AND record_id=$1",&[&n]).await.unwrap().get(0);
        assert_eq!(r["retained_witness_sha256"], exact);
    }
    let (direct, outer) = retention_exports(&tx, A).await;
    assert_eq!(direct, outer["retention_reviews"]);
    assert!(
        outer.get("case").is_none(),
        "exercise current outer export without a commercial case"
    );
    for n in ["R7000000", "R7000007", "R7000008", "R7999999"] {
        assert!(!has_rich(&direct, n));
    }
    for n in ["R7000006", "R10000000"] {
        assert!(!pending(&tx, n).await && has_rich(&direct, n));
        assert!(
            Finance
                .original_invoice(&mut tx, &n.try_into().unwrap())
                .await
                .unwrap()
                .is_some()
        );
    }
    let retained = export(&tx, A).await;
    let refs = retained["unavailable_invoice_references"]
        .as_array()
        .unwrap();
    for n in ["R7000000", "R7000007", "R7000008"] {
        assert!(
            refs.iter()
                .any(|r| *r == serde_json::json!({"number":n,"reason":"identity_pending_review"}))
        );
        assert!(
            !retained["original_invoices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["invoice_number"] == n)
        );
    }
    assert!(!refs.iter().any(|r| r["number"] == "R7999999"));
    assert!(owned(&mut tx, B, 7000005).await && !pending(&tx, "R7000005").await);
    assert!(
        Finance
            .original_invoice(&mut tx, &"R7000005".try_into().unwrap())
            .await
            .unwrap()
            .is_some()
    );
    let (genuine, outer_b) = retention_exports(&tx, B).await;
    assert_eq!(genuine, outer_b["retention_reviews"]);
    assert!(has_rich(&genuine, "R7000005") && has_rich(&genuine, "G202601-72"));
    assert!(!has_rich(&genuine, "R7000000"));
    assert_eq!(correction_fingerprint(&tx).await, old);
    assert_eq!(tx.txn().query_one("SELECT to_jsonb(r)::text FROM commercial_invoice_identity_reviews r WHERE number='R7000007'",&[]).await.unwrap().get::<_,String>(0),initial_review);
    assert_eq!(
        tx.txn()
            .query_one(
                "SELECT count(*) FROM commercial_invoice_identity_reviews",
                &[]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        3
    );
    tx.commit().await.unwrap();
    assert!(
        db.run_migrations(None).await.unwrap().is_empty(),
        "already applied correction is not repeated"
    );
    println!(
        "RESIDUAL GROUP R2: exact erased-owner witnesses for payment and legacy order, unchanged numeric owner, byte/rich fences, genuine erased owner, live precedence, full long identity and immutable evidence passed"
    );
}
