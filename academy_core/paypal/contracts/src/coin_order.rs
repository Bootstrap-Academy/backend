use std::future::Future;

use academy_models::{
    coin::Balance,
    paypal::{PaypalCoinOrder, PaypalOrderId},
    user::UserId,
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PaypalCoinOrderService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Create a new coin order.
    fn create(
        &self,
        txn: &mut Txn,
        id: PaypalOrderId,
        user_id: UserId,
        coins: u64,
    ) -> impl Future<Output = anyhow::Result<PaypalCoinOrder>> + Send;

    /// Mark a previously coin order as captured and add the Morphcoins to the
    /// user's balance.
    fn capture(
        &self,
        txn: &mut Txn,
        order: PaypalCoinOrder,
    ) -> impl Future<Output = anyhow::Result<Balance>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockPaypalCoinOrderService<Txn> {
    pub fn with_create(mut self, result: PaypalCoinOrder) -> Self {
        self.expect_create()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(result.id.clone()),
                mockall::predicate::eq(result.user_id),
                mockall::predicate::eq(result.coins),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_capture(mut self, order: PaypalCoinOrder, result: Balance) -> Self {
        self.expect_capture()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(order))
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
