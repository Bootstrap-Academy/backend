use std::{future::Future, net::IpAddr};

use academy_models::{
    auth::{AccessToken, AuthError},
    contract::{
        ContractCancellationType, ContractDeclarantName, ContractDeclaration,
        ContractDeclarationDetails, ContractDeclarationKind, ContractKind,
    },
    email_address::EmailAddress,
    pagination::PaginationSlice,
};
use chrono::{DateTime, Utc};
use thiserror::Error;

pub trait ContractFeatureService: Send + Sync + 'static {
    /// Declare the cancellation of a contract (§ 312k BGB).
    ///
    /// Does not require authentication.
    fn declare_cancellation(
        &self,
        client_ip: IpAddr,
        request: ContractCancellationRequest,
    ) -> impl Future<Output = Result<ContractDeclarationResult, ContractDeclareError>> + Send;

    /// Declare the withdrawal from a contract (§ 356a BGB).
    ///
    /// Does not require authentication.
    fn declare_withdrawal(
        &self,
        client_ip: IpAddr,
        request: ContractWithdrawalRequest,
    ) -> impl Future<Output = Result<ContractDeclarationResult, ContractDeclareError>> + Send;

    /// Return all contract declarations matching the given query.
    ///
    /// Requires admin privileges.
    fn list_declarations(
        &self,
        token: &AccessToken,
        query: ContractDeclarationListQuery,
    ) -> impl Future<Output = Result<ContractDeclarationListResult, ContractListError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCancellationRequest {
    pub name: ContractDeclarantName,
    pub email: EmailAddress,
    pub contract: ContractKind,
    pub cancellation_type: ContractCancellationType,
    pub details: ContractDeclarationDetails,
    pub requested_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractWithdrawalRequest {
    pub name: ContractDeclarantName,
    pub email: EmailAddress,
    pub contract: ContractKind,
    pub details: ContractDeclarationDetails,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDeclarationResult {
    pub declaration: ContractDeclaration,
    /// Whether the confirmation email has been sent to the declarant.
    ///
    /// The declaration is stored regardless of any email trouble.
    pub confirmation_email_sent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContractDeclarationListQuery {
    pub kind: Option<ContractDeclarationKind>,
    pub pagination: PaginationSlice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDeclarationListResult {
    pub total: u64,
    pub declarations: Vec<ContractDeclaration>,
}

#[derive(Debug, Error)]
pub enum ContractDeclareError {
    #[error("Too many requests")]
    RateLimit,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum ContractListError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
