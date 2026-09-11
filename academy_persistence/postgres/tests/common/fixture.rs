//! Admission for the real, disposable PostgreSQL process created by test-unit.py.
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};

use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{MIGRATIONS, PostgresDatabase, PostgresDatabaseConfig};
use serde_json::{Value, json};
use uuid::Uuid;

pub fn root() -> PathBuf {
    let requested = PathBuf::from(
        std::env::var("ACADEMY_UNIT_TEST_FIXTURE").expect("owned CI fixture required"),
    );
    let root = requested.canonicalize().unwrap();
    assert_eq!(requested, root);
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(
        root.file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("academy-unit-tests-")
    );
    let uid = std::fs::metadata("/proc/self").unwrap().uid();
    let meta = root.metadata().unwrap();
    assert_eq!(meta.uid(), uid);
    assert_eq!(meta.mode() & 0o077, 0);
    let owner = root.join("OWNER.json");
    let meta = std::fs::symlink_metadata(&owner).unwrap();
    assert!(meta.file_type().is_file());
    assert_eq!(meta.uid(), uid);
    assert_eq!(meta.mode() & 0o077, 0);
    let marker: Value = serde_json::from_slice(&std::fs::read(owner).unwrap()).unwrap();
    assert_eq!(marker["unit"], "academy-unit-tests");
    assert_eq!(marker["suite"], "postgres");
    assert_eq!(marker["canonical_root"], root.to_str().unwrap());
    assert_eq!(marker["owner_uid"].as_u64(), Some(u64::from(uid)));
    let run = std::env::var("ACADEMY_UNIT_TEST_RUN_ID").unwrap();
    Uuid::parse_str(&run).unwrap();
    assert_eq!(marker["run_id"].as_str(), Some(run.as_str()));
    root
}

pub fn evidence_path(label: &str, suffix: &str) -> PathBuf {
    assert!(
        label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    );
    assert!(suffix.chars().all(|c| c.is_ascii_alphanumeric()));
    let root = root();
    let evidence = root.join("evidence");
    assert_eq!(evidence.canonicalize().unwrap(), evidence);
    assert_eq!(evidence.metadata().unwrap().mode() & 0o077, 0);
    evidence.join(format!("{label}-{}.{}", Uuid::new_v4(), suffix))
}

pub async fn connect() -> PostgresDatabase {
    connect_database("academy_unit_tests").await
}

pub async fn fresh_history() -> PostgresDatabase {
    let base = connect().await;
    let name = format!("academy_ci_history_{}", Uuid::new_v4().simple());
    base.execute(&format!("CREATE DATABASE {name} TEMPLATE template0 LOCALE_PROVIDER icu ICU_LOCALE 'de-DE' ENCODING 'UTF8'")).await.unwrap();
    std::fs::write(root().join("evidence").join(format!("{name}.json")), serde_json::to_vec(&json!({"database": name, "run_id": std::env::var("ACADEMY_UNIT_TEST_RUN_ID").unwrap()})).unwrap()).unwrap();
    connect_database(&name).await
}

async fn connect_database(name: &str) -> PostgresDatabase {
    let root = root();
    let marker: Value =
        serde_json::from_slice(&std::fs::read(root.join("OWNER.json")).unwrap()).unwrap();
    let config_path = root.join("fixture.toml");
    let metadata = std::fs::symlink_metadata(&config_path).unwrap();
    assert!(metadata.file_type().is_file());
    assert_eq!(
        metadata.uid(),
        std::fs::metadata("/proc/self").unwrap().uid()
    );
    assert_eq!(metadata.mode() & 0o077, 0);
    assert_eq!(
        std::env::var("ACADEMY_CONFIG").unwrap(),
        config_path.to_str().unwrap()
    );
    assert!(std::env::var_os("DATABASE_URL").is_none());
    assert!(!std::env::vars_os().any(|(key, _)| key.to_string_lossy().starts_with("PG")));
    assert_eq!(std::env::var("SQLX_OFFLINE").unwrap(), "true");
    assert_eq!(std::env::var("RUST_TEST_THREADS").unwrap(), "1");
    assert!(!std::env::vars_os().any(|(key, _)| {
        ["BOOTSTRAP_", "IF1_", "INVOICE_"]
            .iter()
            .any(|prefix| key.to_string_lossy().starts_with(prefix))
    }));
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
    assert!(port >= 1024);
    assert_eq!(parsed.get_ports(), &[port]);
    assert_eq!(marker["database"], "academy_unit_tests");
    assert_eq!(marker["role"], "academy_unit_tests");
    assert_eq!(parsed.get_dbname(), Some("academy_unit_tests"));
    assert_eq!(parsed.get_user(), Some("academy_unit_tests"));
    let data = root.join("pgdata");
    assert_eq!(data.canonicalize().unwrap(), data);
    let db = PostgresDatabase::connect(&PostgresDatabaseConfig {
        url: format!("postgres://academy_unit_tests@127.0.0.1:{port}/{name}"),
        max_connections: 12,
        min_connections: 0,
        acquire_timeout: Duration::from_secs(10),
        idle_timeout: None,
        max_lifetime: None,
    })
    .await
    .unwrap();
    let txn = db.begin_transaction().await.unwrap();
    let row = txn.txn().query_one("SELECT current_database()::text,current_user::text,inet_server_port(),current_setting('data_directory'),version(),current_setting('statement_timeout'),(SELECT datlocprovider::text FROM pg_database WHERE datname=current_database()),('ä'::text COLLATE \"default\" < 'z') AND ('ä'::text COLLATE \"C\" > 'z')", &[]).await.unwrap();
    assert_eq!(row.get::<_, &str>(0), name);
    assert_eq!(row.get::<_, &str>(1), "academy_unit_tests");
    assert_eq!(row.get::<_, i32>(2), i32::from(port));
    assert_eq!(
        PathBuf::from(row.get::<_, &str>(3)).canonicalize().unwrap(),
        data
    );
    assert!(row.get::<_, &str>(4).starts_with("PostgreSQL 18."));
    assert_eq!(row.get::<_, &str>(5), "1min");
    assert_eq!(row.get::<_, &str>(6), "i");
    assert!(
        row.get::<_, bool>(7),
        "fixture must exercise ICU default order distinct from C"
    );
    let guard = json!({"database":row.get::<_, &str>(0),"role":row.get::<_, &str>(1),"port":port,
        "data_directory":row.get::<_, &str>(3),"version":row.get::<_, &str>(4),"run_id":marker["run_id"],"icu":true});
    txn.commit().await.unwrap();
    // The actual checkout, not a copied historical path or a presence-only test.
    for migration in MIGRATIONS {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("migrations")
            .join(migration.name);
        assert_eq!(
            migration.up,
            std::fs::read_to_string(directory.join("up.sql")).unwrap()
        );
        assert_eq!(
            migration.down,
            std::fs::read_to_string(directory.join("down.sql")).unwrap()
        );
    }
    std::fs::write(
        evidence_path("guard", "json"),
        serde_json::to_vec_pretty(&guard).unwrap(),
    )
    .unwrap();
    preserve_dump(name, "before-reset");
    println!(
        "OWNED_CI_RESET_TARGET {} {} {}",
        marker["run_id"],
        port,
        data.display()
    );
    db
}

pub async fn preserve(db: &PostgresDatabase, label: &str) {
    let tx = db.begin_transaction().await.unwrap();
    let name: String = tx
        .txn()
        .query_one("SELECT current_database()::text", &[])
        .await
        .unwrap()
        .get(0);
    tx.commit().await.unwrap();
    preserve_dump(&name, label);
}

fn preserve_dump(name: &str, label: &str) {
    let root = root();
    let marker: Value =
        serde_json::from_slice(&std::fs::read(root.join("OWNER.json")).unwrap()).unwrap();
    if name != "academy_unit_tests" {
        let id = name
            .strip_prefix("academy_ci_history_")
            .expect("owned history database");
        assert_eq!(Uuid::parse_str(id).unwrap().simple().to_string(), id);
        let record: Value = serde_json::from_slice(
            &std::fs::read(root.join("evidence").join(format!("{name}.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(record["database"], name);
        assert_eq!(record["run_id"], marker["run_id"]);
    }
    let pg_bin = PathBuf::from(marker["pg_bin"].as_str().unwrap());
    assert!(pg_bin.is_absolute());
    assert_eq!(pg_bin.canonicalize().unwrap(), pg_bin);
    let version = std::process::Command::new(pg_bin.join("pg_dump"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert!(
        String::from_utf8(version.stdout)
            .unwrap()
            .contains("(PostgreSQL) 18.")
    );
    let output = std::process::Command::new(pg_bin.join("pg_dump"))
        .env_clear()
        .args([
            "-h",
            "127.0.0.1",
            "-p",
            &marker["port"].to_string(),
            "-U",
            "academy_unit_tests",
            "-d",
            name,
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
    let target = evidence_path(label, "sql");
    std::fs::write(&target, output.stdout).unwrap();
    println!("PRESERVED_PRE_RESET {}", target.display());
}
