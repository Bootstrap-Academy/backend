use std::future::Future;

use academy_models::paypal::{PaypalOrderId, PaypalRemoteOrder};
use thiserror::Error;
use uuid::Uuid;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PaypalApiService: Send + Sync + 'static {
    /// Return the public PayPal client id.
    fn client_id(&self) -> &str;

    /// Create a new order for the given number of Morphcoins and return the
    /// order id.
    fn create_order(
        &self,
        coins: u64,
    ) -> impl Future<Output = Result<PaypalOrderId, PaypalCreateOrderError>> + Send;

    /// Read authoritative order identity, payee, amount and capture evidence.
    fn get_order(
        &self,
        order_id: &PaypalOrderId,
    ) -> impl Future<Output = anyhow::Result<PaypalRemoteOrder>> + Send;

    /// Capture payment with the persisted request UUID; a non-success remains ambiguous.
    fn capture_order(
        &self,
        order_id: &PaypalOrderId,
        request_id: Uuid,
    ) -> impl Future<Output = Result<PaypalRemoteOrder, PaypalCaptureOrderError>> + Send;
}

#[derive(Debug, Error)]
pub enum PaypalCreateOrderError {
    #[error("Failed to create order")]
    Failed,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum PaypalCaptureOrderError {
    #[error("Failed to capture order")]
    Failed,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl MockPaypalApiService {
    pub fn with_create_order(mut self, coins: u64, order_id: Option<PaypalOrderId>) -> Self {
        self.expect_create_order()
            .once()
            .with(mockall::predicate::eq(coins))
            .return_once(|_| {
                Box::pin(std::future::ready(
                    order_id.ok_or(PaypalCreateOrderError::Failed),
                ))
            });
        self
    }
}
