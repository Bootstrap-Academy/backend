use std::future::Future;

use academy_auth_contracts::internal::AuthInternalAuthenticateError;
use academy_models::{
    auth::InternalToken,
    coin::{Balance, CoinOperation, TransactionDescription},
    email_address::EmailAddress,
    heart::{HeartOperation, HeartOperationReceipt, Hearts},
    user::{UserComposite, UserId},
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait InternalService: Send + Sync + 'static {
    /// Return the user with the given id.
    fn get_user(
        &self,
        token: &InternalToken,
        user_id: UserId,
    ) -> impl Future<Output = Result<UserComposite, InternalGetUserError>> + Send;

    /// Return the user with the given email address.
    fn get_user_by_email(
        &self,
        token: &InternalToken,
        email: EmailAddress,
    ) -> impl Future<Output = Result<UserComposite, InternalGetUserByEmailError>> + Send;

    /// Add Morphcoins to the balance of the given user.
    /// New negative operations require an ordinary recipient. Limited-service
    /// purchases use their owning exact-order acceptance; nonnegative credits
    /// retain the recipient's applicable verification and withholding rules.
    fn add_coins(
        &self,
        token: &InternalToken,
        user_id: UserId,
        coins: i64,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
    ) -> impl Future<Output = Result<Balance, InternalAddCoinsError>> + Send;

    /// Apply or replay a durable, immutable internal coin operation.
    /// Exact completed receipts are returned before current recipient lookup.
    /// New negative operations have the same ordinary-recipient scope as add_coins.
    fn apply_coin_operation(
        &self,
        token: &InternalToken,
        operation: CoinOperation,
    ) -> impl Future<Output = Result<Balance, InternalAddCoinsError>> + Send;

    /// Get hearts of the given user.
    fn get_hearts(
        &self,
        token: &InternalToken,
        user_id: UserId,
    ) -> impl Future<Output = Result<Hearts, InternalGetHeartsError>> + Send;

    /// Add hearts for the given user.
    fn add_hearts(
        &self,
        token: &InternalToken,
        user_id: UserId,
        hearts: i64,
    ) -> impl Future<Output = Result<Hearts, InternalAddHeartsError>> + Send;

    /// Apply one final incorrect attempt or replay its immutable receipt.
    fn apply_heart_operation(
        &self,
        token: &InternalToken,
        operation: HeartOperation,
    ) -> impl Future<Output = Result<HeartOperationReceipt, InternalHeartOperationError>> + Send;

    /// Return whether the given user is a premium member.
    fn has_premium(
        &self,
        token: &InternalToken,
        user_id: UserId,
    ) -> impl Future<Output = Result<bool, InternalHasPremiumError>> + Send;
}

#[derive(Debug, Error)]
pub enum InternalGetUserError {
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum InternalGetUserByEmailError {
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum InternalAddCoinsError {
    #[error("The operation id was already used with a different request.")]
    OperationConflict,
    #[error("The user does not exist.")]
    UserNotFound,
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum InternalGetHeartsError {
    #[error("The user does not exist.")]
    UserNotFound,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum InternalAddHeartsError {
    #[error("The user does not exist.")]
    UserNotFound,
    #[error("The user does not have enough hearts.")]
    NotEnoughHearts,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum InternalHeartOperationError {
    #[error("The operation id was already used with a different request.")]
    OperationConflict,
    #[error("Only a two-half-heart debit for an incorrect attempt is supported.")]
    InvalidRequest,
    #[error("The user does not exist.")]
    UserNotFound,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum InternalHasPremiumError {
    #[error("The user does not exist.")]
    UserNotFound,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
