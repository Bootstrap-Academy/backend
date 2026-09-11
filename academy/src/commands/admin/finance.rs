use academy_config::Config;
use academy_models::finance::FinancialDocumentNumber;
use academy_persistence_contracts::{Database, Transaction, finance::FinancialDocumentRepository};
use academy_persistence_postgres::finance::PostgresFinancialDocumentRepository;
use anyhow::bail;
use clap::Subcommand;

use crate::database;

#[derive(Debug, Subcommand)]
pub enum AdminFinanceCommand {
    /// List recorded missing/orphan invoice evidence; never charge or grant coins.
    ReconcileInvoices,
    /// Archive original issued invoice bytes with a factual source reference.
    /// This does not assert a capture timestamp, current tax facts or ownership.
    RecordOriginalInvoice {
        number: String,
        file: std::path::PathBuf,
        source_reference: String,
    },
    /// Explain the replacement of timestamp-only closure by evidenced claim disposition
    #[command(aliases(["s"]))]
    Settle {
        /// Number of the document, e.g. `S1337`
        number: String,
    },
}

impl AdminFinanceCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::ReconcileInvoices => {
                let db = database::connect(&config.database).await?;
                let mut txn = db.begin_transaction().await?;
                println!(
                    "{}",
                    PostgresFinancialDocumentRepository
                        .invoice_reconciliation(&mut txn)
                        .await?
                );
                txn.commit().await
            }
            Self::RecordOriginalInvoice {
                number,
                file,
                source_reference,
            } => {
                let number = FinancialDocumentNumber::try_new(number)?;
                anyhow::ensure!(
                    number.starts_with('R') && source_reference.trim().len() >= 8,
                    "Invoice number and factual archive/source reference required"
                );
                let pdf = std::fs::read(file)?;
                anyhow::ensure!(
                    pdf.starts_with(b"%PDF-") && pdf.len() <= 20 * 1024 * 1024,
                    "Original PDF required (maximum 20 MiB)"
                );
                let db = database::connect(&config.database).await?;
                let mut txn = db.begin_transaction().await?;
                PostgresFinancialDocumentRepository
                    .record_original_invoice(&mut txn, &number, &pdf, &source_reference)
                    .await?;
                txn.commit().await?;
                println!(
                    "Original bytes archived; payment, capture and entitlement history unchanged."
                );
                Ok(())
            }
            AdminFinanceCommand::Settle { number } => {
                let number = FinancialDocumentNumber::try_new(number)?;
                bail!(
                    "Timestamp-only closure is no longer available for {}. Settlement execution is not available through this command or the current staff API. Preserve existing reservation, receipt and uncertainty records for review. Historical document stamps remain historical assertions; this command changed nothing.",
                    *number
                )
            }
        }
    }
}
