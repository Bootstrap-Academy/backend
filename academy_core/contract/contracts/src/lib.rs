use std::{future::Future, net::IpAddr};

use academy_models::{
    auth::{AccessToken, AuthError},
    contract::{
        ContractCancellationType, ContractDeclarantName, ContractDeclaration,
        ContractDeclarationDetails, ContractDeclarationId, ContractDeclarationKind,
        ContractDesignation, ContractKind, ContractProcessingNote,
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

    /// Record that a declaration has been processed by hand.
    ///
    /// Requires admin privileges.
    fn set_declaration_processed(
        &self,
        token: &AccessToken,
        id: ContractDeclarationId,
        update: ContractDeclarationProcessingUpdate,
    ) -> impl Future<Output = Result<ContractDeclaration, ContractSetProcessedError>> + Send;
}

/// What an administrator records when a declaration has been processed.
///
/// A field that is not given leaves the stored value alone; `processed_at` is
/// always set to the time of the request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContractDeclarationProcessingUpdate {
    /// The end of the contract as it was confirmed to the declarant.
    pub effective_end: Option<DateTime<Utc>>,
    /// What was done.
    pub note: Option<ContractProcessingNote>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCancellationRequest {
    pub name: ContractDeclarantName,
    pub email: EmailAddress,
    pub contract: ContractKind,
    /// The contract as the declarant named it
    /// (§ 312k Abs. 2 S. 2 Nr. 2 BGB), for every kind of contract.
    pub contract_designation: Option<ContractDesignation>,
    pub cancellation_type: ContractCancellationType,
    pub details: ContractDeclarationDetails,
    pub requested_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractWithdrawalRequest {
    pub name: ContractDeclarantName,
    pub email: EmailAddress,
    pub contract: ContractKind,
    /// The contract or order as the declarant named it.
    pub contract_designation: Option<ContractDesignation>,
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

#[derive(Debug, Error)]
pub enum ContractSetProcessedError {
    #[error("The declaration does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
