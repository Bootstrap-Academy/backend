use academy_config::Config;
use academy_persistence_contracts::{Database, Transaction};
use academy_persistence_postgres::{
    MigrationStatus, PostgresDatabase, mfa::PostgresMfaRepository,
    oauth2::PostgresOAuth2Repository, session::PostgresSessionRepository,
    user::PostgresUserRepository,
};
use anyhow::Context;
use clap::Subcommand;
use load::LoadCommand;
use tracing::info;

use crate::database;

mod load;

#[derive(Debug, Subcommand)]
pub enum MigrateCommand {
    /// List all pending and applied migrations
    #[command(aliases(["status", "s", "l"]))]
    List,
    /// Apply all pending migrations
    #[command(aliases(["u"]))]
    Up {
        /// Only apply the next `n` migrations
        #[arg(short = 'n', long)]
        count: Option<usize>,
    },
    /// Revert the last migration
    #[command(aliases(["d"]))]
    Down {
        /// Revert the last `n` migrations
        #[arg(short = 'n', long, default_value = "1")]
        count: usize,
    },
    /// Reset the database and delete all data
    Reset {
        #[arg(long, required = true)]
        force: bool,
    },
    /// Reset the database and fill it with the demo dataset
    Demo {
        #[arg(long, required = true)]
        force: bool,
    },
    /// Import data from the old backend
    Load {
        #[command(subcommand)]
        command: LoadCommand,
    },
}

impl MigrateCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        let db = database::connect(&config.database).await?;
        match self {
            Self::List => list(db).await,
            Self::Up { count } => up(db, count).await,
            Self::Down { count } => down(db, Some(count)).await,
            Self::Reset { force: _ } => reset(db).await,
            Self::Demo { force: _ } => demo(db).await,
            Self::Load { command } => command.invoke(db).await,
        }
    }
}

async fn list(db: PostgresDatabase) -> anyhow::Result<()> {
    for MigrationStatus { migration, applied } in db.list_migrations().await? {
        if applied {
            println!("[applied] {}", migration.name);
        } else {
            println!("[pending] {}", migration.name);
        }
    }

    Ok(())
}

async fn up(db: PostgresDatabase, cnt: Option<usize>) -> anyhow::Result<()> {
    migration_logs(&db.run_migrations(cnt).await?, "applied");
    Ok(())
}

async fn down(db: PostgresDatabase, cnt: Option<usize>) -> anyhow::Result<()> {
    migration_logs(&db.revert_migrations(cnt).await?, "reverted");
    Ok(())
}

async fn reset(db: PostgresDatabase) -> anyhow::Result<()> {
    db.reset().await?;
    info!("Database reset successful");

    Ok(())
}

async fn demo(db: PostgresDatabase) -> anyhow::Result<()> {
    db.reset().await.context("Failed to reset database")?;
    info!("Database reset successful");
    migration_logs(
        &db.run_migrations(None)
            .await
            .context("Failed to run migrations")?,
        "applied",
    );

    let mut txn = db.begin_transaction().await?;
    academy_demo::create(
        &mut txn,
        PostgresUserRepository,
        PostgresSessionRepository,
        PostgresMfaRepository,
        PostgresOAuth2Repository,
    )
    .await
    .context("Failed to restore demo dataset")?;
    txn.commit().await?;
    info!("Demo dataset has been restored");

    Ok(())
}

fn migration_logs(logs: &[&str], action: &str) {
    let mut none = true;
    for &name in logs {
        info!("{action} {name}");
        none = false;
    }
    if none {
        info!("No migrations have been {action}.");
    }
}
