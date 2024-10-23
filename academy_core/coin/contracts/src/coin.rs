use std::future::Future;

use academy_models::{
    coin::{Balance, TransactionDescription},
    user::UserId,
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait CoinService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Add Morphcoins to the given user's balance.
    fn add_coins(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        coins: i64,
        withhold: bool,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
    ) -> impl Future<Output = Result<Balance, CoinAddCoinsError>> + Send;
}

#[derive(Debug, Error)]
pub enum CoinAddCoinsError {
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockCoinService<Txn> {
    pub fn with_add_coins(
        mut self,
        user_id: UserId,
        coins: i64,
        withhold: bool,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
        result: Result<Balance, CoinAddCoinsError>,
    ) -> Self {
        self.expect_add_coins()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(coins),
                mockall::predicate::eq(withhold),
                mockall::predicate::eq(description),
                mockall::predicate::eq(include_in_credit_note),
            )
            .return_once(|_, _, _, _, _, _| Box::pin(std::future::ready(result)));
        self
    }
}
