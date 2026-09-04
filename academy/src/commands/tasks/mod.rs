use academy_config::Config;
use academy_core_premium_contracts::premium::PremiumService;
use academy_di::Provide;
use academy_models::finance::{FinancialDocumentKind, credit_note_issued_at, retention_cutoff};
use academy_persistence_contracts::{
    Database, Transaction, finance::FinancialDocumentRepository, premium::PremiumRepository,
    session::SessionRepository,
};
use academy_persistence_postgres::{
    finance::PostgresFinancialDocumentRepository, session::PostgresSessionRepository,
};
use academy_shared_contracts::fs::FsService;
use academy_shared_impl::fs::FsServiceImpl;
use anyhow::Context;
use chrono::Utc;
use clap::Subcommand;
use tracing::info;

use crate::{
    database,
    environment::{Provider, types},
};

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Remove expired records from the database.
    PruneDatabase,
    /// Remove invoices and credit notes whose retention period has expired.
    PruneDocuments,
    /// Refresh premium subscriptions.
    RefreshPremium,
}

impl TaskCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            TaskCommand::PruneDatabase => prune_database(config).await,
            TaskCommand::PruneDocuments => prune_documents(config).await,
            TaskCommand::RefreshPremium => refresh_premium(config).await,
        }
    }
}

async fn prune_database(config: Config) -> anyhow::Result<()> {
    let db = database::connect(&config.database).await?;
    let mut txn = db.begin_transaction().await?;

    let session_repo = PostgresSessionRepository;
    let now = Utc::now();
    let pruned = session_repo
        .delete_by_updated_at(&mut txn, now - config.session.refresh_token_ttl.0)
        .await
        .context("Failed to prune sessions")?;
    info!("Pruned {pruned} expired sessions.");

    txn.commit().await?;

    Ok(())
}

/// Delete invoices and credit notes whose retention period has expired.
///
/// The retention period ends eight years after the end of the calendar year in
/// which the document was issued (§ 147 Abs. 3 Satz 1 und Abs. 4 AO); the
/// number of years is configured as `finance.retention_years`.
async fn prune_documents(config: Config) -> anyhow::Result<()> {
    let db = database::connect(&config.database).await?;
    let document_repo = PostgresFinancialDocumentRepository;
    let fs = FsServiceImpl;

    let cutoff = retention_cutoff(Utc::now(), config.finance.retention_years)
        .context("Failed to determine the document retention cutoff")?;

    let mut txn = db.begin_transaction().await?;

    let expired = document_repo
        .list_issued_before(&mut txn, cutoff)
        .await
        .context("Failed to list expired documents")?;

    let mut files = 0;
    for document in &expired {
        let archive = match document.kind {
            FinancialDocumentKind::Invoice => &config.finance.invoices_archive,
            FinancialDocumentKind::CreditNote => &config.finance.credit_notes_archive,
        };
        let path = archive.join(format!("{}.pdf", *document.number));

        if fs
            .delete_file(&path)
            .await
            .with_context(|| format!("Failed to delete {}", path.display()))?
        {
            files += 1;
        }
    }

    let records = document_repo
        .delete_issued_before(&mut txn, cutoff)
        .await
        .context("Failed to delete expired documents")?;

    txn.commit().await?;

    // Credit notes that were archived before they were recorded in the
    // database have no record to prune, but their file name states the month
    // they cover.
    let mut unrecorded = 0;
    for path in fs
        .list_files(&config.finance.credit_notes_archive)
        .await
        .context("Failed to list archived credit notes")?
    {
        if path.extension().is_none_or(|extension| extension != "pdf") {
            continue;
        }

        let issued_at = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(credit_note_issued_at);

        if issued_at.is_some_and(|issued_at| issued_at < cutoff)
            && fs
                .delete_file(&path)
                .await
                .with_context(|| format!("Failed to delete {}", path.display()))?
        {
            unrecorded += 1;
        }
    }

    info!(
        "Pruned {records} documents issued before {cutoff}: {files} archived files and \
         {unrecorded} archived credit notes without a record."
    );

    Ok(())
}

async fn refresh_premium(config: Config) -> anyhow::Result<()> {
    let mut provider = Provider::from_config(&config).await?;

    let db: types::Database = provider.provide();
    let mut txn = db.begin_transaction().await?;

    let premium_repo: types::PremiumRepo = provider.provide();
    let premium: types::Premium = provider.provide();

    let user_ids = premium_repo.list_subscription_users(&mut txn).await?;
    for &user_id in &user_ids {
        premium.get_active(&mut txn, user_id).await?;
    }
    info!("Refreshed {} premium subscriptions", user_ids.len());

    txn.commit().await?;

    Ok(())
}
