use std::future::Future;

use academy_models::auth::{AccessToken, AuthError};
use thiserror::Error;

pub mod coin;
pub mod invoice;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinanceFeatureService: Send + Sync + 'static {
    /// Return a short-lived token which can be used to download finance
    /// documents for the authenticated user.
    fn get_download_token(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<String, FinanceGetDownloadTokenError>> + Send;

    /// Download the given invoice pdf.
    fn download_invoice(
        &self,
        token: &str,
        invoice_number: u64,
    ) -> impl Future<Output = Result<Vec<u8>, FinanceDownloadInvoiceError>> + Send;
}

#[derive(Debug, Error)]
pub enum FinanceGetDownloadTokenError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum FinanceDownloadInvoiceError {
    #[error("The download token is invalid or has expired.")]
    InvalidToken,
    #[error("The invoice does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
