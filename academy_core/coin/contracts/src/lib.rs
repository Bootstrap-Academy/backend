use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    coin::Balance,
    user::UserIdOrSelf,
};
use thiserror::Error;

pub trait CoinFeatureService: Send + Sync + 'static {
    /// Return the Morphcoin balance of the given user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn get_balance(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<Balance, CoinGetBalanceError>> + Send;
}

#[derive(Debug, Error)]
pub enum CoinGetBalanceError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
