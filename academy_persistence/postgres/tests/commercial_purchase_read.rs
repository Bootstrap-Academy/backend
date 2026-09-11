//! Actual repository/SQL controls in a newly owned, identity-checked disposable fixture.
//! Synthetic rows exercise original ownership and readback, not provider/payment behavior.
mod common;
use academy_models::{
    purchase::{PurchaseOffer, PurchaseStatus},
    user::UserId,
};
use academy_persistence_contracts::{
    Database, Transaction,
    moderation::{CommercialPurchaseReadError, ModerationRepository},
    purchase::PurchaseRepository,
};
use academy_persistence_postgres::{
    PostgresDatabase, PostgresDatabaseConfig, PostgresTransaction,
    moderation::PostgresModerationRepository as Repo,
    purchase::PostgresPurchaseRepository as Purchase,
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use uuid::Uuid;

async fn setup() -> PostgresDatabase {
    let root = PathBuf::from(std::env::var("BOOTSTRAP_PERSONAL_PURCHASE_FIXTURE").unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("bootstrap-personal-purchase-read-")
    );
    let marker: Value =
        serde_json::from_slice(&std::fs::read(root.join("OWNER.json")).unwrap()).unwrap();
    assert_eq!(marker["root"], root.to_str().unwrap());
    assert_eq!(marker["unit"], "L3-personal-purchase-read-backend-1");
    assert_eq!(marker["owner"], "/root/learning_source_review");
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
    assert_eq!(port, 56880);
    assert_eq!(parsed.get_ports(), &[port]);
    assert_eq!(parsed.get_dbname(), marker["database"].as_str());
    assert_eq!(parsed.get_user(), marker["user"].as_str());
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
    assert_eq!(row.get::<_, &str>(1), marker["user"].as_str().unwrap());
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
fn at() -> DateTime<Utc> {
    "2026-09-01T00:00:00Z".parse().unwrap()
}
fn offer(owner: Uuid, source: &str, coins: u64) -> PurchaseOffer {
    serde_json::from_value(json!({"id":Uuid::new_v4(),"user_id":owner,"source":source,
        "created_at":"2026-09-01T00:00:00Z","expires_at":"2026-09-01T00:10:00Z",
        "recipient":"original@example.invalid","product":{"kind":"course","reference":"original","title":"Original","description":"Original",
            "coins":coins,"facts":{"unchanged":true},"revision":"r1","service_starts_at":null},
        "document_hash":"original-document","hash":"original-offer","text":"original","declaration":"original"})).unwrap()
}
async fn insert(
    tx: &mut PostgresTransaction,
    original: &PurchaseOffer,
    raw: &Value,
    state: Option<&str>,
) {
    tx.txn().execute("INSERT INTO purchase_offers(id,user_id,source,offer,terms_pdf,withdrawal_pdf,created_at,expires_at) VALUES($1,$2,$3,$4::text::jsonb,$5,$6,$7,$8)",
        &[&original.id,&original.user_id,&original.source,&raw.to_string(),&b"terms original".to_vec(),&b"withdrawal original".to_vec(),&original.created_at,&original.expires_at]).await.unwrap();
    if let Some(state) = state {
        tx.txn()
            .execute(
                "INSERT INTO purchase_progress(order_id,state) VALUES($1,$2)",
                &[&original.id, &state],
            )
            .await
            .unwrap();
    }
}
async fn ordinary_offer(
    tx: &mut PostgresTransaction,
    owner: Uuid,
    source: &str,
    coins: u64,
    state: Option<&str>,
) -> PurchaseOffer {
    let o = offer(owner, source, coins);
    insert(tx, &o, &serde_json::to_value(&o).unwrap(), state).await;
    o
}
async fn accept(tx: &mut PostgresTransaction, id: Uuid) {
    tx.txn().execute("INSERT INTO purchase_acceptances(order_id,accepted_at,confirmation_body,message_metadata) VALUES($1,$2,'original confirmation','{}')",&[&id,&at()]).await.unwrap();
}
async fn read(
    db: &PostgresDatabase,
    claimant: Uuid,
    id: Uuid,
) -> Result<PurchaseStatus, CommercialPurchaseReadError> {
    let mut tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    let result = Repo
        .commercial_purchase_status(&mut tx, claimant.into(), id)
        .await;
    tx.commit().await.unwrap();
    result
}
async fn fingerprint(db: &PostgresDatabase) -> Value {
    let tx = db.begin_transaction().await.unwrap();
    let mut values = serde_json::Map::new();
    for table in [
        "purchase_offers",
        "purchase_progress",
        "purchase_acceptances",
        "purchase_fulfillments",
        "purchase_debits",
        "purchase_cash_captures",
        "purchase_provision_observations",
        "purchase_document_corrections",
        "purchase_delivery_attempts",
        "commercial_learning_subjects",
        "commercial_cases",
        "commercial_purchase_authorizations",
        "commercial_journal",
        "commercial_evidence",
        "users",
        "coins",
        "transactions",
    ] {
        let text:String=tx.txn().query_one(&format!("SELECT coalesce(jsonb_agg(to_jsonb(x) ORDER BY to_jsonb(x)::text),'[]')::text FROM {table} x"),&[]).await.unwrap().get(0);
        values.insert(table.into(), serde_json::from_str(&text).unwrap());
    }
    tx.commit().await.unwrap();
    Value::Object(values)
}
async fn correction(tx: &mut PostgresTransaction, id: Uuid, kind: &str, statement: &str) {
    tx.txn().execute("INSERT INTO purchase_document_corrections(order_id,document_kind,version,original_sha256,created_at,evidence,statement,statement_sha256) VALUES($1,$2,1,'fixture-original',$3,'{}',$4,encode(sha256(convert_to($4,'UTF8')),'hex'))",
        &[&id,&kind,&at(),&statement]).await.unwrap();
}
async fn parent(tx: &mut PostgresTransaction, id: Uuid, kind: &str, statement: &str, version: i32) {
    if kind == "timing" {
        tx.txn().execute("INSERT INTO purchase_provision_observations(order_id,observed_at,evidence,statement,statement_version) VALUES($1,$2,'{\"original_observation\":true}',$3,$4)",&[&id,&at(),&statement,&version]).await.unwrap();
    } else {
        tx.txn().execute("INSERT INTO purchase_fulfillments(order_id,result,statement,statement_version) VALUES($1,'{\"stored_fulfillment\":true}',$2,$3)",&[&id,&statement,&version]).await.unwrap();
    }
}

#[tokio::test]
async fn commercial_original_purchase_repository_controls() {
    let db = setup().await;
    let claimant = Uuid::new_v4();
    let unrelated = Uuid::new_v4();
    let s1 = Uuid::new_v4();
    let s2 = Uuid::new_v4();
    let mut tx = db.begin_transaction().await.unwrap();
    let own = ordinary_offer(&mut tx, claimant, "backend", 17, Some("offered")).await;
    tx.commit().await.unwrap();
    assert_eq!(
        read(&db, claimant, own.id).await.unwrap().offer.user_id,
        claimant
    );
    let mut tx = db.begin_transaction().await.unwrap();
    assert!(
        !tx.txn()
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM commercial_cases WHERE subject=$1)",
                &[&claimant]
            )
            .await
            .unwrap()
            .get::<_, bool>(0)
    );
    let case: Uuid = tx
        .txn()
        .query_one(
            "INSERT INTO commercial_cases(subject) VALUES($1) RETURNING id",
            &[&claimant],
        )
        .await
        .unwrap()
        .get(0);
    tx.txn().execute("INSERT INTO commercial_learning_subjects(subject,case_id,election_id,election,erased_at) VALUES($1,$3,$4,'{}',$6),($2,$3,$5,'{}',NULL)",
        &[&s1,&s2,&case,&Uuid::new_v4(),&Uuid::new_v4(),&at()]).await.unwrap();
    let old = ordinary_offer(&mut tx, s1, "skills", 17, Some("review")).await;
    let current = ordinary_offer(&mut tx, s2, "events", 23, Some("offered")).await;
    let foreign = ordinary_offer(&mut tx, unrelated, "paypal", 31, Some("paid")).await;
    let missing_progress = ordinary_offer(&mut tx, s1, "skills", 11, None).await;
    let mut malformed = offer(s1, "skills", 19);
    let mut raw = serde_json::to_value(&malformed).unwrap();
    raw["user_id"] = json!(s2);
    insert(&mut tx, &malformed, &raw, Some("offered")).await;
    let wrong_owner = malformed.id;
    malformed.id = Uuid::new_v4();
    raw = serde_json::to_value(&malformed).unwrap();
    raw["id"] = json!(Uuid::new_v4());
    insert(&mut tx, &malformed, &raw, Some("offered")).await;
    let wrong_id = malformed.id;
    malformed.id = Uuid::new_v4();
    raw = serde_json::to_value(&malformed).unwrap();
    raw["product"].as_object_mut().unwrap().remove("title");
    insert(&mut tx, &malformed, &raw, Some("offered")).await;
    let malformed_status = malformed.id;
    tx.commit().await.unwrap();
    let before = fingerprint(&db).await;
    assert_eq!(read(&db, claimant, old.id).await.unwrap().offer.user_id, s1);
    assert_eq!(
        read(&db, claimant, current.id).await.unwrap().offer.user_id,
        s2
    );
    assert_eq!(read(&db, claimant, old.id).await.unwrap().offer, old);
    for (owner, id) in [
        (claimant, foreign.id),
        (unrelated, old.id),
        (claimant, Uuid::new_v4()),
        (s2, old.id),
    ] {
        assert!(matches!(
            read(&db, owner, id).await,
            Err(CommercialPurchaseReadError::NotFound)
        ));
    }
    for id in [missing_progress.id, wrong_owner, wrong_id, malformed_status] {
        assert!(matches!(
            read(&db, claimant, id).await,
            Err(CommercialPurchaseReadError::Unavailable)
        ));
    }
    assert_eq!(before, fingerprint(&db).await);
    println!(
        "PASS: O without case; original erased S1 and current S2 exact durable ownership; absent/foreign404 and known malformed/missing-progress503; repeated read-only calls leave17 tables unchanged"
    );

    let mut tx = db.begin_transaction().await.unwrap();
    let mut states = Vec::new();
    for state in [
        "offered",
        "accepted",
        "awaiting_payment",
        "paid",
        "fulfilled",
        "failed",
        "review",
    ] {
        let o = ordinary_offer(&mut tx, s1, "skills", 17, Some(state)).await;
        states.push((o, state));
    }
    let unaccepted_zero = ordinary_offer(&mut tx, claimant, "backend", 0, Some("paid")).await;
    let accepted_zero = ordinary_offer(&mut tx, claimant, "backend", 0, Some("accepted")).await;
    accept(&mut tx, accepted_zero.id).await;
    let free = ordinary_offer(&mut tx, claimant, "backend", 0, Some("paid")).await;
    accept(&mut tx, free.id).await;
    let debited = ordinary_offer(&mut tx, claimant, "skills", 17, Some("paid")).await;
    accept(&mut tx, debited.id).await;
    let ledger = Uuid::new_v4();
    tx.txn().execute("INSERT INTO purchase_debits(order_id,ledger,tender_observations) VALUES($1,$2::text::jsonb,'{}')",
        &[&debited.id,&json!({"id":ledger,"coins":-17}).to_string()]).await.unwrap();
    let cash = ordinary_offer(&mut tx, claimant, "paypal", 17, Some("paid")).await;
    accept(&mut tx, cash.id).await;
    tx.txn().execute("INSERT INTO purchase_debits(order_id,ledger,tender_observations) VALUES($1,$2::text::jsonb,'{}')",
        &[&cash.id,&json!({"id":Uuid::new_v4(),"coins":-17}).to_string()]).await.unwrap();
    let cash_evidence = json!({"captured":true,"original_cash":17});
    tx.txn()
        .execute(
            "INSERT INTO purchase_cash_captures(order_id,evidence) VALUES($1,$2::text::jsonb)",
            &[&cash.id, &cash_evidence.to_string()],
        )
        .await
        .unwrap();
    // Financial observation fixtures are O-owned; creating a new S acceptance
    // would require the separate live purchase-authorization workflow.
    let mut timed = offer(claimant, "events", 23);
    timed.provision_window_seconds = Some(600);
    timed.product.service_starts_at = Some(at() + chrono::TimeDelta::seconds(300));
    insert(
        &mut tx,
        &timed,
        &serde_json::to_value(&timed).unwrap(),
        Some("review"),
    )
    .await;
    accept(&mut tx, timed.id).await;
    tx.txn().execute("UPDATE purchase_progress SET smtp_accepted_at=$2,review_reason='Original review' WHERE order_id=$1",&[&timed.id,&at()]).await.unwrap();
    parent(&mut tx, timed.id, "timing", "timing v2 original", 2).await;
    parent(
        &mut tx,
        timed.id,
        "fulfillment",
        "fulfillment v2 original",
        2,
    )
    .await;
    correction(&mut tx, timed.id, "timing", "").await;
    correction(&mut tx, timed.id, "fulfillment", "").await;
    tx.commit().await.unwrap();
    let before = fingerprint(&db).await;
    for (o, state) in &states {
        let got = read(&db, claimant, o.id).await.unwrap();
        assert_eq!(&got.offer, o);
        assert_eq!(got.state, *state);
        assert!(
            got.accepted_at.is_none()
                && got.confirmation_smtp_accepted_at.is_none()
                && got.financial_evidence.is_none()
                && got.fulfillment.is_none()
                && got.provision_timing.is_none()
                && got.provision_deadline.is_none()
        );
    }
    for id in [unaccepted_zero.id, accepted_zero.id] {
        assert!(
            read(&db, claimant, id)
                .await
                .unwrap()
                .financial_evidence
                .is_none()
        );
    }
    assert_eq!(
        read(&db, claimant, free.id)
            .await
            .unwrap()
            .financial_evidence,
        Some(json!({"charged_coins":0,"ledger_id":null,"no_charge":true}))
    );
    assert_eq!(
        read(&db, claimant, debited.id)
            .await
            .unwrap()
            .financial_evidence,
        Some(json!({"charged_coins":17,"ledger_id":ledger,"no_charge":false}))
    );
    assert_eq!(
        read(&db, claimant, cash.id)
            .await
            .unwrap()
            .financial_evidence,
        Some(cash_evidence)
    );
    let got = read(&db, claimant, timed.id).await.unwrap();
    assert_eq!(got.offer, timed);
    assert_eq!(got.accepted_at, Some(at()));
    assert_eq!(got.confirmation_smtp_accepted_at, Some(at()));
    assert_eq!(
        got.provision_deadline,
        Some(at() + chrono::TimeDelta::seconds(300))
    );
    assert_eq!(got.document_corrections, vec!["fulfillment", "timing"]);
    assert_eq!(got.fulfillment, Some(json!({"stored_fulfillment":true})));
    assert_eq!(
        got.provision_timing,
        Some(json!({"original_observation":true}))
    );
    assert_eq!(got.review_reason.as_deref(), Some("Original review"));
    assert_eq!(before, fingerprint(&db).await);
    println!(
        "PASS: all7 stored states and null outcomes; cash > debit > proved-zero precedence; original deadline/SMTP/fulfillment/timing/correction-kind observations; no selected document materialization or writes"
    );

    let mut tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    assert_eq!(
        Repo.commercial_purchase_owner(&mut tx, claimant.into(), timed.id)
            .await
            .unwrap(),
        UserId::from(claimant)
    );
    assert_eq!(
        Purchase
            .timing_statement(&mut tx, timed.id, false)
            .await
            .unwrap(),
        Some(vec![])
    );
    assert_eq!(
        Purchase
            .fulfillment_statement(&mut tx, timed.id, false)
            .await
            .unwrap(),
        Some(vec![])
    );
    assert_eq!(
        Purchase
            .timing_statement(&mut tx, timed.id, true)
            .await
            .unwrap(),
        Some(b"timing v2 original".to_vec())
    );
    assert_eq!(
        Purchase
            .fulfillment_statement(&mut tx, timed.id, true)
            .await
            .unwrap(),
        Some(b"fulfillment v2 original".to_vec())
    );
    tx.commit().await.unwrap();
    let mut tx = db.begin_transaction().await.unwrap();
    let mut variants = Vec::new();
    for kind in ["timing", "fulfillment"] {
        for mode in [
            "no-parent",
            "v1-no-correction",
            "v2-no-correction",
            "nonempty-correction",
        ] {
            let o = ordinary_offer(&mut tx, s1, "skills", 17, Some("fulfilled")).await;
            if mode != "no-parent" {
                parent(
                    &mut tx,
                    o.id,
                    kind,
                    "parent original",
                    if mode == "v1-no-correction" { 1 } else { 2 },
                )
                .await;
            }
            if mode == "v1-no-correction" {
                // The new legacy observer auto-adds a correction. Remove only this
                // synthetic row to exercise a schema-valid missing historic artifact.
                tx.txn().execute("DELETE FROM purchase_document_corrections WHERE order_id=$1 AND document_kind=$2",&[&o.id,&kind]).await.unwrap();
            }
            if mode == "nonempty-correction" || mode == "no-parent" {
                correction(&mut tx, o.id, kind, "selected correction").await;
            }
            variants.push((o.id, kind, mode));
        }
    }
    tx.commit().await.unwrap();
    let before = fingerprint(&db).await;
    let mut tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    for (id, kind, mode) in variants {
        let actual = if kind == "timing" {
            Purchase.timing_statement(&mut tx, id, false).await.unwrap()
        } else {
            Purchase
                .fulfillment_statement(&mut tx, id, false)
                .await
                .unwrap()
        };
        let expected = match mode {
            "v2-no-correction" => Some(b"parent original".to_vec()),
            "nonempty-correction" => Some(b"selected correction".to_vec()),
            _ => None,
        };
        assert_eq!(actual, expected, "{kind} {mode}");
    }
    tx.commit().await.unwrap();
    assert_eq!(before, fingerprint(&db).await);
    println!(
        "PASS: both empty corrections defeat nonempty v2 originals; original variants remain exact; no-parent, missing-v1, v2 fallback and nonempty correction selection use unchanged actual statement readers"
    );

    let locked = db.begin_transaction().await.unwrap();
    locked
        .txn()
        .execute(
            "UPDATE purchase_progress SET state='failed' WHERE order_id=$1",
            &[&old.id],
        )
        .await
        .unwrap();
    let previous = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        read(&db, claimant, old.id),
    )
    .await
    .expect("narrow status read must not wait on progress FOR UPDATE")
    .unwrap();
    assert_eq!(previous.state, "review");
    locked.commit().await.unwrap();
    assert_eq!(read(&db, claimant, old.id).await.unwrap().state, "failed");
    let mut tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    assert!(Purchase.get(&mut tx, old.id).await.is_err());
    tx.rollback().await.unwrap();
    println!(
        "PASS: status sees prior committed progress without waiting for writer, next read sees committed change; ordinary locking get stays incompatible with READ ONLY and is unchanged"
    );
}
