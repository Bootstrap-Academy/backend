use std::{collections::HashSet, future::Future, time::Duration};

use academy_models::Sha256Hash;
use academy_persistence_contracts::{Database, Transaction};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow};
use bb8::{Pool, PooledConnection};
use bb8_postgres::{
    PostgresConnectionManager,
    tokio_postgres::{self, NoTls},
};
use ouroboros::self_referencing;
use tracing::trace;

pub mod admin_audit;
pub mod coin;
pub mod contract;
pub mod deletion;
pub mod finance;
pub mod heart;
pub mod mfa;
pub mod moderation;
pub mod oauth2;
pub mod paypal;
pub mod premium;
pub mod purchase;
pub mod session;
pub mod user;
pub mod withdrawal;

type PgClient = tokio_postgres::Client;
type PgPooledConnection = PooledConnection<'static, PostgresConnectionManager<NoTls>>;
type PgTransaction<'a> = tokio_postgres::Transaction<'a>;

#[derive(Debug, Clone)]
pub struct PostgresDatabase {
    pool: Pool<PostgresConnectionManager<NoTls>>,
    premium_shared_clock: bool,
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

fn premium_shares_receipt_clock(config: &tokio_postgres::Config) -> bool {
    // hostaddr overrides even Unix hosts and can select a different fallback
    // route. Conservatively grant no clock proof whenever it is configured.
    config.get_hostaddrs().is_empty()
        && !config.get_hosts().is_empty()
        && config.get_hosts().iter().all(|host| match host {
            tokio_postgres::config::Host::Unix(_) => true,
            tokio_postgres::config::Host::Tcp(host) => host == "127.0.0.1" || host == "::1",
        })
}

impl PostgresDatabase {
    pub async fn connect(config: &PostgresDatabaseConfig) -> anyhow::Result<Self> {
        let postgres_config: tokio_postgres::Config = config.url.parse()?;
        // Current production uses a local Unix socket. Direct loopback fixtures
        // share that host clock too. Remote clock order is deliberately unknown.
        let premium_shared_clock = premium_shares_receipt_clock(&postgres_config);
        let manager = PostgresConnectionManager::new(postgres_config, NoTls);
        let pool = Pool::builder()
            .max_size(config.max_connections)
            .min_idle(config.min_connections)
            .connection_timeout(config.acquire_timeout)
            .idle_timeout(config.idle_timeout)
            .max_lifetime(config.max_lifetime)
            .build(manager)
            .await?;

        Ok(Self {
            pool,
            premium_shared_clock,
        })
    }

    #[cfg(feature = "dummy")]
    pub async fn dummy() -> Self {
        let manager = PostgresConnectionManager::new("".parse().unwrap(), NoTls);
        Self {
            pool: Pool::builder().build_unchecked(manager),
            premium_shared_clock: false,
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
            // Old backfills used a truncating formatter. Keep their bytes and
            // applied-name history intact, but refuse unsafe pending inputs.
            let invoice_guard = match migration.name {
                "2026-09-03-200000_create_financial_documents" => Some("captured_at IS NOT NULL"),
                "2026-09-07-100000_add_withdrawal_consent_to_financial_documents" => {
                    Some("withdrawal_consent_at IS NOT NULL")
                }
                _ => None,
            };
            if let Some(predicate) = invoice_guard {
                // SHARE excludes source writes through the historical batch and
                // its marker commit; a separate preflight transaction would not.
                txn.batch_execute("LOCK TABLE paypal_coin_orders IN SHARE MODE")
                    .await?;
                let unsafe_input: bool = txn
                    .query_one(
                        &format!("SELECT EXISTS(SELECT 1 FROM paypal_coin_orders WHERE invoice_number>=10000000 AND {predicate})"),
                        &[],
                    )
                    .await?
                    .get(0);
                anyhow::ensure!(
                    !unsafe_input,
                    "Migration {} remains pending: long invoice inputs require an explicit reviewed upgrade procedure; original rows must not be renamed or marked applied",
                    migration.name
                );
            }
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
            premium_observation_users: HashSet::new(),
            premium_shared_clock: self.premium_shared_clock,
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
    premium_observation_users: HashSet<uuid::Uuid>,
    premium_shared_clock: bool,
    #[borrows(mut conn)]
    #[covariant]
    txn: Option<PgTransaction<'this>>,
}

impl PostgresTransaction {
    pub(crate) fn premium_shares_receipt_clock(&self) -> bool {
        *self.borrow_premium_shared_clock()
    }
    pub(crate) fn observe_premium_after_commit(&mut self, user_id: uuid::Uuid) {
        self.with_premium_observation_users_mut(|users| {
            users.insert(user_id);
        });
    }

    pub fn txn(&self) -> &PgTransaction<'_> {
        self.borrow_txn().as_ref().unwrap()
    }

    async fn savepoint<'a, F, T, E>(
        &'a self,
        f: impl FnOnce(&'a PgTransaction) -> F,
    ) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
        E: From<anyhow::Error>,
    {
        let txn = self.txn();
        txn.batch_execute("savepoint sp")
            .await
            .map_err(anyhow::Error::from)?;
        match f(txn).await {
            Ok(result) => Ok(result),
            Err(err) => {
                txn.batch_execute("rollback to savepoint sp")
                    .await
                    .map_err(anyhow::Error::from)?;
                Err(err)
            }
        }
    }
}

impl Transaction for PostgresTransaction {
    async fn commit(mut self) -> anyhow::Result<()> {
        trace!("commit transaction");

        self.with_txn_mut(|txn| txn.take())
            .unwrap()
            .commit()
            .await
            .context("Failed to commit transaction")?;

        let mut heads = self.into_heads();
        if !heads.premium_observation_users.is_empty() {
            // This is a new transaction after the purchase COMMIT. Its database
            // observation bounds visibility; it is never called a commit time.
            // Failure must not turn a committed debit into a failed purchase.
            let users: Vec<_> = heads.premium_observation_users.into_iter().collect();
            let observation: anyhow::Result<()> = async {
                let txn = heads.conn.transaction().await?;
                txn.batch_execute("SET LOCAL statement_timeout = '2s'").await?;
                txn.execute("INSERT INTO premium_period_commit_observation(operation_id) SELECT id FROM premium_period_changes WHERE user_id=ANY($1::uuid[]) ORDER BY id ON CONFLICT DO NOTHING", &[&users]).await?;
                txn.commit().await?;
                Ok(())
            }.await;
            if observation.is_err() {
                tracing::warn!(
                    "Committed Premium visibility observation failed; uncertain declaration ordering requires review"
                );
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_clock_supports_direct_unix_and_loopback_hosts() {
        for url in [
            "host=/run/postgresql user=academy",
            "postgres://academy@/academy?host=/run/postgresql",
            "host=127.0.0.1 user=academy",
            "host=::1 user=academy",
            "postgres://academy@127.0.0.1/academy",
            "postgres://academy@[::1]/academy",
            "host=/run/postgresql,127.0.0.1,::1 user=academy",
        ] {
            let config = url.parse().unwrap();
            assert!(premium_shares_receipt_clock(&config), "{url}");
        }
    }

    #[test]
    fn receipt_clock_rejects_unknown_or_remote_hosts() {
        for url in [
            "",
            "user=academy",
            "host=localhost user=academy",
            "host=192.0.2.42 user=academy",
            "host=db.example user=academy",
            "postgres://academy@localhost/academy?host=/run/postgresql",
            "host=127.0.0.1,192.0.2.42 user=academy",
            "host=192.0.2.42,/run/postgresql user=academy",
        ] {
            let config = url.parse().unwrap();
            assert!(!premium_shares_receipt_clock(&config), "{url}");
        }
    }

    #[test]
    fn receipt_clock_rejects_hostaddr_overrides_and_fallbacks() {
        for url in [
            "host=127.0.0.1 hostaddr=192.0.2.42 user=academy",
            "host=/run/postgresql hostaddr=192.0.2.42 user=academy",
            "host=127.0.0.1,::1 hostaddr=127.0.0.1,192.0.2.42 user=academy",
            "host=127.0.0.1,::1 hostaddr=192.0.2.42,::1 user=academy",
            "host=::1 hostaddr=2001:db8::42 user=academy",
            "hostaddr=192.0.2.42 user=academy",
            "host=127.0.0.1 hostaddr=127.0.0.1 user=academy",
            "host=/run/postgresql hostaddr=::1 user=academy",
            "postgres://academy@127.0.0.1/academy?hostaddr=192.0.2.42",
        ] {
            let config: tokio_postgres::Config = url.parse().unwrap();
            assert!(!config.get_hostaddrs().is_empty(), "{url}");
            assert!(!premium_shares_receipt_clock(&config), "{url}");
        }
    }
}
