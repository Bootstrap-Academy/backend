use academy_config::Config;
use academy_models::finance::{FinancialDocumentKind, FinancialDocumentNumber};
use academy_persistence_contracts::{Database, Transaction, finance::FinancialDocumentRepository};
use academy_persistence_postgres::finance::PostgresFinancialDocumentRepository;
use anyhow::{Context, bail};
use chrono::Utc;
use clap::Subcommand;

use crate::database;

#[derive(Debug, Subcommand)]
pub enum AdminFinanceCommand {
    /// Record that the claim of a financial document has been paid out
    #[command(aliases(["s"]))]
    Settle {
        /// Number of the document, e.g. `S1337`
        number: String,
    },
}

impl AdminFinanceCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            AdminFinanceCommand::Settle { number } => settle(config, number).await,
        }
    }
}

/// Stamp a document as settled.
///
/// A final statement records the unused share of the purchased Morphcoins of
/// an account that has been deleted, which is refunded on request. The refund
/// itself is made by hand, and after the deletion there is no balance left
/// that would show it, so the document is stamped here once the money has been
/// sent. Every listing then shows that the claim is closed.
async fn settle(config: Config, number: String) -> anyhow::Result<()> {
    let number = FinancialDocumentNumber::try_new(number)
        .context("The given document number is not a valid document number")?;

    let db = database::connect(&config.database).await?;
    let document_repo = PostgresFinancialDocumentRepository;

    let mut txn = db.begin_transaction().await?;

    let Some(document) = document_repo.get(&mut txn, &number).await? else {
        bail!("There is no document with the number {}.", *number);
    };

    if document.kind != FinancialDocumentKind::FinalStatement {
        bail!(
            "{} is a {} and records no claim that could be settled.",
            *number,
            document.kind.as_str()
        );
    }

    if let Some(settled_at) = document.settled_at {
        bail!("{} has already been settled on {settled_at}.", *number);
    }

    let now = Utc::now();
    if !document_repo.settle(&mut txn, &number, now).await? {
        bail!("There is no document with the number {}.", *number);
    }

    txn.commit().await?;

    println!("{} has been settled on {now}.", *number);

    Ok(())
}
