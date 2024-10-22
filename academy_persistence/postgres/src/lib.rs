use std::{collections::HashSet, fmt::Write, time::Duration};

use academy_models::Sha256Hash;
use academy_persistence_contracts::{Database, Transaction};
use academy_utils::trace_instrument;
use anyhow::{anyhow, Context};
use bb8::{Pool, PooledConnection};
use bb8_postgres::{
    tokio_postgres::{self, NoTls},
    PostgresConnectionManager,
};
use ouroboros::self_referencing;
use tracing::trace;

pub mod coin;
pub mod mfa;
pub mod oauth2;
pub mod paypal;
pub mod session;
pub mod user;

type PgClient = tokio_postgres::Client;
type PgPooledConnection = PooledConnection<'static, PostgresConnectionManager<NoTls>>;
type PgTransaction<'a> = tokio_postgres::Transaction<'a>;

#[derive(Debug, Clone)]
pub struct PostgresDatabase {
    pool: Pool<PostgresConnectionManager<NoTls>>,
}

#[derive(Debug)]
pub struct PostgresDatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_lifetime: Option<Duration>,
}

impl PostgresDatabase {
    pub async fn connect(config: &PostgresDatabaseConfig) -> anyhow::Result<Self> {
        let manager = PostgresConnectionManager::new(config.url.parse()?, NoTls);
        let pool = Pool::builder()
            .max_size(config.max_connections)
            .min_idle(config.min_connections)
            .connection_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .build(manager)
            .await?;

        Ok(Self { pool })
    }

    #[cfg(feature = "dummy")]
    pub async fn dummy() -> Self {
        let manager = PostgresConnectionManager::new("".parse().unwrap(), NoTls);
        Self {
            pool: Pool::builder().build_unchecked(manager),
        }
    }

    pub async fn list_migrations(&self) -> anyhow::Result<Vec<MigrationStatus>> {
        let conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire database connection")?;
        create_migrations_table(&conn)
            .await
            .context("Failed to create migrations table")?;
        list_migrations(&conn)
            .await
            .context("Failed to list migrations")
    }

    pub async fn run_migrations(&self, cnt: Option<usize>) -> anyhow::Result<Vec<&'static str>> {
        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire database connection")?;
        create_migrations_table(&conn)
            .await
            .context("Failed to create migrations table")?;

        let mut out = Vec::new();
        let insert_migration = conn
            .prepare("insert into _migrations (name) values ($1);")
            .await?;
        let pending = list_migrations(&conn)
            .await
            .context("Failed to list migrations")?
            .into_iter()
            .filter_map(|MigrationStatus { migration, applied }| (!applied).then_some(migration))
            .take(cnt.unwrap_or(usize::MAX));
        for migration in pending {
            let txn = conn
                .transaction()
                .await
                .context("Failed to begin transaction")?;
            txn.batch_execute(migration.up)
                .await
                .with_context(|| format!("Failed to run migration {}", migration.name))?;
            txn.execute(&insert_migration, &[&migration.name])
                .await
                .with_context(|| format!("Failed to mark migration {} as run", migration.name))?;
            txn.commit().await.context("Failed to commit transaction")?;
            out.push(migration.name);
        }
        Ok(out)
    }

    pub async fn revert_migrations(&self, cnt: Option<usize>) -> anyhow::Result<Vec<&'static str>> {
        let mut conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire database connection")?;
        create_migrations_table(&conn)
            .await
            .context("Failed to create migrations table")?;

        let mut out = Vec::new();
        let revert_migration = conn
            .prepare("delete from _migrations where name=$1")
            .await?;
        let applied = list_migrations(&conn)
            .await
            .context("Failed to list migrations")?
            .into_iter()
            .rev()
            .filter_map(|MigrationStatus { migration, applied }| applied.then_some(migration))
            .take(cnt.unwrap_or(usize::MAX));
        for migration in applied {
            let txn = conn
                .transaction()
                .await
                .context("Failed to begin transaction")?;
            txn.batch_execute(migration.down)
                .await
                .with_context(|| format!("Failed to revert migration {}", migration.name))?;
            txn.execute(&revert_migration, &[&migration.name])
                .await
                .with_context(|| {
                    format!("Failed to mark migration {} as reverted", migration.name)
                })?;
            txn.commit().await.context("Failed to commit transaction")?;
            out.push(migration.name);
        }

        Ok(out)
    }

    pub async fn reset(&self) -> anyhow::Result<()> {
        self.execute("drop schema public cascade; create schema public;")
            .await
            .context("Failed to drop and recreate schema public")
    }

    pub async fn execute(&self, query: &str) -> anyhow::Result<()> {
        let conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire database connection")?;
        conn.batch_execute(query)
            .await
            .context("Failed to execute query")?;
        Ok(())
    }
}

impl Database for PostgresDatabase {
    type Transaction = PostgresTransaction;

    async fn begin_transaction(&self) -> anyhow::Result<Self::Transaction> {
        trace!("begin transaction");

        let conn = self
            .pool
            .get_owned()
            .await
            .context("Failed to acquire database connection")?;

        PostgresTransactionAsyncSendTryBuilder {
            conn,
            txn_builder: |conn| Box::pin(async move { conn.transaction().await.map(Some) }),
        }
        .try_build()
        .await
        .context("Failed to begin transaction")
    }

    #[trace_instrument(skip(self))]
    async fn ping(&self) -> anyhow::Result<()> {
        let conn = self
            .pool
            .get()
            .await
            .context("Failed to acquire database connection")?;

        conn.query_one("select 1", &[])
            .await
            .map_err(Into::into)
            .map(|row| row.get(0))
            .and_then(|res: i32| {
                (res == 1)
                    .then_some(())
                    .ok_or_else(|| anyhow!("Expected a result of 1, got {res} instead"))
            })
            .context("Failed to ping database")
    }
}

#[self_referencing]
pub struct PostgresTransaction {
    conn: PgPooledConnection,
    #[borrows(mut conn)]
    #[covariant]
    txn: Option<PgTransaction<'this>>,
}

impl PostgresTransaction {
    fn txn(&self) -> &PgTransaction<'_> {
        self.borrow_txn().as_ref().unwrap()
    }
}

impl Transaction for PostgresTransaction {
    async fn commit(mut self) -> anyhow::Result<()> {
        trace!("commit transaction");

        self.with_txn_mut(|txn| txn.take())
            .unwrap()
            .commit()
            .await
            .context("Failed to commit transaction")
    }

    async fn rollback(mut self) -> anyhow::Result<()> {
        trace!("rollback transaction");

        self.with_txn_mut(|txn| txn.take())
            .unwrap()
            .rollback()
            .await
            .context("Failed to rollback transaction")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub name: &'static str,
    pub up: &'static str,
    pub down: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct MigrationStatus {
    pub migration: Migration,
    pub applied: bool,
}

// generated by `build.rs` script
pub const MIGRATIONS: &[Migration] = include!(env!("MIGRATIONS"));

async fn create_migrations_table(conn: &PgClient) -> anyhow::Result<()> {
    conn.execute(
        "create table if not exists _migrations (name text primary key);",
        &[],
    )
    .await?;
    Ok(())
}

async fn list_migrations(conn: &PgClient) -> anyhow::Result<Vec<MigrationStatus>> {
    let applied = conn
        .query("select name from _migrations;", &[])
        .await?
        .into_iter()
        .map(|row| row.get(0))
        .collect::<HashSet<String>>();

    Ok(MIGRATIONS
        .iter()
        .map(|&migration| MigrationStatus {
            migration,
            applied: applied.contains(migration.name),
        })
        .collect())
}

fn decode_sha256hash(hash: Vec<u8>) -> anyhow::Result<Sha256Hash> {
    hash.try_into()
        .map(Sha256Hash)
        .map_err(|x| anyhow!("Failed to decode SHA256 hash {x:?}"))
}

macro_rules! columns {
    ($vis:vis $ident:ident as $alias:literal: $fst:literal $(, $col:literal)* $(,)?) => {
        ::paste::paste! {
            #[allow(unused, reason = "usually not needed for views")]
            $vis const [< $ident:snake:upper _CNT >]: usize = [ $fst $(, $col)* ].len();
            $vis const [< $ident:snake:upper _COLS >]: &str = ::core::concat!( '"', $alias, "\".\"", $fst, '"' $(, ", \"" , $alias, "\".\"", $col, '"' )* );
            #[allow(unused, reason = "usually not needed for views")]
            $vis const [< $ident:snake:upper _COL_NAMES >]: &str = ::core::concat!( '"', $fst, '"' $(, ", \"", $col, '"' )* );
        }
    };
}
use columns;

fn arg_indices(indices: impl IntoIterator<Item = usize>) -> String {
    let mut it = indices.into_iter();
    let mut out = String::new();
    if let Some(x) = it.next() {
        write!(&mut out, "${x}").unwrap();
    }
    for x in it {
        write!(&mut out, ", ${x}").unwrap();
    }
    out
}

#[derive(Debug, Default)]
struct ColumnCounter(usize);
impl ColumnCounter {
    fn idx(&mut self) -> usize {
        let idx = self.0;
        self.0 += 1;
        idx
    }
}
