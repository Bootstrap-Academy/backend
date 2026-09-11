//! Runs the actual per-item production task against only the marked synthetic DB.
//! FsService below is a test double: no production filesystem deletion occurs.
use super::*;
use academy_persistence_postgres::{PostgresDatabase, PostgresDatabaseConfig};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;
const FORWARD: &str = "2026-09-11-030000_invoice_disposal_preservation";
async fn setup() -> PostgresDatabase {
    let root = PathBuf::from(std::env::var("BOOTSTRAP_INVOICE_PRESERVATION_FIXTURE").unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("bootstrap-invoice-preservation-")
    );
    let marker: Value =
        serde_json::from_slice(&std::fs::read(root.join("OWNER.json")).unwrap()).unwrap();
    assert_eq!(marker["canonical_root"], root.to_str().unwrap());
    assert_eq!(marker["unit"], "L3-invoice-disposal-preservation-backend-1");
    assert_eq!(marker["owner"], "/root/learning_source_review");
    assert_eq!(
        std::env::var("ACADEMY_CONFIG").unwrap(),
        root.join("fixture.toml").to_str().unwrap()
    );
    assert!(std::env::var_os("DATABASE_URL").is_none());
    assert!(!std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("PG")));
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
    assert_eq!(port, 56980);
    assert_eq!(parsed.get_ports(), &[port]);
    assert_eq!(parsed.get_dbname(), marker["database"].as_str());
    assert_eq!(parsed.get_user(), marker["role"].as_str());
    let db = PostgresDatabase::connect(&PostgresDatabaseConfig {
        url: config.database.url.clone(),
        max_connections: 12,
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
    let guard = json!({"marker":marker,"parsed_host":"127.0.0.1","parsed_port":port,"parsed_database":parsed.get_dbname(),"parsed_role":parsed.get_user(),"actual_database":row.get::<_,&str>(0),"actual_role":row.get::<_,&str>(1),"actual_port":row.get::<_,i32>(2),"actual_data_directory":row.get::<_,&str>(3),"actual_version":row.get::<_,&str>(4)});
    let capture_id = Uuid::new_v4();
    std::fs::write(
        root.join("evidence")
            .join(format!("reset-guard-{capture_id}.json")),
        serde_json::to_vec_pretty(&guard).unwrap(),
    )
    .unwrap();
    txn.commit().await.unwrap();
    drop(db);
    // Preserve the preceding full owned database state before the next reset.
    let dump = root
        .join("evidence")
        .join(format!("pre-reset-{capture_id}.sql"));
    let output = std::process::Command::new(
        "/nix/store/qbfm4pm2smh5znpvvwwwg95d6njwl17w-postgresql-18.6/bin/pg_dump",
    )
    .env_clear()
    .args([
        "-h",
        "127.0.0.1",
        "-p",
        "56980",
        "-U",
        marker["role"].as_str().unwrap(),
        "-d",
        marker["database"].as_str().unwrap(),
        "--no-owner",
        "--no-privileges",
    ])
    .output()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(&dump, output.stdout).unwrap();
    println!("PRESERVED_PRE_RESET {}", dump.display());
    let baseline = std::env::var("INVOICE_SOURCE_BASELINE").as_deref() == Ok("1");
    if !baseline {
        assert!(
            academy_persistence_postgres::MIGRATIONS
                .iter()
                .any(|m| m.name == FORWARD),
            "current forward must be embedded before reset"
        );
    }
    let db = PostgresDatabase::connect(&PostgresDatabaseConfig {
        url: config.database.url.clone(),
        max_connections: 12,
        min_connections: 0,
        acquire_timeout: Duration::from_secs(10),
        idle_timeout: None,
        max_lifetime: None,
    })
    .await
    .unwrap();
    db.reset().await.unwrap();
    db.run_migrations(None).await.unwrap();
    db
}

async fn seed(db: &PostgresDatabase, n: &str) {
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().execute("INSERT INTO invoice_originals(invoice_number,pdf,provenance) VALUES($1,$2,'synthetic_original')",&[&n,&b"synthetic bytes".as_slice()]).await.unwrap();
    tx.txn().execute("INSERT INTO invoice_reconciliation(invoice_number,state,reason) VALUES($1,'evidenced','synthetic original')",&[&n]).await.unwrap();
    tx.txn().execute("INSERT INTO commercial_archive_work(number,kind,source,disposal_authorized,assessment) VALUES($1,'invoice','unrecorded_archive',true,'{\"original\":true}')",&[&n]).await.unwrap();
    tx.commit().await.unwrap();
}
async fn pending(db: &PostgresDatabase, n: &str) {
    let tx = db.begin_transaction().await.unwrap();
    tx.txn()
        .execute(
            "SELECT pg_advisory_xact_lock(hashtextextended('commercial-archive:'||$1::text,0))",
            &[&n],
        )
        .await
        .unwrap();
    tx.txn().execute("INSERT INTO commercial_invoice_identity_reviews(number,reason,source_key,evidence) VALUES($1,'synthetic_pending','task-control','{}') ON CONFLICT DO NOTHING",&[&n]).await.unwrap();
    tx.commit().await.unwrap();
}
async fn state(db: &PostgresDatabase, n: &str) -> (bool, bool, bool, bool) {
    let tx = db.begin_transaction().await.unwrap();
    let r=tx.txn().query_one("SELECT disposal_started_at IS NOT NULL,file_removed_at IS NOT NULL,EXISTS(SELECT 1 FROM invoice_originals WHERE invoice_number=$1),commercial_invoice_identity_pending($1) FROM commercial_archive_work WHERE number=$1 AND kind='invoice'",&[&n]).await.unwrap();
    (r.get(0), r.get(1), r.get(2), r.get(3))
}
struct Files {
    db: PostgresDatabase,
    calls: Arc<AtomicUsize>,
    mode: &'static str,
}
impl Files {
    fn new(db: &PostgresDatabase, mode: &'static str) -> Self {
        Self {
            db: db.clone(),
            calls: Arc::new(AtomicUsize::new(0)),
            mode,
        }
    }
}
impl FsService for Files {
    async fn store_file(&self, _: &Path, _: &[u8]) -> anyhow::Result<()> {
        panic!("unexpected file write")
    }
    async fn read_file(&self, _: &Path) -> anyhow::Result<Option<Vec<u8>>> {
        panic!("unexpected file read")
    }
    async fn list_files(&self, _: &Path) -> anyhow::Result<Vec<PathBuf>> {
        panic!("unexpected file enumeration")
    }
    async fn delete_file(&self, path: &Path) -> anyhow::Result<bool> {
        assert_eq!(path.parent(), Some(Path::new("/synthetic-never-opened")));
        let n = path.file_stem().unwrap().to_str().unwrap();
        // Independent real transaction proves admission committed before this callback.
        let observed = state(&self.db, n).await;
        assert!(observed.0 && !observed.1 && !observed.2);
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.mode == "error" {
            anyhow::bail!("synthetic removal unavailable");
        }
        if self.mode == "late_pending" {
            pending(&self.db, n).await;
        }
        Ok(self.mode != "absent")
    }
}
async fn item(db: &PostgresDatabase, fs: &Files, n: &str) -> anyhow::Result<bool> {
    dispose_archive_item(
        db,
        &PostgresFinancialDocumentRepository,
        fs,
        Path::new("/synthetic-never-opened"),
        &n.try_into().unwrap(),
        FinancialDocumentKind::Invoice,
    )
    .await
}

#[tokio::test]
async fn invoice_task_false_admission_has_zero_file_calls() {
    let db = setup().await;
    let n = "R10000101";
    seed(&db, n).await;
    pending(&db, n).await;
    let fs = Files::new(&db, "present");
    assert!(!item(&db, &fs, n).await.unwrap());
    assert_eq!(fs.calls.load(Ordering::SeqCst), 0);
    assert_eq!(state(&db, n).await, (false, false, true, true));
    println!("TASK false actual SQL admission: committed preservation; zero file/ack calls");
}

#[tokio::test]
async fn invoice_task_failed_admission_commit_has_zero_file_calls() {
    let db = setup().await;
    let n = "R10000102";
    seed(&db, n).await;
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().batch_execute("CREATE FUNCTION synthetic_fail_admission() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.disposal_started_at IS NOT NULL THEN RAISE EXCEPTION 'synthetic admission commit failure'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER synthetic_fail_admission AFTER UPDATE ON commercial_archive_work DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_fail_admission();").await.unwrap();
    tx.commit().await.unwrap();
    let fs = Files::new(&db, "present");
    assert!(item(&db, &fs, n).await.is_err());
    assert_eq!(fs.calls.load(Ordering::SeqCst), 0);
    assert_eq!(state(&db, n).await, (false, false, true, false));
    println!(
        "TASK actual admission COMMIT failure: zero file calls and original preserved by rollback"
    );
}

#[tokio::test]
async fn invoice_task_removal_error_retains_intent_and_new_pending_blocks_retry() {
    let db = setup().await;
    let n = "R10000103";
    seed(&db, n).await;
    let fs = Files::new(&db, "error");
    assert!(item(&db, &fs, n).await.is_err());
    assert_eq!(state(&db, n).await, (true, false, false, false));
    pending(&db, n).await;
    assert!(!item(&db, &fs, n).await.unwrap());
    assert_eq!(fs.calls.load(Ordering::SeqCst), 1);
    assert_eq!(state(&db, n).await, (true, false, false, true));
    println!(
        "TASK removal error: committed original deletion/intent survives; new pending refuses second file call"
    );
}

#[tokio::test]
async fn invoice_task_success_absence_and_discarded_result_keep_ack_history() {
    let db = setup().await;
    for (n, mode, removed) in [
        ("R10000104", "present", true),
        ("R10000105", "absent", false),
    ] {
        seed(&db, n).await;
        let fs = Files::new(&db, mode);
        assert_eq!(item(&db, &fs, n).await.unwrap(), removed);
        assert_eq!(state(&db, n).await, (true, true, false, false));
        // Simulate losing the caller's receipt by retrying after the committed acknowledgement.
        assert!(!item(&db, &fs, n).await.unwrap());
        assert_eq!(fs.calls.load(Ordering::SeqCst), 1);
    }
    println!(
        "TASK actual positive/absent outcomes acknowledged; discarded success response retry makes no second file call"
    );
}

#[tokio::test]
async fn invoice_task_postcommit_pending_does_not_conceal_actual_removal() {
    let db = setup().await;
    let n = "R10000106";
    seed(&db, n).await;
    let fs = Files::new(&db, "late_pending");
    assert!(item(&db, &fs, n).await.unwrap());
    assert_eq!(state(&db, n).await, (true, true, false, true));
    assert!(!item(&db, &fs, n).await.unwrap());
    assert_eq!(fs.calls.load(Ordering::SeqCst), 1);
    println!(
        "TASK postcommit review cannot revoke authorized callback; actual removal acknowledged and new admission blocked"
    );
}

#[tokio::test]
async fn invoice_task_ack_commit_error_preserves_recoverable_intent() {
    let db = setup().await;
    let n = "R10000107";
    seed(&db, n).await;
    let tx = db.begin_transaction().await.unwrap();
    tx.txn().batch_execute("CREATE FUNCTION synthetic_fail_ack() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.file_removed_at IS NOT NULL THEN RAISE EXCEPTION 'synthetic acknowledgement commit failure'; END IF; RETURN NEW; END $$; CREATE CONSTRAINT TRIGGER synthetic_fail_ack AFTER UPDATE ON commercial_archive_work DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_fail_ack();").await.unwrap();
    tx.commit().await.unwrap();
    let fs = Files::new(&db, "present");
    assert!(item(&db, &fs, n).await.is_err());
    assert_eq!(fs.calls.load(Ordering::SeqCst), 1);
    assert_eq!(state(&db, n).await, (true, false, false, false));
    pending(&db, n).await;
    assert!(!item(&db, &fs, n).await.unwrap());
    assert_eq!(fs.calls.load(Ordering::SeqCst), 1);
    println!(
        "TASK actual acknowledgement COMMIT error retains begun history; later pending blocks retry"
    );
}
