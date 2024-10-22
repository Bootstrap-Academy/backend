use std::{future::Future, ops::RangeInclusive};

use academy_models::{
    auth::{AccessToken, AuthError},
    coin::Balance,
    paypal::PaypalOrderId,
};
use thiserror::Error;

pub mod coin_order;

pub trait PaypalFeatureService: Send + Sync + 'static {
    /// Return the public PayPal client id.
    fn get_client_id(&self) -> &str;

    /// Create a new PayPal order to purchase the specified number of
    /// Morphcoins.
    ///
    /// Requires a verified email address.
    fn create_coin_order(
        &self,
        token: &AccessToken,
        coins: u64,
    ) -> impl Future<Output = Result<PaypalOrderId, PaypalCreateCoinOrderError>> + Send;

    /// Complete Morphcoin purchase.
    ///
    /// Requires a verified email address.
    fn capture_coin_order(
        &self,
        token: &AccessToken,
        order_id: PaypalOrderId,
    ) -> impl Future<Output = Result<Balance, PaypalCaptureCoinOrderError>> + Send;
}

#[derive(Debug, Error)]
pub enum PaypalCreateCoinOrderError {
    #[error("The specified number of Morphcoins is outside of the allowed range.")]
    InvalidAmount(RangeInclusive<u64>),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The user's invoice info is incomplete.")]
    IncompleteInvoiceInfo,
    #[error("Failed to create the PayPal order.")]
    CreateOrderFailure,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum PaypalCaptureCoinOrderError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error("The order does not exist.")]
    NotFound,
    #[error("The user's invoice info is incomplete.")]
    IncompleteInvoiceInfo,
    #[error("Failed to capture the PayPal order.")]
    CaptureOrderFailure,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
