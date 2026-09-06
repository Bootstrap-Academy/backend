use std::collections::HashSet;

use academy_config::Config;
use academy_core_premium_contracts::premium::PremiumService;
use academy_di::Provide;
use academy_models::{
    admin_audit::ADMIN_AUDIT_LOG_RETENTION_MONTHS,
    finance::{FinancialDocumentKind, credit_note_issued_at, retention_cutoff},
};
use academy_persistence_contracts::{
    Database, Transaction, admin_audit::AdminAuditRepository, finance::FinancialDocumentRepository,
    premium::PremiumRepository, session::SessionRepository,
};
use academy_persistence_postgres::{
    admin_audit::PostgresAdminAuditRepository, finance::PostgresFinancialDocumentRepository,
    session::PostgresSessionRepository,
};
use academy_shared_contracts::fs::FsService;
use academy_shared_impl::fs::FsServiceImpl;
use anyhow::{Context, anyhow};
use chrono::{Months, Utc};
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
    /// Remove the financial documents whose retention period has expired.
    PruneDocuments,
    /// List archived documents that are not recorded in the database and
    /// records whose archived file is missing. Changes nothing.
    ListOrphanDocuments,
    /// Refresh premium subscriptions.
    RefreshPremium,
}

impl TaskCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            TaskCommand::PruneDatabase => prune_database(config).await,
            TaskCommand::PruneDocuments => prune_documents(config).await,
            TaskCommand::ListOrphanDocuments => list_orphan_documents(config).await,
            TaskCommand::RefreshPremium => refresh_premium(config).await,
        }
    }
}

async fn prune_database(config: Config) -> anyhow::Result<()> {
    let db = database::connect(&config.database).await?;
    let mut txn = db.begin_transaction().await?;

    let now = Utc::now();

    let session_repo = PostgresSessionRepository;
    let pruned = session_repo
        .delete_by_updated_at(&mut txn, now - config.session.refresh_token_ttl.0)
        .await
        .context("Failed to prune sessions")?;
    info!("Pruned {pruned} expired sessions.");

    let admin_audit_repo = PostgresAdminAuditRepository;
    let audit_log_cutoff = now
        .checked_sub_months(Months::new(ADMIN_AUDIT_LOG_RETENTION_MONTHS))
        .ok_or_else(|| anyhow!("Failed to determine the audit log retention cutoff"))?;
    let pruned = admin_audit_repo
        .delete_by_at(&mut txn, audit_log_cutoff)
        .await
        .context("Failed to prune audit log entries")?;
    info!("Pruned {pruned} audit log entries older than {audit_log_cutoff}.");

    txn.commit().await?;

    Ok(())
}

/// Delete the invoices, credit notes and final statements whose retention
/// period has expired.
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
            FinancialDocumentKind::FinalStatement => &config.finance.final_statements_archive,
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

/// List the archived documents that no record refers to, and the records whose
/// archived file is missing.
///
/// Pdf files of accounts that were deleted before `financial_documents`
/// existed have no record and are therefore never picked up by
/// `prune-documents`; a record whose pdf could not be rendered has no file.
/// This task only reports both, so that they can be dealt with by hand.
async fn list_orphan_documents(config: Config) -> anyhow::Result<()> {
    let db = database::connect(&config.database).await?;
    let document_repo = PostgresFinancialDocumentRepository;
    let fs = FsServiceImpl;

    let mut txn = db.begin_transaction().await?;
    let numbers = document_repo
        .list_numbers(&mut txn)
        .await
        .context("Failed to list the recorded documents")?;
    txn.commit().await?;

    let archives = [
        &config.finance.invoices_archive,
        &config.finance.credit_notes_archive,
        &config.finance.final_statements_archive,
    ];

    let recorded = numbers
        .iter()
        .map(|number| (**number).as_str())
        .collect::<HashSet<_>>();
    let mut archived = HashSet::new();

    let mut orphan_files = 0;
    for archive in archives {
        for path in fs
            .list_files(archive)
            .await
            .with_context(|| format!("Failed to list {}", archive.display()))?
        {
            if path.extension().is_none_or(|extension| extension != "pdf") {
                continue;
            }

            let Some(number) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            archived.insert(number.to_owned());

            if !recorded.contains(number) {
                println!("{}", path.display());
                orphan_files += 1;
            }
        }
    }

    let mut missing_files = 0;
    for number in &numbers {
        if !archived.contains((**number).as_str()) {
            println!("{} (recorded, no archived file)", **number);
            missing_files += 1;
        }
    }

    info!(
        "{orphan_files} archived documents have no record and {missing_files} records have no \
         archived document. Nothing was changed."
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
