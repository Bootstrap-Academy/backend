use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{
    PostgresDatabase, PostgresDatabaseConfig, mfa::PostgresMfaRepository,
    oauth2::PostgresOAuth2Repository, session::PostgresSessionRepository,
    user::PostgresUserRepository,
};

pub type Db = PostgresDatabase;

pub async fn setup() -> Db {
    let db = setup_clean().await;

    db.run_migrations(None).await.unwrap();

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

    db
}

pub async fn setup_clean() -> Db {
    let config = academy_config::load().unwrap();

    let db = Db::connect(&PostgresDatabaseConfig {
        url: config.database.url,
        max_connections: config.database.max_connections,
        min_connections: config.database.min_connections,
        acquire_timeout: config.database.acquire_timeout.into(),
        idle_timeout: config.database.idle_timeout.map(Into::into),
        max_lifetime: config.database.max_lifetime.map(Into::into),
    })
    .await
    .unwrap();

    db.reset().await.unwrap();
    db
}
