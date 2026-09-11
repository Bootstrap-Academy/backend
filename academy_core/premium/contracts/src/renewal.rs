use std::future::Future;

use crate::PremiumUpdateSubscriptionError;
use academy_models::{
    premium::{PremiumRenewalConsent, PremiumRenewalOffer},
    user::UserId,
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PremiumRenewalService: Send + Sync + 'static {
    fn offer(&self) -> PremiumRenewalOffer;
    fn enable(
        &self,
        user_id: UserId,
        consent: PremiumRenewalConsent,
    ) -> impl Future<Output = Result<(), PremiumUpdateSubscriptionError>> + Send;
    /// Retry saved confirmations independently from any coin debit.
    fn deliver_pending(&self) -> impl Future<Output = anyhow::Result<()>> + Send;
}
