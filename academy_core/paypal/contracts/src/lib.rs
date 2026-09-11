use std::{future::Future, ops::RangeInclusive};

use academy_models::{
    auth::{AccessToken, AuthError},
    coin::Balance,
    paypal::PaypalOrderId,
    purchase::{PurchaseAcceptance, PurchaseStatus},
};
use thiserror::Error;

pub mod coin_order;

pub trait PaypalFeatureService: Send + Sync + 'static {
    fn offer_coin_order(
        &self,
        token: &AccessToken,
        coins: u64,
    ) -> impl Future<Output = Result<PurchaseStatus, PaypalCreateCoinOrderError>> + Send;
    /// Reconcile started payments and retry durable receipt work, isolating individual failures.
    fn retry_payments(&self) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the public PayPal client id.
    fn get_client_id(&self) -> &str;

    /// Create a new PayPal order to purchase the specified number of
    /// Morphcoins.
    ///
    /// Requires a verified email address and the declarations under
    /// § 356 Abs. 6 Nr. 2 BGB.
    fn create_coin_order(
        &self,
        token: &AccessToken,
        coins: u64,
        declaration: PurchaseAcceptance,
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
    #[error("Exact offer changed or unavailable; review the original order before a new purchase")]
    OfferChanged,
    #[error("The specified number of Morphcoins is outside of the allowed range.")]
    InvalidAmount(RangeInclusive<u64>),
    #[error("The user did not give the withdrawal declarations.")]
    WithdrawalConsentMissing,
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
    #[error(
        "Your payment is still being checked. Please retry this order later; do not place a second order."
    )]
    Pending,
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
