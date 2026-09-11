use academy_models::{purchase::PurchaseStatus, user::UserId};
use serde_json::Value;
use std::future::Future;
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait ModerationRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Original claimant of the exact case, including closed/erased cases.
    fn commercial_case_subject(
        &self,
        txn: &mut Txn,
        case_id: uuid::Uuid,
    ) -> impl Future<Output = anyhow::Result<Option<UserId>>> + Send;
    /// The caller supplies a fresh transaction: the implementation establishes a
    /// repeatable-read, read-only snapshot before its first data query.
    fn commercial_document_inventory(
        &self,
        txn: &mut Txn,
        claimant: UserId,
    ) -> impl Future<Output = anyhow::Result<academy_models::commercial_document::DocumentInventory>>
    + Send;
    /// Exact original offer owner admitted by durable claimant ownership.
    fn commercial_purchase_owner(
        &self,
        txn: &mut Txn,
        claimant: UserId,
        offer: uuid::Uuid,
    ) -> impl Future<Output = Result<UserId, CommercialPurchaseReadError>> + Send;
    /// Narrow original-order observations; no current learning authority or mutation.
    fn commercial_purchase_status(
        &self,
        txn: &mut Txn,
        claimant: UserId,
        offer: uuid::Uuid,
    ) -> impl Future<Output = Result<PurchaseStatus, CommercialPurchaseReadError>> + Send;
    /// Independent commercial authority; fixed SQL dispatcher and recipient scope.
    fn commercial_operation(
        &self,
        txn: &mut Txn,
        operation: &str,
        actor: Option<UserId>,
        body: &Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
    /// Fixed owning-service operations; no caller-provided SQL or source routing.
    fn operation(
        &self,
        txn: &mut Txn,
        operation: &str,
        actor: Option<UserId>,
        body: &Value,
    ) -> impl Future<Output = anyhow::Result<Value>> + Send;
}

#[derive(Debug, thiserror::Error)]
#[error("Moderation command conflicts with current case state or required fields")]
pub struct ModerationConflict;

#[derive(Debug, thiserror::Error)]
pub enum CommercialPurchaseReadError {
    #[error("Original purchase not found or not owned")]
    NotFound,
    #[error("Known original purchase observations are unavailable")]
    Unavailable,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
