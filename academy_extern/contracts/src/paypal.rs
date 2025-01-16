use std::future::Future;

use academy_models::paypal::PaypalOrderId;
use thiserror::Error;

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

    /// Capture payment for the given order.
    fn capture_order(
        &self,
        order_id: &PaypalOrderId,
    ) -> impl Future<Output = Result<(), PaypalCaptureOrderError>> + Send;
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

    pub fn with_capture_order(mut self, order_id: PaypalOrderId, ok: bool) -> Self {
        self.expect_capture_order()
            .once()
            .with(mockall::predicate::eq(order_id))
            .return_once(move |_| {
                Box::pin(std::future::ready(
                    ok.then_some(()).ok_or(PaypalCaptureOrderError::Failed),
                ))
            });
        self
    }
}
