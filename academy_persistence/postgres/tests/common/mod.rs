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

    academy_demo::create(
        &mut txn,
        PostgresUserRepository,
        PostgresSessionRepository,
        PostgresMfaRepository,
        PostgresOAuth2Repository,
    )
    .await
    .unwrap();

    txn.commit().await.unwrap();
}

pub async fn setup_clean() -> Db {
    let db = fixture::connect().await;

    db.reset().await.unwrap();
    db
}
