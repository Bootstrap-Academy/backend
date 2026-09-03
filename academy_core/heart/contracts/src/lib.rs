use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    heart::{HeartConfig, Hearts},
    user::UserIdOrSelf,
    withdrawal::WithdrawalConsentDeclaration,
};
use thiserror::Error;

pub mod heart;

pub trait HeartFeatureService: Send + Sync + 'static {
    /// Return the public heart configuration.
    fn get_config(&self) -> HeartConfig;

    /// Return the hearts of the given user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn get(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<Hearts, HeartGetError>> + Send;

    /// Manually refill hearts to maximum.
    ///
    /// Does nothing if the user already has the maximum number of hearts.
    ///
    /// Requires the declarations under § 356 Abs. 6 Nr. 2 BGB.
    fn refill(
        &self,
        token: &AccessToken,
        declaration: WithdrawalConsentDeclaration,
    ) -> impl Future<Output = Result<Hearts, HeartRefillError>> + Send;
}

#[derive(Debug, Error)]
pub enum HeartGetError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    UserNotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum HeartRefillError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error("The user did not give the withdrawal declarations.")]
    WithdrawalConsentMissing,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
