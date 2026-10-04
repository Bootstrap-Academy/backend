#![allow(
    dead_code,
    reason = "Each integration-test binary compiles its own subset of shared fixture helpers"
)]
use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{
    MIGRATIONS, PostgresDatabase, mfa::PostgresMfaRepository, oauth2::PostgresOAuth2Repository,
    session::PostgresSessionRepository, user::PostgresUserRepository,
};

pub mod fixture;

pub type Db = PostgresDatabase;

pub async fn setup() -> Db {
    setup_through(None).await
}

pub async fn setup_through(last: Option<&str>) -> Db {
    let db = if last.is_some() {
        fixture::fresh_history().await
    } else {
        setup_clean().await
    };
    apply_through(&db, last).await;
    seed(&db).await;
    db
}

pub async fn setup_before(name: &str, with_demo: bool) -> Db {
    let db = fixture::fresh_history().await;
    let count = MIGRATIONS
        .iter()
        .position(|m| m.name == name)
        .expect("named historical boundary");
    assert_eq!(db.run_migrations(Some(count)).await.unwrap().len(), count);
    if with_demo {
        seed(&db).await;
    }
    db
}

pub async fn apply_through(db: &Db, last: Option<&str>) -> Vec<&'static str> {
    let end = last
        .map(|name| {
            MIGRATIONS
                .iter()
                .position(|m| m.name == name)
                .expect("named boundary")
                + 1
        })
        .unwrap_or(MIGRATIONS.len());
    let state = db.list_migrations().await.unwrap();
    assert!(
        state.iter().skip(end).all(|m| !m.applied),
        "historical reapply cannot remove a later guard"
    );
    let applied = state.into_iter().take(end).filter(|m| m.applied).count();
    db.run_migrations(Some(end - applied)).await.unwrap()
}

pub async fn seed(db: &Db) {
    let mut txn = db.begin_transaction().await.unwrap();

    let origin_recorded: bool = txn
        .txn()
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns \
             WHERE table_schema=current_schema() AND table_name='sessions' AND column_name='origin')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    if origin_recorded {
        academy_demo::create(
            &mut txn,
            PostgresUserRepository,
            PostgresSessionRepository,
            PostgresMfaRepository,
            PostgresOAuth2Repository,
        )
        .await
        .unwrap();
    } else {
        // Historical boundaries precede recorded session origins. Their demo
        // sessions keep the columns of that time and become legacy sessions
        // once the origin migration runs, like real sessions did.
        academy_demo::user::create(&mut txn, PostgresUserRepository)
            .await
            .unwrap();
        for session in &*academy_demo::session::ALL_SESSIONS {
            txn.txn()
                .execute(
                    "INSERT INTO sessions(id,user_id,device_name,created_at,updated_at,mfa_verified) \
                     VALUES($1,$2,$3,$4,$5,$6)",
                    &[
                        &*session.id,
                        &*session.user_id,
                        &session.device_name.as_deref(),
                        &session.created_at,
                        &session.updated_at,
                        &session.mfa_verified,
                    ],
                )
                .await
                .unwrap();
        }
        academy_demo::mfa::create(&mut txn, PostgresMfaRepository)
            .await
            .unwrap();
        academy_demo::oauth2::create(&mut txn, PostgresOAuth2Repository)
            .await
            .unwrap();
    }

    txn.commit().await.unwrap();
}

pub async fn setup_clean() -> Db {
    let db = fixture::connect().await;

    db.reset().await.unwrap();
    db
}

/// Check the intended preservation guard independently of later additive migrations.
pub async fn assert_down_refused(db: &Db, name: &str, expected: &str) {
    let migration = academy_persistence_postgres::MIGRATIONS
        .iter()
        .find(|m| m.name == name)
        .unwrap();
    let tx = db.begin_transaction().await.unwrap();
    let error = tx.txn().batch_execute(migration.down).await.unwrap_err();
    assert_eq!(error.code().unwrap().code(), "P0001");
    assert!(
        error.as_db_error().unwrap().message().contains(expected),
        "{error:?}"
    );
    tx.rollback().await.unwrap();
}
