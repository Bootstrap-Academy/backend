use std::{collections::HashMap, future::Future};

use academy_models::{
    auth::{AccessToken, AuthError},
    premium::{PremiumPlan, PremiumPlanDetails, PremiumStatus},
    user::UserIdOrSelf,
};
use thiserror::Error;

pub mod plan;
pub mod premium;
pub mod purchase;

pub trait PremiumFeatureService: Send + Sync + 'static {
    /// Return all available premium plans.
    fn get_plans(&self) -> HashMap<PremiumPlan, PremiumPlanDetails>;

    /// Return the premium status of the given user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn get_status(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<Option<PremiumStatus>, PremiumGetStatusError>> + Send;

    /// Purchase premium for the authenticated user using the given plan and
    /// optionally set up a subscription.
    ///
    /// Requires a verified email address.
    fn purchase(
        &self,
        token: &AccessToken,
        plan: PremiumPlan,
        subscribe: bool,
    ) -> impl Future<Output = Result<PremiumStatus, PremiumPurchaseError>> + Send;

    /// Update or cancel a premium subscription.
    ///
    /// Requires a verified email address.
    fn update_subscription(
        &self,
        token: &AccessToken,
        plan: Option<PremiumPlan>,
    ) -> impl Future<Output = Result<(), PremiumUpdateSubscriptionError>> + Send;
}

#[derive(Debug, Error)]
pub enum PremiumGetStatusError {
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum PremiumPurchaseError {
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum PremiumUpdateSubscriptionError {
    #[error("The user is not a premium member.")]
    NoPremium,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
