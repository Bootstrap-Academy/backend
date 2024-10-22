use std::future::Future;

use academy_models::paypal::{PaypalCoinOrder, PaypalOrderId};
use chrono::{DateTime, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PaypalRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Create a new coin order.
    fn create_coin_order(
        &self,
        txn: &mut Txn,
        order: &PaypalCoinOrder,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the coin order with the given id.
    fn get_coin_order(
        &self,
        txn: &mut Txn,
        order_id: &PaypalOrderId,
    ) -> impl Future<Output = anyhow::Result<Option<PaypalCoinOrder>>> + Send;

    /// Capture the coin order with the given id.
    fn capture_coin_order(
        &self,
        txn: &mut Txn,
        order_id: &PaypalOrderId,
        captured_at: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return an invoice number which has not yet been used.
    fn get_next_invoice_number(
        &self,
        txn: &mut Txn,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockPaypalRepository<Txn> {
    pub fn with_create_coin_order(mut self, order: PaypalCoinOrder) -> Self {
        self.expect_create_coin_order()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(order))
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_get_coin_order(
        mut self,
        order_id: PaypalOrderId,
        result: Option<PaypalCoinOrder>,
    ) -> Self {
        self.expect_get_coin_order()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(order_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_capture_coin_order(
        mut self,
        order_id: PaypalOrderId,
        captured_at: DateTime<Utc>,
    ) -> Self {
        self.expect_capture_coin_order()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(order_id),
                mockall::predicate::eq(captured_at),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_get_next_invoice_number(mut self, result: u64) -> Self {
        self.expect_get_next_invoice_number()
            .once()
            .with(mockall::predicate::always())
            .return_once(move |_| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
