use std::future::Future;

use academy_models::{coin::Balance, user::UserId};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait CoinRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the Morphcoin balance of the given user.
    fn get_balance(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Balance>> + Send;

    /// Add Morphcoins to the balance of the given user.
    fn add_coins(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        coins: i64,
        withhold: bool,
    ) -> impl Future<Output = Result<Balance, CoinRepoAddCoinsError>> + Send;

    /// Release withheld coins.
    fn release_coins(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[derive(Debug, Error)]
pub enum CoinRepoAddCoinsError {
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockCoinRepository<Txn> {
    pub fn with_get_balance(mut self, user_id: UserId, result: Balance) -> Self {
        self.expect_get_balance()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_add_coins(
        mut self,
        user_id: UserId,
        coins: i64,
        withhold: bool,
        result: Result<Balance, CoinRepoAddCoinsError>,
    ) -> Self {
        self.expect_add_coins()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(coins),
                mockall::predicate::eq(withhold),
            )
            .return_once(move |_, _, _, _| Box::pin(std::future::ready(result)));
        self
    }

    pub fn with_release_coins(mut self, user_id: UserId) -> Self {
        self.expect_release_coins()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
