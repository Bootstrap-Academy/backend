use std::{collections::HashSet, path::Path};

use academy_config::Config;
use academy_core_premium_contracts::premium::PremiumService;
use academy_di::Provide;
use academy_models::{
    admin_audit::ADMIN_AUDIT_LOG_RETENTION_MONTHS,
    finance::{FinancialDocumentKind, FinancialDocumentNumber, credit_note_issued_at},
    retention::retention_cutoff,
};
use academy_persistence_contracts::{
    Database, Transaction, admin_audit::AdminAuditRepository, contract::ContractRepository,
    finance::FinancialDocumentRepository, premium::PremiumRepository, session::SessionRepository,
};
use academy_persistence_postgres::{
    admin_audit::PostgresAdminAuditRepository, contract::PostgresContractRepository,
    finance::PostgresFinancialDocumentRepository, session::PostgresSessionRepository,
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
    /// Reconcile started PayPal payments and retry their invoice/confirmation work.
    RetryPaypalPayments,
    /// Show unresolved payments and legacy orders requiring provider reconciliation. Read-only.
    ListPaypalPayments,
    /// Retry up to 100 due account erasure deliveries (durable, per service).
    RetryUserDeletions,
    /// Retry due cancellation/withdrawal confirmations.
    RetryContractConfirmations,
    /// Show aggregate pending erasures and the oldest request per service. Read-only.
    ListUserDeletions,
}

impl TaskCommand {
    pub async fn invoke(self, config: Config) -> anyhow::Result<()> {
        match self {
            TaskCommand::PruneDatabase => prune_database(config).await,
            TaskCommand::PruneDocuments => prune_documents(config).await,
            TaskCommand::ListOrphanDocuments => list_orphan_documents(config).await,
            TaskCommand::RefreshPremium => refresh_premium(config).await,
            TaskCommand::RetryPaypalPayments => {
                use academy_core_paypal_contracts::PaypalFeatureService;
                let mut provider = Provider::from_config(&config).await?;
                let payments: types::PaypalFeature = provider.provide();
                payments.retry_payments().await
            }
            TaskCommand::RetryContractConfirmations => {
                use academy_core_contract_contracts::ContractFeatureService;
                let mut provider = Provider::from_config(&config).await?;
                let contracts: types::ContractFeature = provider.provide();
                contracts.retry_confirmations().await
            }
            TaskCommand::RetryUserDeletions => {
                let db = database::connect(&config.database).await?;
                let mut provider = crate::environment::ConfigProvider::new(&config)?;
                let services: types::MicroservicesApi = provider.provide();
                crate::deletions::retry(&db, &services).await
            }
            TaskCommand::ListUserDeletions => {
                let db = database::connect(&config.database).await?;
                let mut txn = db.begin_transaction().await?;
                for (service, count, oldest) in
                    academy_persistence_postgres::deletion::backlog(&mut txn).await?
                {
                    println!("{service}: pending={count} oldest={oldest}");
                }
                Ok(())
            }
            TaskCommand::ListPaypalPayments => list_paypal_payments(config).await,
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

    // A cancellation or withdrawal is kept as evidence until a claim out of
    // the declared contract is time-barred: three years, counted from the end
    // of the calendar year in which the declaration was received
    // (§ 195, § 199 Abs. 1 BGB). The number of years is configured as
    // `contract.retention_years`.
    let contract_repo = PostgresContractRepository;
    let declaration_cutoff = retention_cutoff(now, config.contract.retention_years)
        .context("Failed to determine the declaration retention cutoff")?;
    let pruned = contract_repo
        .delete_by_received_at(&mut txn, declaration_cutoff)
        .await
        .context("Failed to prune contract declarations")?;
    info!("Pruned {pruned} contract declarations received before {declaration_cutoff}.");

    let pruned = academy_persistence_postgres::premium::PostgresPremiumRepository
        .prune_renewal_evidence(&mut txn, declaration_cutoff)
        .await?;
    info!(
        "Pruned {pruned} inactive Premium renewal records after their evidence retention period."
    );

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

    // The record deletion and durable file-disposal queue commit together. A
    // rollback cannot leave a retained financial record pointing at an erased PDF.
    let records = document_repo
        .delete_issued_before(&mut txn, cutoff)
        .await
        .context("Failed to queue expired unheld documents")?;
    txn.commit().await?;
    let mut txn = db.begin_transaction().await?;
    let work = document_repo.pending_archive_disposals(&mut txn).await?;
    txn.commit().await?;
    let mut files = 0;
    for (number, kind) in work {
        let archive = match kind {
            FinancialDocumentKind::Invoice => &config.finance.invoices_archive,
            FinancialDocumentKind::CreditNote => &config.finance.credit_notes_archive,
            FinancialDocumentKind::FinalStatement => &config.finance.final_statements_archive,
        };
        if dispose_archive_item(&db, &document_repo, &fs, archive, &number, kind).await? {
            files += 1;
        }
    }

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
            && let Some(stem) = path.file_stem().and_then(|v| v.to_str())
        {
            let number =
                academy_models::finance::FinancialDocumentNumber::try_new(stem.to_owned())?;
            let mut txn = db.begin_transaction().await?;
            document_repo
                .observe_unrecorded_archive(&mut txn, &number, FinancialDocumentKind::CreditNote)
                .await?;
            txn.commit().await?;
            unrecorded += 1;
        }
    }

    info!(
        "Pruned {records} documents issued before {cutoff}: {files} archived files and \
         {unrecorded} old credit-note archive candidates observed for independent review (recorded/held originals kept)."
    );

    Ok(())
}

/// One existing queue item. Committed admission precedes the file call; a later
/// review can block a new admission but cannot revoke this committed authority.
async fn dispose_archive_item<D, R, F>(
    db: &D,
    document_repo: &R,
    fs: &F,
    archive: &Path,
    number: &FinancialDocumentNumber,
    kind: FinancialDocumentKind,
) -> anyhow::Result<bool>
where
    D: Database,
    R: FinancialDocumentRepository<D::Transaction>,
    F: FsService,
{
    let mut txn = db.begin_transaction().await?;
    let admitted = document_repo
        .begin_archive_disposal(&mut txn, number, kind)
        .await?;
    txn.commit().await?;
    if !admitted {
        return Ok(false);
    }
    let path = archive.join(format!("{}.pdf", number.as_str()));
    let removed = fs
        .delete_file(&path)
        .await
        .with_context(|| format!("Failed to remove {}", path.display()))?;
    // An absent file is also an observed completed attempt. Do not conceal this
    // history if a new review appeared after the earlier committed admission.
    let mut txn = db.begin_transaction().await?;
    document_repo
        .acknowledge_archive_disposal(&mut txn, number, kind)
        .await?;
    txn.commit().await?;
    Ok(removed)
}

#[cfg(test)]
mod disposal_tests;

/// List the archived documents that no record refers to, and the records whose
/// archived file is missing.
///
/// Pdf files of accounts that were deleted before `financial_documents`
/// existed have no record and are therefore never picked up by
/// `prune-documents`; a record whose pdf could not be rendered has no file.
/// This task only reports both, so that they can be dealt with by hand.
async fn list_orphan_documents(config: Config) -> anyhow::Result<()> {
    let db=database::connect(&config.database).await?;
    let document_repo=PostgresFinancialDocumentRepository;
    let fs=FsServiceImpl;
    let mut txn=db.begin_transaction().await?;
    let inventory=document_repo.archive_inventory(&mut txn).await?;
    txn.commit().await?;
    let archives=[
        (FinancialDocumentKind::Invoice,&config.finance.invoices_archive),
        (FinancialDocumentKind::CreditNote,&config.finance.credit_notes_archive),
        (FinancialDocumentKind::FinalStatement,&config.finance.final_statements_archive),
    ];
    let recorded=inventory.iter().filter(|(_,_,recorded,_)| *recorded)
        .map(|(number,kind,_,_)|(number.as_str().to_owned(),kind.as_str())).collect::<HashSet<_>>();
    let mut archived=HashSet::new();
    let mut orphan_files=0;
    for (kind,archive) in archives {
        for path in fs.list_files(archive).await.with_context(||format!("Failed to list {}",archive.display()))? {
            if path.extension().is_none_or(|extension|extension!="pdf") { continue; }
            let Some(number)=path.file_stem().and_then(|stem|stem.to_str()) else { continue; };
            let key=(number.to_owned(),kind.as_str());
            archived.insert(key.clone());
            if !recorded.contains(&key) {
                println!("{} (no financial record in {} namespace; original/evidence review required)",path.display(),kind.as_str());
                orphan_files+=1;
            }
        }
    }
    let mut unavailable=0;
    for (number,kind,recorded,database_original) in inventory {
        let has_file=archived.contains(&(number.as_str().to_owned(),kind.as_str()));
        if database_original && !recorded {
            println!("{} (authoritative invoice database original; financial record absent, ownership/retention review required)",number.as_str());
        }
        if !has_file {
            if database_original {
                println!("{} (invoice database original available; filesystem cache absent)",number.as_str());
            } else {
                println!("{} ({} recorded original unavailable in its archive namespace)",number.as_str(),kind.as_str());
                unavailable+=1;
            }
        }
    }
    info!("{orphan_files} files lack a matching namespace record; {unavailable} recorded originals unavailable. Nothing was changed.");
    Ok(())
}

async fn refresh_premium(config: Config) -> anyhow::Result<()> {
    let mut provider = Provider::from_config(&config).await?;

    use academy_core_premium_contracts::PremiumFeatureService;
    let feature: types::PremiumFeature = provider.provide();
    feature.retry_renewal_confirmations().await?;

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

async fn list_paypal_payments(config: Config) -> anyhow::Result<()> {
    let db = database::connect(&config.database).await?;
    let txn = db.begin_transaction().await?;
    // No names, addresses, access tokens or full provider payloads in operator output.
    for row in txn.txn().query(
        "SELECT order_id,user_id,started_at,attempts,capture_id,fulfilled_at,receipt_sent_at,receipt_attempts,last_error FROM paypal_payments WHERE started_at IS NOT NULL AND receipt_sent_at IS NULL ORDER BY started_at,order_id", &[]).await? {
        println!("order={} recipient={} started={:?} attempts={} capture={:?} fulfilled={:?} receipt={:?} receipt_attempts={} error={:?}",
            row.get::<_,String>("order_id"), row.get::<_,uuid::Uuid>("user_id"), row.get::<_,Option<chrono::DateTime<Utc>>>("started_at"),
            row.get::<_,i64>("attempts"), row.get::<_,Option<String>>("capture_id"), row.get::<_,Option<chrono::DateTime<Utc>>>("fulfilled_at"),
            row.get::<_,Option<chrono::DateTime<Utc>>>("receipt_sent_at"), row.get::<_,i64>("receipt_attempts"), row.get::<_,Option<String>>("last_error"));
    }
    for row in txn.txn().query("SELECT order_id AS id,user_id,coins,invoice_number FROM paypal_legacy_reconciliation UNION SELECT o.id,o.user_id,o.coins,o.invoice_number FROM paypal_coin_orders o LEFT JOIN paypal_payments p ON p.order_id=o.id WHERE p.order_id IS NULL AND o.captured_at IS NULL ORDER BY id", &[]).await? {
        println!("legacy_unresolved order={} recipient={} coins={} invoice={} action=provider_and_ledger_reconciliation_required_no_automatic_capture",
            row.get::<_,String>("id"), row.get::<_,uuid::Uuid>("user_id"), row.get::<_,i64>("coins"), row.get::<_,i64>("invoice_number"));
    }
    txn.commit().await
}
