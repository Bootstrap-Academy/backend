use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    finance::{FinancialDocument, FinancialDocumentKind},
    pagination::PaginationSlice,
};
use thiserror::Error;

pub mod coin;
pub mod invoice;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinanceFeatureService: Send + Sync + 'static {
    /// Internal caller must prove the recipient; dedicated download audience
    /// cannot be used as ordinary account or microservice authority.
    fn recipient_download_token(
        &self,
        user: academy_models::user::UserId,
    ) -> impl Future<Output = Result<String, FinanceGetDownloadTokenError>> + Send;
    /// Return an existing owner-authorized original only. The internal caller
    /// must prove full recipient rights; this path never issues a new document.
    fn download_recipient_original(
        &self,
        user: academy_models::user::UserId,
        kind: FinancialDocumentKind,
        number: u64,
        month: u32,
    ) -> impl Future<Output = Result<Vec<u8>, FinanceDownloadError>> + Send;
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
    ) -> impl Future<Output = Result<Vec<u8>, FinanceDownloadError>> + Send;

    /// Download the given credit note pdf.
    fn download_credit_note(
        &self,
        token: &str,
        year: i32,
        month: u32,
    ) -> impl Future<Output = Result<Vec<u8>, FinanceDownloadError>> + Send;

    /// Return the issued financial documents, newest first.
    ///
    /// Requires admin privileges. Documents of deleted accounts are included;
    /// they are no longer linked to an account.
    fn list_documents(
        &self,
        token: &AccessToken,
        query: FinancialDocumentListQuery,
    ) -> impl Future<Output = Result<FinancialDocumentListResult, FinanceListError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinancialDocumentListQuery {
    pub kind: Option<FinancialDocumentKind>,
    /// Matches the document number and the recorded customer details.
    pub search: Option<String>,
    pub pagination: PaginationSlice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinancialDocumentListResult {
    pub total: u64,
    pub documents: Vec<FinancialDocument>,
}

#[derive(Debug, Error)]
pub enum FinanceListError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum FinanceGetDownloadTokenError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum FinanceDownloadError {
    #[error("The download token is invalid or has expired.")]
    InvalidToken,
    #[error("The invoice does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
