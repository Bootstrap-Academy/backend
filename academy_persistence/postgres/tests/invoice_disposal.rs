//! New owned ordinary-invoice fixture; parsed config and actual identity precede every reset.
mod common;
use academy_models::finance::{FinancialDocumentKind, FinancialDocumentNumber};
use academy_persistence_contracts::{Database, Transaction, finance::FinancialDocumentRepository};
use academy_persistence_postgres::{
    PostgresDatabase, PostgresTransaction, finance::PostgresFinancialDocumentRepository as Finance,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use uuid::Uuid;
const FORWARD: &str = "2026-09-11-030000_invoice_disposal_preservation";
const A: Uuid = Uuid::from_u128(0x11111111111111111111111111111111);
const B: Uuid = Uuid::from_u128(0x22222222222222222222222222222222);
async fn setup() -> PostgresDatabase {
    setup_mode(false).await
}
async fn setup_mode(pre_forward: bool) -> PostgresDatabase {
    if pre_forward {
        common::setup_before(FORWARD, false).await
    } else {
        common::setup().await
    }
}

fn cutoff() -> DateTime<Utc> {
    "2010-01-01T00:00:00Z".parse().unwrap()
}
fn number(n: &str) -> FinancialDocumentNumber {
    n.try_into().unwrap()
}
async fn users(tx: &PostgresTransaction) {
    for (id, name) in [(A, "INVOICE_A"), (B, "INVOICE_B")] {
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
}
async fn document(tx: &PostgresTransaction, n: &str, kind: &str) {
    tx.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES($1,$2,$3,'2000-01-01',ARRAY['SYNTHETIC ORIGINAL'])",&[&n,&kind,&A]).await.unwrap();
    if kind == "invoice" {
        original(tx, n).await;
    }
}
async fn original(tx: &PostgresTransaction, n: &str) {
    tx.txn().execute("INSERT INTO invoice_originals(invoice_number,pdf,provenance) VALUES($1,$2,'synthetic_original')",&[&n,&format!("%PDF synthetic {n}").into_bytes()]).await.unwrap();
    tx.txn().execute("INSERT INTO invoice_reconciliation(invoice_number,state,reason) VALUES($1,'evidenced','synthetic retained original')",&[&n]).await.unwrap();
}
async fn pending(tx: &PostgresTransaction, n: &str) {
    tx.txn().execute("INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence) VALUES($1,'synthetic_pending','owned-disposal-control','{}') ON CONFLICT DO NOTHING",&[&n]).await.unwrap();
}
async fn snapshot(tx: &PostgresTransaction, n: &str) -> Value {
    let s:String=tx.txn().query_one("SELECT jsonb_build_object('record',(SELECT to_jsonb(d) FROM financial_documents d WHERE number=$1),'original',(SELECT to_jsonb(i) FROM invoice_originals i WHERE invoice_number=$1),'reconciliation',(SELECT to_jsonb(i) FROM invoice_reconciliation i WHERE invoice_number=$1),'work',(SELECT jsonb_agg(to_jsonb(w) ORDER BY kind) FROM commercial_archive_work w WHERE number=$1),'retained',(SELECT jsonb_agg(to_jsonb(o) ORDER BY subject) FROM moderation_retained_record_owners o WHERE kind='financial_document' AND record_id=$1),'reviews',(SELECT jsonb_agg(to_jsonb(r) ORDER BY reason,source_key) FROM commercial_invoice_identity_reviews r WHERE number=$1))::text",&[&n]).await.unwrap().get(0);
    serde_json::from_str(&s).unwrap()
}

#[tokio::test]
async fn pending_record_is_skipped_while_safe_sibling_advances() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    users(&tx).await;
    document(&tx, "R10000001", "invoice").await;
    document(&tx, "R10000002", "invoice").await;
    pending(&tx, "R10000001").await;
    let old = snapshot(&tx, "R10000001").await;
    tx.commit().await.unwrap();
    let mut tx = db.begin_transaction().await.unwrap();
    let result = Finance.delete_issued_before(&mut tx, cutoff()).await;
    println!("MIXED actual result {result:?}");
    let count = result.unwrap();
    let current = snapshot(&tx, "R10000001").await;
    let safe = snapshot(&tx, "R10000002").await;
    tx.commit().await.unwrap();
    assert_eq!(count, 1);
    assert_eq!(current, old);
    assert!(safe["record"].is_null() && safe["original"].is_null());
    assert_eq!(safe["work"][0]["source"], "record_disposal");
    println!(
        "GROUP mixed pending/safe: exact pending originals retained; one actual record deleted and queued"
    );
}

#[tokio::test]
async fn deletion_capture_preserves_new_conflict_and_both_after_consumers() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    users(&tx).await;
    document(&tx, "R10000003", "invoice").await;
    tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document','R10000003')",&[&A]).await.unwrap();
    tx.txn().execute("INSERT INTO paypal_payments(order_id,invoice_number,user_id,request_id,snapshot,capture_id,capture,balance,withheld_balance,fulfilled_at) VALUES('conflicting-source',10000003,$1,$2,'{}','capture','{}',0,0,'2026-09-01')",&[&B,&Uuid::new_v4()]).await.unwrap();
    assert!(
        !tx.txn()
            .query_one(
                "SELECT commercial_invoice_identity_pending('R10000003')",
                &[]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    let old = snapshot(&tx, "R10000003").await;
    tx.commit().await.unwrap();
    let mut tx = db.begin_transaction().await.unwrap();
    let count = Finance
        .delete_issued_before(&mut tx, cutoff())
        .await
        .unwrap();
    let now = snapshot(&tx, "R10000003").await;
    println!("CAPTURE actual deletion count {count}; current {now}");
    tx.commit().await.unwrap();
    assert_eq!(count, 0, "new conflict must skip actual DELETE");
    for key in ["record", "original", "reconciliation", "retained", "work"] {
        assert_eq!(now[key], old[key], "preserve {key}");
    }
    assert_eq!(now["reviews"].as_array().unwrap().len(), 1);
    assert_eq!(
        now["reviews"][0]["reason"],
        "numeric_recorded_owner_conflict"
    );
    println!(
        "GROUP capture: new observation persisted, original row/bytes/retained mapping kept, AFTER removal and queue consumers skipped"
    );
}

#[tokio::test]
async fn pending_orphan_and_begun_intent_never_admit_new_file_work() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    users(&tx).await;
    for (n, begun) in [("R10000004", false), ("R10000005", true)] {
        original(&tx, n).await;
        tx.txn().execute("INSERT INTO commercial_archive_work(number,kind,source,disposal_authorized,assessment,disposal_started_at) VALUES($1,'invoice','unrecorded_archive',true,'{\"original_assessment\":true}',CASE WHEN $2 THEN '2026-09-01'::timestamptz END)",&[&n,&begun]).await.unwrap();
        pending(&tx, n).await;
    }
    tx.commit().await.unwrap();
    for n in ["R10000004", "R10000005"] {
        let mut tx = db.begin_transaction().await.unwrap();
        let before = snapshot(&tx, n).await;
        let admitted = Finance
            .begin_archive_disposal(&mut tx, &number(n), FinancialDocumentKind::Invoice)
            .await
            .unwrap();
        let after = snapshot(&tx, n).await;
        println!("ORPHAN {n}: admitted={admitted}; after={after}");
        tx.commit().await.unwrap();
        assert!(!admitted);
        assert_eq!(before, after);
    }
    println!(
        "GROUP queued pending/begun: originals, assessment, reconciliation and original intent unchanged; false admission"
    );
}

async fn orphan(tx: &PostgresTransaction, n: &str, begun: bool) {
    original(tx, n).await;
    tx.txn().execute("INSERT INTO commercial_archive_work(number,kind,source,disposal_authorized,assessment,disposal_started_at) VALUES($1,'invoice','unrecorded_archive',true,'{\"old\":true}',CASE WHEN $2 THEN '2026-09-01'::timestamptz END)",&[&n,&begun]).await.unwrap();
}
async fn staff(db: &PostgresDatabase, n: &str, kind: &str) -> (Uuid, Value) {
    let actor = *academy_demo::user::FOO.user.id;
    let session = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .execute(
            "UPDATE users SET admin=true,enabled=true WHERE id=$1",
            &[&actor],
        )
        .await
        .unwrap();
    tx.txn().execute("INSERT INTO sessions(id,user_id,created_at,updated_at,mfa_verified) VALUES($1,$2,clock_timestamp(),clock_timestamp(),true)",&[&session,&actor]).await.unwrap();
    tx.txn()
        .execute(
            "INSERT INTO session_refresh_tokens(session_id,refresh_token_hash) VALUES($1,$2)",
            &[&session, &vec![0x5au8; 32]],
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (
        actor,
        json!({"command_id":Uuid::new_v4(),"number":n,"kind":kind,"assessment":"Synthetic human assessment retaining this original evidence","next_review_at":"2099-01-01T00:00:00Z","authorize_disposal":false,"_staff_session":session,"_staff_refresh_hash":"5a".repeat(32)}),
    )
}
async fn operation(
    tx: &PostgresTransaction,
    op: &str,
    actor: Uuid,
    body: &Value,
) -> anyhow::Result<Value> {
    let text: String = tx
        .txn()
        .query_one(
            "SELECT commercial_operation($1,$2,$3::text::jsonb)::text",
            &[&op, &actor, &body.to_string()],
        )
        .await?
        .get(0);
    Ok(serde_json::from_str(&text)?)
}
async fn pid(tx: &PostgresTransaction) -> i32 {
    tx.txn()
        .query_one("SELECT pg_backend_pid()", &[])
        .await
        .unwrap()
        .get(0)
}
async fn wait_for_block(db: &PostgresDatabase, waiter: i32, holder: i32) {
    for _ in 0..300 {
        let tx = db.begin_transaction().await.unwrap();
        let row=tx.txn().query_one("SELECT query,wait_event_type,wait_event,pg_blocking_pids(pid) FROM pg_stat_activity WHERE pid=$1",&[&waiter]).await.unwrap();
        let blockers: Vec<i32> = row.get(3);
        if blockers.contains(&holder) {
            println!(
                "OBSERVED_ACTUAL_WAIT waiter={waiter} holder={holder} query={} event={:?}/{:?} blockers={blockers:?}",
                row.get::<_, String>(0),
                row.get::<_, Option<String>>(1),
                row.get::<_, Option<String>>(2)
            );
            return;
        }
        drop(tx);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("no actual wait for {holder}");
}
async fn revoke(db: &PostgresDatabase, actor: Uuid, b: &Value, mode: &str) {
    let tx = db.begin_transaction().await.unwrap();
    let session: Uuid = serde_json::from_value(b["_staff_session"].clone()).unwrap();
    match mode {
        "admin" => {
            tx.txn()
                .execute("UPDATE users SET admin=false WHERE id=$1", &[&actor])
                .await
                .unwrap();
        }
        "mfa" => {
            tx.txn()
                .execute(
                    "UPDATE sessions SET mfa_verified=false WHERE id=$1",
                    &[&session],
                )
                .await
                .unwrap();
        }
        "refresh" => {
            tx.txn()
                .execute(
                    "DELETE FROM session_refresh_tokens WHERE session_id=$1",
                    &[&session],
                )
                .await
                .unwrap();
        }
        "current" => {}
        _ => panic!("unknown mode"),
    }
    tx.commit().await.unwrap();
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
        let value:String=tx.txn().query_one(&format!("SELECT md5(coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb)::text) FROM \"{quoted}\" t"),&[]).await.unwrap().get(0);
        out.insert(name, value);
    }
    out
}

#[tokio::test]
async fn retention_replay_checks_current_staff_after_actual_command_wait() {
    let db = setup().await;
    for (i, mode) in ["current", "admin", "mfa", "refresh"]
        .into_iter()
        .enumerate()
    {
        let n = format!("R1000020{i}");
        let tx = db.begin_transaction().await.unwrap();
        orphan(&tx, &n, false).await;
        tx.commit().await.unwrap();
        let (actor, b) = staff(&db, &n, "invoice").await;
        let tx = db.begin_transaction().await.unwrap();
        let original = operation(&tx, "archive_review", actor, &b).await.unwrap();
        let command: Uuid = serde_json::from_value(b["command_id"].clone()).unwrap();
        let journal: String = tx
            .txn()
            .query_one(
                "SELECT to_jsonb(j)::text FROM commercial_journal j WHERE command_id=$1",
                &[&command],
            )
            .await
            .unwrap()
            .get(0);
        tx.commit().await.unwrap();
        let holder = db.begin_transaction().await.unwrap();
        let holder_pid = pid(&holder).await;
        holder
            .txn()
            .execute(
                "SELECT pg_advisory_xact_lock(hashtextextended('commercial-command:'||$1::text,0))",
                &[&command.to_string()],
            )
            .await
            .unwrap();
        let worker = db.begin_transaction().await.unwrap();
        let worker_pid = pid(&worker).await;
        let body = b.clone();
        let run = tokio::spawn(async move {
            let r = operation(&worker, "archive_review", actor, &body).await;
            worker.rollback().await.unwrap();
            r
        });
        wait_for_block(&db, worker_pid, holder_pid).await;
        revoke(&db, actor, &b, mode).await;
        holder.commit().await.unwrap();
        let result = run.await.unwrap();
        println!("REPLAY {mode}: {result:?}");
        if mode == "current" {
            assert_eq!(result.unwrap(), original);
        } else {
            assert!(
                result.is_err(),
                "revoked proof cannot return old receipt after command wait"
            );
        }
        let tx = db.begin_transaction().await.unwrap();
        assert_eq!(
            tx.txn()
                .query_one(
                    "SELECT to_jsonb(j)::text FROM commercial_journal j WHERE command_id=$1",
                    &[&command]
                )
                .await
                .unwrap()
                .get::<_, String>(0),
            journal
        );
    }
    println!(
        "GROUP replay: observed command waits, current exact receipt and immutable journal, admin/MFA/refresh losses denied"
    );
}

#[tokio::test]
async fn archive_work_and_document_waits_observe_committed_preservation() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    users(&tx).await;
    orphan(&tx, "R10000301", false).await;
    tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document','R10000301')",&[&A]).await.unwrap();
    tx.txn().execute("INSERT INTO paypal_payments(order_id,invoice_number,user_id,request_id,snapshot,capture_id,capture,balance,withheld_balance,fulfilled_at) VALUES('wait-conflict',10000301,$1,$2,'{}','capture','{}',0,0,'2026-09-01')",&[&B,&Uuid::new_v4()]).await.unwrap();
    orphan(&tx, "R10000302", true).await;
    document(&tx, "R10000303", "invoice").await;
    tx.commit().await.unwrap();
    // Actual archive review captures a conflict while holding archive before work.
    let (actor, mut b) = staff(&db, "R10000301", "invoice").await;
    b["authorize_disposal"] = json!(true);
    b["remaining_claims_assessed"] = json!(true);
    b["document_not_necessary"] = json!(true);
    b["alternative_evidence"] =
        json!("Synthetic separately retained alternative evidence for this controlled assessment");
    let holder = db.begin_transaction().await.unwrap();
    operation(&holder, "archive_review", actor, &b)
        .await
        .unwrap();
    let hp = pid(&holder).await;
    let mut worker = db.begin_transaction().await.unwrap();
    let wp = pid(&worker).await;
    let run = tokio::spawn(async move {
        let v = Finance
            .begin_archive_disposal(
                &mut worker,
                &number("R10000301"),
                FinancialDocumentKind::Invoice,
            )
            .await
            .unwrap();
        worker.commit().await.unwrap();
        v
    });
    wait_for_block(&db, wp, hp).await;
    holder.commit().await.unwrap();
    assert!(!run.await.unwrap());
    // Actual acknowledgement is a work-row publisher without an archive lock.
    let mut holder = db.begin_transaction().await.unwrap();
    Finance
        .acknowledge_archive_disposal(
            &mut holder,
            &number("R10000302"),
            FinancialDocumentKind::Invoice,
        )
        .await
        .unwrap();
    let hp = pid(&holder).await;
    let mut worker = db.begin_transaction().await.unwrap();
    let wp = pid(&worker).await;
    let run = tokio::spawn(async move {
        let v = Finance
            .begin_archive_disposal(
                &mut worker,
                &number("R10000302"),
                FinancialDocumentKind::Invoice,
            )
            .await
            .unwrap();
        worker.commit().await.unwrap();
        v
    });
    wait_for_block(&db, wp, hp).await;
    holder.commit().await.unwrap();
    assert!(!run.await.unwrap());
    // Actual case opening publishes a hold; its FK lock delays the pruning row lock.
    let holder = db.begin_transaction().await.unwrap();
    operation(&holder, "open", A, &json!({"command_id":Uuid::new_v4()}))
        .await
        .unwrap();
    let hp = pid(&holder).await;
    let mut worker = db.begin_transaction().await.unwrap();
    let wp = pid(&worker).await;
    let run = tokio::spawn(async move {
        let v = Finance
            .delete_issued_before(&mut worker, cutoff())
            .await
            .unwrap();
        worker.commit().await.unwrap();
        v
    });
    wait_for_block(&db, wp, hp).await;
    holder.commit().await.unwrap();
    assert_eq!(run.await.unwrap(), 0);
    let tx = db.begin_transaction().await.unwrap();
    assert!(!snapshot(&tx, "R10000303").await["record"].is_null());
    println!(
        "GROUP actual archive-review, acknowledgement work and case-open/document waits: committed preservation wins"
    );
}

#[tokio::test]
async fn disposal_first_rejects_waiting_adoption_and_staff_row_waits_recheck() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    orphan(&tx, "R10000401", false).await;
    tx.commit().await.unwrap();
    let mut holder = db.begin_transaction().await.unwrap();
    assert!(
        Finance
            .begin_archive_disposal(
                &mut holder,
                &number("R10000401"),
                FinancialDocumentKind::Invoice
            )
            .await
            .unwrap()
    );
    let hp = pid(&holder).await;
    let mut worker = db.begin_transaction().await.unwrap();
    let wp = pid(&worker).await;
    let run = tokio::spawn(async move {
        let r = Finance
            .record_original_invoice(
                &mut worker,
                &number("R10000401"),
                b"cannot resurrect",
                "synthetic",
            )
            .await;
        worker.rollback().await.unwrap();
        r
    });
    wait_for_block(&db, wp, hp).await;
    holder.commit().await.unwrap();
    assert!(run.await.unwrap().is_err());
    for (i, kind) in ["archive", "work", "statement"].into_iter().enumerate() {
        let n = if kind == "statement" {
            "S10000403".to_owned()
        } else {
            format!("R1000041{i}")
        };
        let tx = db.begin_transaction().await.unwrap();
        if kind == "statement" {
            tx.txn().execute("INSERT INTO financial_documents(number,kind,issued_at) VALUES($1,'final_statement','2000-01-01')",&[&n]).await.unwrap();
        } else {
            orphan(&tx, &n, false).await;
        }
        tx.commit().await.unwrap();
        let (actor, b) = staff(
            &db,
            &n,
            if kind == "statement" {
                "final_statement"
            } else {
                "invoice"
            },
        )
        .await;
        let holder = db.begin_transaction().await.unwrap();
        let hp = pid(&holder).await;
        match kind {
            "archive" => {
                holder.txn().execute("SELECT pg_advisory_xact_lock(hashtextextended('commercial-archive:'||$1::text,0))",&[&n]).await.unwrap();
            }
            "work" => {
                holder
                    .txn()
                    .query_one(
                        "SELECT number FROM commercial_archive_work WHERE number=$1 FOR UPDATE",
                        &[&n],
                    )
                    .await
                    .unwrap();
            }
            _ => {
                holder
                    .txn()
                    .query_one(
                        "SELECT number FROM financial_documents WHERE number=$1 FOR UPDATE",
                        &[&n],
                    )
                    .await
                    .unwrap();
            }
        }
        let worker = db.begin_transaction().await.unwrap();
        let wp = pid(&worker).await;
        let body = b.clone();
        let op = if kind == "statement" {
            "statement_review"
        } else {
            "archive_review"
        };
        let run = tokio::spawn(async move {
            let r = operation(&worker, op, actor, &body).await;
            worker.rollback().await.unwrap();
            r
        });
        wait_for_block(&db, wp, hp).await;
        revoke(&db, actor, &b, "mfa").await;
        holder.commit().await.unwrap();
        assert!(run.await.unwrap().is_err());
    }
    println!(
        "GROUP reverse archive/adoption order and actual review archive/work/document waits retain current-staff gate"
    );
}

#[tokio::test]
async fn isolation_entries_and_forward_only_downgrade_preserve_exact_state() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    users(&tx).await;
    document(&tx, "R10000501", "invoice").await;
    orphan(&tx, "R10000502", false).await;
    tx.commit().await.unwrap();
    let (actor, b) = staff(&db, "R10000502", "invoice").await;
    let tx = db.begin_transaction().await.unwrap();
    operation(&tx, "archive_review", actor, &b).await.unwrap();
    tx.commit().await.unwrap();
    let old = fingerprint(&db).await;
    for isolation in ["REPEATABLE READ", "SERIALIZABLE"] {
        for entry in ["prune", "begin", "replay", "delete"] {
            let mut tx = db.begin_transaction().await.unwrap();
            tx.txn()
                .batch_execute(&format!("SET TRANSACTION ISOLATION LEVEL {isolation}"))
                .await
                .unwrap();
            let failed = match entry {
                "prune" => Finance
                    .delete_issued_before(&mut tx, cutoff())
                    .await
                    .is_err(),
                "begin" => Finance
                    .begin_archive_disposal(
                        &mut tx,
                        &number("R10000502"),
                        FinancialDocumentKind::Invoice,
                    )
                    .await
                    .is_err(),
                "replay" => operation(&tx, "archive_review", actor, &b).await.is_err(),
                _ => {
                    let e = tx
                        .txn()
                        .execute(
                            "DELETE FROM financial_documents WHERE number='R10000501'",
                            &[],
                        )
                        .await
                        .unwrap_err();
                    assert_eq!(e.code().unwrap().code(), "25001");
                    true
                }
            };
            assert!(failed, "{entry} must refuse {isolation}");
            tx.rollback().await.unwrap();
        }
    }
    assert_eq!(fingerprint(&db).await, old);
    let error = db.revert_migrations(Some(1)).await.unwrap_err();
    assert!(format!("{error:#}").contains("Wallet restoration target and isolation protection"));
    // Exercise this unit's guard as well; the newer wallet refusal is separate.
    let tx = db.begin_transaction().await.unwrap();
    let invoice = academy_persistence_postgres::MIGRATIONS
        .iter()
        .find(|m| m.name == FORWARD)
        .unwrap();
    let error = tx.txn().batch_execute(invoice.down).await.unwrap_err();
    assert_eq!(error.code().unwrap().code(), "P0001");
    assert!(
        error
            .as_db_error()
            .unwrap()
            .message()
            .contains("Invoice disposal preservation is forward-repair-only")
    );
    tx.rollback().await.unwrap();
    assert_eq!(fingerprint(&db).await, old);
    println!(
        "GROUP all four RC boundaries including exact replay; full-forward down refused, {} public-table fingerprints unchanged",
        old.len()
    );
}

#[tokio::test]
async fn additive_migration_preserves_dispatch_and_safe_existing_conditions() {
    let db = setup_mode(true).await;
    let tx = db.begin_transaction().await.unwrap();
    let before: String = tx
        .txn()
        .query_one(
            "SELECT pg_get_functiondef('commercial_operation(text,uuid,jsonb)'::regprocedure)",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    assert_eq!(
        common::apply_through(&db, Some(FORWARD)).await,
        vec![FORWARD]
    );
    let tx = db.begin_transaction().await.unwrap();
    assert_eq!(
        tx.txn()
            .query_one(
                "SELECT pg_get_functiondef('commercial_operation(text,uuid,jsonb)'::regprocedure)",
                &[]
            )
            .await
            .unwrap()
            .get::<_, String>(0),
        before
    );
    users(&tx).await;
    for (n, k) in [
        ("R10000601", "invoice"),
        ("R10000602", "invoice"),
        ("R10000603", "invoice"),
        ("G200001-1", "credit_note"),
        ("S10000601", "final_statement"),
        ("S10000602", "final_statement"),
    ] {
        document(&tx, n, k).await;
    }
    let case: Uuid = tx
        .txn()
        .query_one(
            "INSERT INTO commercial_cases(subject) VALUES($1) RETURNING id",
            &[&A],
        )
        .await
        .unwrap()
        .get(0);
    tx.txn().execute("INSERT INTO commercial_document_holds(case_id,number,basis) VALUES($1,'R10000602','unchanged original hold')",&[&case]).await.unwrap();
    tx.txn()
        .execute(
            "UPDATE financial_documents SET issued_at='2099-01-01' WHERE number='R10000603'",
            &[],
        )
        .await
        .unwrap();
    tx.txn().execute("UPDATE commercial_statement_disposal_reviews SET authorized=true,assessment='{\"existing\":true}' WHERE number='S10000602'",&[]).await.unwrap();
    assert!(tx.txn().query_one("SELECT EXISTS(SELECT 1 FROM commercial_invoice_owner_observations WHERE number='R10000601' AND qualified)",&[]).await.unwrap().get::<_,bool>(0));
    tx.commit().await.unwrap();
    let mut tx = db.begin_transaction().await.unwrap();
    assert_eq!(
        Finance
            .delete_issued_before(&mut tx, cutoff())
            .await
            .unwrap(),
        3
    );
    for n in ["R10000602", "R10000603", "S10000601"] {
        assert!(!snapshot(&tx, n).await["record"].is_null());
    }
    assert!(snapshot(&tx, "R10000601").await["record"].is_null());
    tx.commit().await.unwrap();
    println!(
        "GROUP additive immediate dispatcher unchanged; qualified invoice/noninvoice/assessed statement positive; held/future/unassessed remain"
    );
}

#[tokio::test]
async fn orphan_capture_and_current_record_authorization_are_independent_gates() {
    let db = setup().await;
    let tx = db.begin_transaction().await.unwrap();
    users(&tx).await;
    orphan(&tx, "R10000701", false).await;
    tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document','R10000701')",&[&A]).await.unwrap();
    tx.txn().execute("INSERT INTO paypal_payments(order_id,invoice_number,user_id,request_id,snapshot,capture_id,capture,balance,withheld_balance,fulfilled_at) VALUES('orphan-conflict',10000701,$1,$2,'{}','capture','{}',0,0,'2026-09-01')",&[&B,&Uuid::new_v4()]).await.unwrap();
    assert!(
        !tx.txn()
            .query_one(
                "SELECT commercial_invoice_identity_pending('R10000701')",
                &[]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    let before = snapshot(&tx, "R10000701").await;
    orphan(&tx, "R10000702", false).await;
    tx.txn()
        .execute(
            "UPDATE commercial_archive_work SET disposal_authorized=false WHERE number='R10000702'",
            &[],
        )
        .await
        .unwrap();
    orphan(&tx, "R10000703", false).await;
    tx.txn().execute("INSERT INTO financial_documents(number,kind,issued_at) VALUES('R10000703','invoice','2000-01-01')",&[]).await.unwrap();
    tx.commit().await.unwrap();
    let mut tx = db.begin_transaction().await.unwrap();
    assert!(
        !Finance
            .begin_archive_disposal(
                &mut tx,
                &number("R10000701"),
                FinancialDocumentKind::Invoice
            )
            .await
            .unwrap()
    );
    let after = snapshot(&tx, "R10000701").await;
    for key in ["record", "original", "reconciliation", "work", "retained"] {
        assert_eq!(before[key], after[key]);
    }
    assert_eq!(after["reviews"].as_array().unwrap().len(), 1);
    for n in ["R10000702", "R10000703"] {
        let old = snapshot(&tx, n).await;
        assert!(
            !Finance
                .begin_archive_disposal(&mut tx, &number(n), FinancialDocumentKind::Invoice)
                .await
                .unwrap()
        );
        assert_eq!(snapshot(&tx, n).await, old);
    }
    tx.commit().await.unwrap();
    println!(
        "GROUP orphan capture: newly discovered contradiction preserved on false; independent unauthorized/newly recorded admissions remain false"
    );
}
