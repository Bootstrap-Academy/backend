use std::future::Future;

use crate::PremiumUpdateSubscriptionError;
use academy_models::{
    premium::{PremiumRenewalConsent, PremiumRenewalOffer},
    user::UserId,
};

#[derive(Debug, Clone, Copy)]
pub enum RenewalDocumentKind {
    Terms,
    Withdrawal,
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PremiumRenewalService: Send + Sync + 'static {
    fn offer(&self) -> PremiumRenewalOffer;
    fn offer_for(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<PremiumRenewalOffer, PremiumUpdateSubscriptionError>> + Send;
    fn document_for(
        &self,
        user_id: UserId,
        offer_id: &str,
        kind: RenewalDocumentKind,
    ) -> impl Future<Output = Result<Vec<u8>, PremiumUpdateSubscriptionError>> + Send;
    fn enable(
        &self,
        user_id: UserId,
        consent: PremiumRenewalConsent,
    ) -> impl Future<Output = Result<(), PremiumUpdateSubscriptionError>> + Send;
    /// Retry saved confirmations independently from any coin debit.
    fn deliver_pending(&self) -> impl Future<Output = anyhow::Result<()>> + Send;
}
