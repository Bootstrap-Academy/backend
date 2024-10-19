use std::future::Future;

use academy_models::{coin::Balance, user::UserId};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait CoinRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the Morphcoin balance of the given user.
    fn get_balance(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<Balance>>> + Send;

    /// Update the Morphcoin balance of the given user.
    fn save_balance(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        balance: Balance,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockCoinRepository<Txn> {
    pub fn with_get_balance(mut self, user_id: UserId, result: Option<Balance>) -> Self {
        self.expect_get_balance()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_save_balance(mut self, user_id: UserId, balance: Balance) -> Self {
        self.expect_save_balance()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(balance),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
