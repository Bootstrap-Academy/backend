//! Metadata-only inventory on an exclusively owned synthetic SQL target.
mod common;
use academy_models::commercial_document::*;
use academy_persistence_contracts::{Database, Transaction, moderation::ModerationRepository};
use academy_persistence_postgres::{
    PostgresDatabase, PostgresDatabaseConfig, PostgresTransaction,
    moderation::PostgresModerationRepository as Repo,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
async fn setup() -> PostgresDatabase {
    let root = PathBuf::from(std::env::var("BOOTSTRAP_INVENTORY_FIXTURE").unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("bootstrap-document-inventory-")
    );
    let marker: Value =
        serde_json::from_slice(&std::fs::read(root.join("OWNER.json")).unwrap()).unwrap();
    assert_eq!(marker["canonical_root"], root.to_str().unwrap());
    assert_eq!(
        marker["unit"],
        "L3-original-document-metadata-inventory-backend-1"
    );
    assert_eq!(marker["owner"], "/root/learning_evidence_review");
    assert_eq!(
        std::env::var("ACADEMY_CONFIG").unwrap(),
        root.join("fixture.toml").to_str().unwrap()
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
    assert!(
        parsed.get_hostaddrs().is_empty()
            && parsed.get_options().is_none()
            && parsed.get_password().is_none()
    );
    let port = u16::try_from(marker["port"].as_u64().unwrap()).unwrap();
    assert_eq!(port, 56910);
    assert_eq!(parsed.get_ports(), &[port]);
    assert_eq!(parsed.get_dbname(), marker["database"].as_str());
    assert_eq!(parsed.get_user(), marker["role"].as_str());
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
    let row=txn.txn().query_one("SELECT current_database()::text,current_user::text,inet_server_port(),current_setting('data_directory'),version()",&[]).await.unwrap();
    assert_eq!(row.get::<_, &str>(0), marker["database"].as_str().unwrap());
    assert_eq!(row.get::<_, &str>(1), marker["role"].as_str().unwrap());
    assert_eq!(row.get::<_, i32>(2), i32::from(port));
    assert_eq!(
        PathBuf::from(row.get::<_, &str>(3)).canonicalize().unwrap(),
        root.join("pgdata").canonicalize().unwrap()
    );
    assert!(row.get::<_, &str>(4).starts_with("PostgreSQL 18.6"));
    println!(
        "OWNED TARGET VERIFIED BEFORE common::setup RESET: {} / {} / {} / {}",
        row.get::<_, &str>(0),
        row.get::<_, &str>(1),
        port,
        row.get::<_, &str>(3)
    );
    txn.commit().await.unwrap();
    drop(db);
    common::setup().await
}

async fn offer(
    tx: &PostgresTransaction,
    owner: Uuid,
    source: &str,
    progress: bool,
    mismatch: bool,
) -> Uuid {
    let id = Uuid::new_v4();
    let original=json!({"id":id,"user_id":if mismatch {Uuid::new_v4()} else {owner},"private_text":"must not leave SQL"}).to_string();
    tx.txn().execute("INSERT INTO purchase_offers(id,user_id,source,offer,terms_pdf,withdrawal_pdf,created_at,expires_at) VALUES($1,$2,$3,$4::text::jsonb,$5,$6,'2026-09-01','2026-09-02')",&[&id,&owner,&source,&original,&b"private terms".to_vec(),&Vec::<u8>::new()]).await.unwrap();
    if progress {
        tx.txn()
            .execute("INSERT INTO purchase_progress(order_id) VALUES($1)", &[&id])
            .await
            .unwrap();
    }
    id
}
async fn document(tx: &PostgresTransaction, n: &str, kind: &str, owner: Option<Uuid>) {
    tx.txn().execute("INSERT INTO financial_documents(number,kind,user_id,issued_at,customer_details) VALUES($1,$2,$3,'2000-01-01',ARRAY['private synthetic details'])",&[&n,&kind,&owner]).await.unwrap();
}
async fn payment(tx: &PostgresTransaction, n: i64, owner: Uuid, fulfilled: bool) {
    let id = format!("inventory-{n}");
    tx.txn().execute("INSERT INTO paypal_payments(order_id,invoice_number,user_id,request_id,snapshot,capture_id,capture,balance,withheld_balance,fulfilled_at) VALUES($1,$2,$3,$4,'private synthetic payment',CASE WHEN $5 THEN $1 END,CASE WHEN $5 THEN '{}' END,CASE WHEN $5 THEN 0 END,CASE WHEN $5 THEN 0 END,CASE WHEN $5 THEN '2026-09-01'::timestamptz END)",&[&id,&n,&owner,&Uuid::new_v4(),&fulfilled]).await.unwrap();
}
async fn original(tx: &PostgresTransaction, n: &str, bytes: &[u8]) {
    tx.txn().execute("INSERT INTO invoice_originals(invoice_number,pdf,provenance) VALUES($1,$2,'synthetic')",&[&n,&bytes]).await.unwrap();
}
async fn read(db: &PostgresDatabase, owner: Uuid) -> DocumentInventory {
    let mut tx = db.begin_transaction().await.unwrap();
    let result = Repo
        .commercial_document_inventory(&mut tx, owner.into())
        .await
        .unwrap();
    let settings=tx.txn().query_one("SELECT current_setting('transaction_isolation'),current_setting('transaction_read_only')",&[]).await.unwrap();
    assert_eq!(settings.get::<_, &str>(0), "repeatable read");
    assert_eq!(settings.get::<_, &str>(1), "on");
    tx.commit().await.unwrap();
    result
}
fn finance<'a>(i: &'a DocumentInventory, n: &str) -> &'a DocumentRecord {
    i.records
        .iter()
        .find(|r| r.printed_number.as_deref() == Some(n))
        .unwrap()
}
fn purchase(i: &DocumentInventory, id: Uuid) -> &DocumentRecord {
    i.records.iter().find(|r| r.offer_id == Some(id)).unwrap()
}
fn artifact<'a>(r: &'a DocumentRecord, v: &str) -> &'a DocumentArtifact {
    r.artifacts.iter().find(|a| a.variant == v).unwrap()
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

#[tokio::test]
async fn inventory_owned_metadata_and_stable_read_only_snapshot() {
    let db = setup().await;
    let a = *academy_demo::user::FOO.user.id;
    let b = *academy_demo::user::BAR.user.id;
    let s1 = Uuid::new_v4();
    let s2 = Uuid::new_v4();
    let c = Uuid::new_v4();
    let tx = db.begin_transaction().await.unwrap();
    let ordinary = offer(&tx, a, "backend", true, false).await;
    let foreign = offer(&tx, b, "events", true, false).await;
    tx.commit().await.unwrap();
    let first = read(&db, a).await;
    assert!(first.scope.historical_owner_inventory_complete);
    assert_eq!(
        first
            .records
            .iter()
            .filter(|r| r.offer_id.is_some())
            .count(),
        1
    );
    assert_eq!(purchase(&first, ordinary).artifacts.len(), 7);
    println!(
        "GROUP1 no-case live personal purchase inventory, foreign offer absent, all seven artifacts"
    );

    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .execute(
            "INSERT INTO commercial_cases(id,subject) VALUES($1,$2)",
            &[&c, &a],
        )
        .await
        .unwrap();
    for (subject, erased) in [(s1, true), (s2, false)] {
        tx.txn().execute("INSERT INTO commercial_learning_subjects(subject,case_id,election_id,election,erased_at) VALUES($1,$2,$3,'{}',CASE WHEN $4 THEN clock_timestamp() END)",&[&subject,&c,&Uuid::new_v4(),&erased]).await.unwrap();
    }
    let old = offer(&tx, s1, "skills", true, false).await;
    let current = offer(&tx, s2, "paypal", true, false).await;
    let missing = offer(&tx, s1, "events", false, false).await;
    let mismatch = offer(&tx, s2, "backend", true, true).await;
    tx.txn().execute("INSERT INTO purchase_acceptances(order_id,confirmation_body,message_metadata) VALUES($1,'','{}')",&[&ordinary]).await.unwrap();
    // Version1 original stays historical; even an empty current correction wins.
    for (id, version) in [(old, 1i32), (current, 2), (ordinary, 1)] {
        tx.txn().execute("INSERT INTO purchase_provision_observations(order_id,observed_at,evidence,statement,statement_version) VALUES($1,clock_timestamp(),'{}','original timing',$2)",&[&id,&version]).await.unwrap();
        tx.txn().execute("INSERT INTO purchase_fulfillments(order_id,result,statement,statement_version) VALUES($1,'{}','original fulfillment',$2)",&[&id,&version]).await.unwrap();
    }
    // Existing insertion triggers may create corrections: preserve them, choose
    // a fresh parent whose version2 does not create a legacy correction below.
    // Synthetic missing-correction control: keep both parent originals and
    // remove only this fixture's automatically generated correction rows.
    assert_eq!(
        tx.txn()
            .execute(
                "DELETE FROM purchase_document_corrections WHERE order_id=$1",
                &[&ordinary]
            )
            .await
            .unwrap(),
        2
    );
    let empty = offer(&tx, s1, "skills", true, false).await;
    tx.txn().execute("INSERT INTO purchase_provision_observations(order_id,observed_at,evidence,statement,statement_version) VALUES($1,clock_timestamp(),'{}','nonempty original',2)",&[&empty]).await.unwrap();
    tx.txn().execute("INSERT INTO purchase_document_corrections(order_id,document_kind,version,original_sha256,created_at,evidence,statement,statement_sha256) VALUES($1,'timing',1,'synthetic',clock_timestamp(),'{}','',encode(sha256(''::bytea),'hex'))",&[&empty]).await.unwrap();
    tx.txn().execute("INSERT INTO purchase_fulfillments(order_id,result,statement,statement_version) VALUES($1,'{}','nonempty fulfillment',2)", &[&empty]).await.unwrap();
    tx.txn().execute("INSERT INTO purchase_document_corrections(order_id,document_kind,version,original_sha256,created_at,evidence,statement,statement_sha256) VALUES($1,'fulfillment',1,'synthetic',clock_timestamp(),'{}','',encode(sha256(''::bytea),'hex'))", &[&empty]).await.unwrap();
    let orphan = offer(&tx, s2, "events", true, false).await;
    tx.txn().execute("INSERT INTO purchase_document_corrections(order_id,document_kind,version,original_sha256,created_at,evidence,statement,statement_sha256) VALUES($1,'timing',1,'synthetic',clock_timestamp(),'{}','orphan',encode(sha256('orphan'::bytea),'hex'))",&[&orphan]).await.unwrap();
    tx.commit().await.unwrap();
    let i = read(&db, a).await;
    assert!(!i.records.iter().any(|r| r.offer_id == Some(foreign)));
    assert_eq!(purchase(&i, old).source_subject, s1);
    assert_eq!(purchase(&i, current).source_subject, s2);
    assert_eq!(
        purchase(&i, old).owner_relation,
        OwnerRelation::SameCaseLearningSubject
    );
    assert_eq!(
        purchase(&i, missing).reason,
        Some(UnavailableReason::MissingProgress)
    );
    assert_eq!(
        purchase(&i, mismatch).reason,
        Some(UnavailableReason::OriginalIdentityMismatch)
    );
    assert!(
        purchase(&i, mismatch)
            .artifacts
            .iter()
            .all(|a| a.selector.is_none())
    );
    assert_eq!(
        artifact(purchase(&i, empty), "timing").selection_source,
        SelectionSource::Correction
    );
    assert_eq!(
        artifact(purchase(&i, empty), "timing").observation,
        ArtifactObservation::Empty
    );
    assert!(artifact(purchase(&i, empty), "timing").selector.is_none());
    assert_eq!(
        artifact(purchase(&i, empty), "timing-original").observation,
        ArtifactObservation::Nonempty
    );
    assert_eq!(
        artifact(purchase(&i, orphan), "timing").observation,
        ArtifactObservation::Absent
    );
    assert_eq!(
        artifact(purchase(&i, current), "fulfillment").selection_source,
        SelectionSource::OriginalV2
    );
    assert_eq!(
        artifact(purchase(&i, ordinary), "confirmation").observation,
        ArtifactObservation::Empty
    );
    for variant in ["timing", "fulfillment"] {
        assert_eq!(
            artifact(purchase(&i, ordinary), variant).observation,
            ArtifactObservation::Absent
        );
        assert_eq!(
            artifact(purchase(&i, old), variant).selection_source,
            SelectionSource::Correction
        );
        assert_eq!(
            artifact(purchase(&i, empty), variant).observation,
            ArtifactObservation::Empty
        );
        assert!(artifact(purchase(&i, empty), variant).selector.is_none());
    }
    assert!(
        purchase(&i, missing)
            .artifacts
            .iter()
            .all(|a| a.selector.is_none())
    );
    println!(
        "GROUP2 durable O/S1/S2 ownership; missing progress/identity mismatch; original/current/empty/orphan artifact selection"
    );

    let tx = db.begin_transaction().await.unwrap();
    payment(&tx, 10_000_000, a, true).await;
    document(&tx, "R10000000", "invoice", Some(a)).await;
    original(&tx, "R10000000", b"synthetic full ID original").await;
    payment(&tx, 1_000_000, b, true).await;
    document(&tx, "R1000000", "invoice", Some(b)).await;
    original(&tx, "R1000000", b"private B original").await;
    payment(&tx, 7_000_000, a, true).await;
    document(&tx, "R7000000", "invoice", Some(b)).await;
    original(&tx, "R7000000", b"private ambiguous original").await;
    tx.txn().execute("INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence) VALUES('R7000000','numeric_recorded_owner_conflict','synthetic','{\"private_conflict\":true}')",&[]).await.unwrap();
    payment(&tx, 7_000_001, a, false).await;
    document(&tx, "R7000001", "invoice", Some(a)).await;
    payment(&tx, 7_000_002, b, false).await;
    document(&tx, "R7000002", "invoice", Some(a)).await;
    document(&tx, "R4000000", "invoice", None).await;
    tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document','R4000000')",&[&a]).await.unwrap();
    original(&tx, "R4000000", b"genuine preserved").await;
    document(&tx, "R0000003", "invoice", Some(a)).await;
    original(&tx, "R0000003", b"").await;
    document(&tx, "R0000004", "invoice", Some(a)).await;
    original(&tx, "R0000004", b"present behind fence").await;
    tx.txn().execute("INSERT INTO commercial_archive_work(number,kind,source) VALUES('R0000004','credit_note','record_disposal')",&[]).await.unwrap();
    document(&tx, "R01", "invoice", Some(a)).await;
    document(&tx, "R9223372036854775808", "invoice", Some(a)).await;
    document(&tx, "S18446744073709551615", "final_statement", Some(a)).await;
    document(&tx, "S18446744073709551616", "final_statement", Some(a)).await;
    document(&tx, "S99", "final_statement", None).await;
    tx.txn().execute("INSERT INTO moderation_retained_record_owners(subject,kind,record_id) VALUES($1,'financial_document','S99')",&[&s1]).await.unwrap();
    original(&tx, "R8888888", b"ownerless original").await;
    original(&tx, "R7999998", b"unadmitted bytes must not be observed").await;
    original(
        &tx,
        "R7999999",
        b"unsupported generic association must not enumerate",
    )
    .await;
    tx.txn().execute("INSERT INTO commercial_retention_owners(number,kind,subject,source) VALUES('R7999999','invoice',$1,'unsupported_old')",&[&a]).await.unwrap();
    tx.txn().execute("INSERT INTO commercial_invoice_owner_observations(number,subject,basis,evidence_hash,evidence,qualified) VALUES('R7999998',$1,'synthetic_qualified','synthetic','{}',true)",&[&a]).await.unwrap();
    tx.commit().await.unwrap();
    let i = read(&db, a).await;
    let other = read(&db, b).await;
    assert_eq!(
        finance(&i, "R10000000").selector.as_ref().unwrap().id,
        "10000000"
    );
    for n in ["R1000000", "R7000002", "R8888888", "R7999999", "S99"] {
        assert!(
            !i.records
                .iter()
                .any(|r| r.printed_number.as_deref() == Some(n)),
            "unexpected {n}"
        );
    }
    assert!(
        !other
            .records
            .iter()
            .any(|r| r.printed_number.as_deref() == Some("R7000000")),
        "lower document owner must not enumerate a higher-denied invoice"
    );
    assert_eq!(
        finance(&i, "R7000000").reason,
        Some(UnavailableReason::IdentityPendingReview)
    );
    assert_eq!(
        finance(&i, "R7000000").artifacts[0].observation,
        ArtifactObservation::Unchecked
    );
    assert_eq!(
        finance(&i, "R7000001").record_basis,
        RecordBasis::OwnInvoiceReference
    );
    assert_eq!(
        finance(&i, "R7000001").reason,
        Some(UnavailableReason::OriginalReaderNotAdmitted)
    );
    assert_eq!(finance(&i, "R4000000").reader_state, ReaderState::Candidate);
    assert_eq!(
        finance(&i, "R0000003").artifacts[0].selection_source,
        SelectionSource::DatabaseOriginal
    );
    assert_eq!(
        finance(&i, "R0000003").reason,
        Some(UnavailableReason::EmptySelectedArtifact)
    );
    assert_eq!(
        finance(&i, "R0000004").reason,
        Some(UnavailableReason::RetiredRecorded)
    );
    for n in ["R01", "R9223372036854775808", "S18446744073709551616"] {
        assert_eq!(
            finance(&i, n).reason,
            Some(UnavailableReason::UnsupportedIdentifier)
        );
    }
    assert_eq!(
        finance(&i, "S18446744073709551615")
            .selector
            .as_ref()
            .unwrap()
            .id,
        "18446744073709551615"
    );
    assert_eq!(
        finance(&i, "R7999998").record_basis,
        RecordBasis::QualifiedRetentionReference
    );
    assert_eq!(
        finance(&i, "R7999998").reason,
        Some(UnavailableReason::OriginalReaderNotAdmitted)
    );
    assert_eq!(
        finance(&i, "R7999998").artifacts[0].observation,
        ArtifactObservation::Unchecked
    );
    assert!(finance(&i, "R7999998").selector.is_none());
    println!(
        "GROUP3 exact large IDs; IF1 minimal pending/denied owner/qualified metadata-only; finance O-only; archive fence and empty DB original"
    );

    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO user_numbers(user_id,number) VALUES($1,1234567) ON CONFLICT(user_id) DO NOTHING",&[&a]).await.unwrap();
    let customer: i64 = tx
        .txn()
        .query_one("SELECT number FROM user_numbers WHERE user_id=$1", &[&a])
        .await
        .unwrap()
        .get(0);
    let wrong = customer + 10000;
    let ambiguous = format!("G202601-{wrong}");
    document(&tx, &ambiguous, "credit_note", Some(a)).await;
    let blocked = format!("G202602-{customer}");
    document(&tx, &blocked, "invoice", Some(b)).await;
    let unique = format!("G202602-{wrong}");
    document(&tx, &unique, "credit_note", Some(a)).await;
    let large = format!("G1000001-{customer}");
    document(&tx, &large, "credit_note", Some(a)).await;
    tx.commit().await.unwrap();
    let before = fingerprint(&db).await;
    let i = read(&db, a).await;
    assert_eq!(before, fingerprint(&db).await);
    assert_eq!(
        finance(&i, &ambiguous).reason,
        Some(UnavailableReason::AmbiguousPeriod)
    );
    assert_eq!(finance(&i, &unique).selector.as_ref().unwrap().variant, "2");
    assert_eq!(finance(&i, &large).selector.as_ref().unwrap().id, "10000");
    let raw = serde_json::to_string(&i).unwrap();
    println!("ACTUAL METADATA RESPONSE {raw}");
    println!(
        "UNCHANGED TABLE FINGERPRINTS {}",
        serde_json::to_string(&before).unwrap()
    );
    for private in [
        "private",
        "base64",
        "customer_details",
        "evidence_hash",
        "assessment",
        "confirmation_body",
        "terms_pdf",
    ] {
        assert!(!raw.contains(private), "metadata leaked {private}");
    }
    assert!(
        !i.scope.catalog_complete
            && !i.scope.archives_scanned
            && !i.scope.remote_sources_queried
            && i.scope.known_local_enumeration_complete
    );
    let erased = Uuid::new_v4();
    assert!(
        !read(&db, erased)
            .await
            .scope
            .historical_owner_inventory_complete
    );
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO moderation_erasure_events(subject,retained_owner_inventory) VALUES($1,true)",&[&erased]).await.unwrap();
    tx.commit().await.unwrap();
    assert!(
        read(&db, erased)
            .await
            .scope
            .historical_owner_inventory_complete
    );
    println!(
        "GROUP4 credit live fallback ambiguity/different-kind occupant/large year; exact full-table fingerprint unchanged; explicit incomplete scope"
    );

    let holder = db.begin_transaction().await.unwrap();
    holder
        .txn()
        .batch_execute("LOCK TABLE financial_documents IN ACCESS EXCLUSIVE MODE")
        .await
        .unwrap();
    let late_offer = offer(&holder, a, "backend", true, false).await;
    document(&holder, "S987654321", "final_statement", Some(a)).await;
    let reader_db = db.clone();
    let task = tokio::spawn(async move { read(&reader_db, a).await });
    let observer = db.begin_transaction().await.unwrap();
    let mut waited = false;
    for _ in 0..100 {
        observer
            .txn()
            .batch_execute("SELECT pg_stat_clear_snapshot()")
            .await
            .unwrap();
        let rows=observer.txn().query("SELECT pid,wait_event_type,wait_event,pg_blocking_pids(pid)::text FROM pg_stat_activity WHERE datname=current_database() AND query LIKE 'WITH candidates AS (%' AND wait_event_type='Lock'",&[]).await.unwrap();
        if let Some(row) = rows.first() {
            println!(
                "SNAPSHOT WAIT pid={} type={} event={} blockers={}",
                row.get::<_, i32>(0),
                row.get::<_, &str>(1),
                row.get::<_, &str>(2),
                row.get::<_, &str>(3)
            );
            waited = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        waited,
        "inventory did not reach actual held finance projection"
    );
    observer.commit().await.unwrap();
    holder.commit().await.unwrap();
    let earlier = task.await.unwrap();
    assert!(!earlier.records.iter().any(
        |r| r.offer_id == Some(late_offer) || r.printed_number.as_deref() == Some("S987654321")
    ));
    let later = read(&db, a).await;
    purchase(&later, late_offer);
    finance(&later, "S987654321");
    let mut tx = db.begin_transaction().await.unwrap();
    Repo.commercial_document_inventory(&mut tx, a.into())
        .await
        .unwrap();
    let e = tx
        .txn()
        .execute("UPDATE purchase_progress SET state=state WHERE false", &[])
        .await
        .unwrap_err();
    assert_eq!(e.code().unwrap().code(), "25006");
    drop(tx);
    println!(
        "GROUP5 actual inter-query wait/commit preserves old coherent snapshot, next read sees both new rows; read-only write refusal25006"
    );
}
