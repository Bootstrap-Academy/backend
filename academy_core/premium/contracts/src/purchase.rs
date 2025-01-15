use std::future::Future;

use academy_models::{
    premium::{Premium, PremiumPlan},
    user::UserId,
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PremiumPurchaseService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Purchase premium for the given user.
    fn purchase(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        plan: PremiumPlan,
    ) -> impl Future<Output = Result<Premium, PremiumPurchaseError>> + Send;
}

#[derive(Debug, Error)]
pub enum PremiumPurchaseError {
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockPremiumPurchaseService<Txn> {
    pub fn with_purchase(
        mut self,
        user_id: UserId,
        plan: PremiumPlan,
        result: Result<Premium, PremiumPurchaseError>,
    ) -> Self {
        self.expect_purchase()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(plan),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(result)));
        self
    }
}
