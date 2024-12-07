use std::{path::Path, sync::Arc, time::Duration};

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_finance_contracts::{
    invoice::FinanceInvoiceService, FinanceDownloadError, FinanceFeatureService,
    FinanceGetDownloadTokenError,
};
use academy_di::Build;
use academy_models::{auth::AccessToken, user::UserId};
use academy_persistence_contracts::Database;
use academy_shared_contracts::jwt::{JwtService, VerifyJwtError};
use academy_utils::{static_value, trace_instrument};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::instrument;

pub mod coin;
pub mod invoice;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct FinanceFeatureServiceImpl<Db, Auth, Jwt, FinanceInvoice> {
    db: Db,
    auth: Auth,
    jwt: Jwt,
    finance_invoice: FinanceInvoice,
    config: FinanceFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct FinanceFeatureConfig {
    pub vat_percent: Decimal,
    pub invoices_archive: Arc<Path>,
    pub credit_notes_archive: Arc<Path>,
    pub download_token_ttl: Duration,
}

impl<Db, Auth, Jwt, FinanceInvoice> FinanceFeatureService
    for FinanceFeatureServiceImpl<Db, Auth, Jwt, FinanceInvoice>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Jwt: JwtService,
    FinanceInvoice: FinanceInvoiceService<Db::Transaction>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct DownloadToken {
    sub: UserId,
    aud: DownloadTokenAud,
}

static_value!(DownloadTokenAud("finance"));
