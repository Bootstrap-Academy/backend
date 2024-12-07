use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    heart::{HeartConfig, Hearts},
    user::UserIdOrSelf,
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
    fn refill(
        &self,
        token: &AccessToken,
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
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
