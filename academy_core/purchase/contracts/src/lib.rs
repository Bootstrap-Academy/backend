use academy_models::{
    auth::{AccessToken, InternalToken},
    purchase::{PurchaseAcceptance, PurchaseProduct, PurchaseStatus},
    user::UserId,
};
use std::future::Future;
use thiserror::Error;

pub use academy_models::purchase;

#[derive(Debug, Error)]
pub enum PurchaseError {
    #[error("Not found or not authorized")]
    NotFound,
    #[error("The offer changed, expired, or the required declarations are missing")]
    OfferRequired,
    #[error("A verified delivery address is required")]
    ContactRequired,
    #[error("Purchase not available")]
    Unavailable,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PurchaseFeatureService: Send + Sync + 'static {
    /// Typed limited-service entrypoints. The caller proves current learning
    /// authority; a new acceptance additionally requires the exact independently
    /// authorized claimant election enforced by the persistence boundary.
    fn retained_offer(
        &self,
        user: UserId,
        kind: &str,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn retained_accept(
        &self,
        user: UserId,
        acceptance: PurchaseAcceptance,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn retained_get(
        &self,
        user: UserId,
        id: uuid::Uuid,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    /// Current resources for an independently admitted limited learner. Reuses
    /// the actual free-refill and paid-period routines; does not buy or renew.
    fn retained_resources(
        &self,
        user: UserId,
    ) -> impl Future<Output = Result<serde_json::Value, PurchaseError>> + Send;
    /// Internal typed boundary: the caller has independently proved this
    /// recipient for retained rights. This never grants ordinary authority.
    fn recipient_document(
        &self,
        user: UserId,
        id: uuid::Uuid,
        kind: &str,
    ) -> impl Future<Output = Result<Vec<u8>, PurchaseError>> + Send;
    fn cash_offer(
        &self,
        token: &AccessToken,
        product: PurchaseProduct,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn cash_accept(
        &self,
        token: &AccessToken,
        acceptance: PurchaseAcceptance,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn cash_captured(
        &self,
        payment: academy_models::paypal::PaypalPayment,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn cash_fulfilled(
        &self,
        payment: academy_models::paypal::PaypalPayment,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn cash_receipt(
        &self,
        payment: academy_models::paypal::PaypalPayment,
        invoice: Vec<u8>,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
    fn offer(
        &self,
        token: &AccessToken,
        kind: &str,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn external_offer(
        &self,
        token: &InternalToken,
        user: UserId,
        source: &str,
        product: PurchaseProduct,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn accept(
        &self,
        token: &AccessToken,
        acceptance: PurchaseAcceptance,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn external_accept(
        &self,
        token: &InternalToken,
        user: UserId,
        source: &str,
        acceptance: PurchaseAcceptance,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn get(
        &self,
        token: &AccessToken,
        id: uuid::Uuid,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn external_get(
        &self,
        token: &InternalToken,
        user: UserId,
        id: uuid::Uuid,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn external_complete(
        &self,
        token: &InternalToken,
        user: UserId,
        source: &str,
        id: uuid::Uuid,
        result: serde_json::Value,
    ) -> impl Future<Output = Result<PurchaseStatus, PurchaseError>> + Send;
    fn document(
        &self,
        token: &AccessToken,
        id: uuid::Uuid,
        kind: &str,
    ) -> impl Future<Output = Result<Vec<u8>, PurchaseError>> + Send;
    fn list(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<Vec<PurchaseStatus>, PurchaseError>> + Send;
    fn retry(&self) -> impl Future<Output = anyhow::Result<()>> + Send;
}
