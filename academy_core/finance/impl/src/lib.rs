use std::{path::Path, sync::Arc, time::Duration};

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_finance_contracts::{
    FinanceDownloadError, FinanceFeatureService, FinanceGetDownloadTokenError, FinanceListError,
    FinancialDocumentListQuery, FinancialDocumentListResult, invoice::FinanceInvoiceService,
};
use academy_di::Build;
use academy_models::{auth::AccessToken, user::UserId};
use academy_persistence_contracts::{Database, finance::FinancialDocumentRepository};
use academy_shared_contracts::jwt::{JwtService, VerifyJwtError};
use academy_utils::{static_value, trace_instrument};
use anyhow::Context;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::instrument;

pub mod coin;
pub mod invoice;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct FinanceFeatureServiceImpl<Db, Auth, Jwt, FinanceInvoice, DocumentRepo> {
    db: Db,
    auth: Auth,
    jwt: Jwt,
    finance_invoice: FinanceInvoice,
    document_repo: DocumentRepo,
    config: FinanceFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct FinanceFeatureConfig {
    pub vat_percent: Decimal,
    pub invoices_archive: Arc<Path>,
    pub credit_notes_archive: Arc<Path>,
    pub final_statements_archive: Arc<Path>,
    /// Number of years invoices, credit notes and final statements are kept,
    /// counted from the end of the calendar year in which they were issued.
    pub retention_years: u32,
    pub download_token_ttl: Duration,
}

impl<Db, Auth, Jwt, FinanceInvoice, DocumentRepo> FinanceFeatureService
    for FinanceFeatureServiceImpl<Db, Auth, Jwt, FinanceInvoice, DocumentRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Jwt: JwtService,
    FinanceInvoice: FinanceInvoiceService<Db::Transaction>,
    DocumentRepo: FinancialDocumentRepository<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn get_download_token(
        &self,
        token: &AccessToken,
    ) -> Result<String, FinanceGetDownloadTokenError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;

        let data = DownloadToken {
            sub: auth.user_id,
            aud: DownloadTokenAud,
        };
        let token = self.jwt.sign(data, self.config.download_token_ttl)?;

        Ok(token)
    }

    #[instrument(skip(self))]
    async fn download_invoice(
        &self,
        token: &str,
        invoice_number: u64,
    ) -> Result<Vec<u8>, FinanceDownloadError> {
        let DownloadToken { sub: user_id, .. } =
            self.jwt.verify(token).map_err(|err| match err {
                VerifyJwtError::Expired(_) | VerifyJwtError::Invalid => {
                    FinanceDownloadError::InvalidToken
                }
            })?;

        let mut txn = self.db.begin_transaction().await?;

        self.finance_invoice
            .get_invoice_pdf(&mut txn, Some(user_id), invoice_number)
            .await?
            .ok_or(FinanceDownloadError::NotFound)
    }

    #[instrument(skip(self))]
    async fn download_credit_note(
        &self,
        token: &str,
        year: i32,
        month: u32,
    ) -> Result<Vec<u8>, FinanceDownloadError> {
        let DownloadToken { sub: user_id, .. } =
            self.jwt.verify(token).map_err(|err| match err {
                VerifyJwtError::Expired(_) | VerifyJwtError::Invalid => {
                    FinanceDownloadError::InvalidToken
                }
            })?;

        let mut txn = self.db.begin_transaction().await?;

        self.finance_invoice
            .get_credit_note(&mut txn, user_id, year, month)
            .await?
            .ok_or(FinanceDownloadError::NotFound)
    }

    #[trace_instrument(skip(self))]
    async fn list_documents(
        &self,
        token: &AccessToken,
        FinancialDocumentListQuery {
            kind,
            search,
            pagination,
        }: FinancialDocumentListQuery,
    ) -> Result<FinancialDocumentListResult, FinanceListError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let total = self
            .document_repo
            .count(&mut txn, kind, search.clone())
            .await
            .context("Failed to count financial documents")?;

        let documents = self
            .document_repo
            .list(&mut txn, kind, search, pagination)
            .await
            .context("Failed to get financial documents from database")?;

        Ok(FinancialDocumentListResult { total, documents })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct DownloadToken {
    sub: UserId,
    aud: DownloadTokenAud,
}

static_value!(DownloadTokenAud("finance"));
