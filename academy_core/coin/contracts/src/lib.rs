use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError},
    coin::{Balance, TransactionDescription},
    user::UserIdOrSelf,
};
use thiserror::Error;

pub mod coin;

pub trait CoinFeatureService: Send + Sync + 'static {
    /// Return the Morphcoin balance of the given user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn get_balance(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<Balance, CoinGetBalanceError>> + Send;

    /// Add Morphcoins to the balance of the given user.
    ///
    /// Requires admin privileges.
    fn add_coins(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        coins: i64,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
    ) -> impl Future<Output = Result<Balance, CoinAddCoinsError>> + Send;
}

#[derive(Debug, Error)]
pub enum CoinGetBalanceError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    UserNotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum CoinAddCoinsError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user does not exist.")]
    UserNotFound,
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
