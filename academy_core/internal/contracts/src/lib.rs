use std::future::Future;

use academy_auth_contracts::internal::AuthInternalAuthenticateError;
use academy_models::{
    auth::InternalToken,
    coin::{Balance, TransactionDescription},
    email_address::EmailAddress,
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
    fn add_coins(
        &self,
        token: &InternalToken,
        user_id: UserId,
        coins: i64,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
    ) -> impl Future<Output = Result<Balance, InternalAddCoinsError>> + Send;
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
    #[error("The user does not exist.")]
    UserNotFound,
    #[error("The user does not have enough coins.")]
    NotEnoughCoins,
    #[error(transparent)]
    Auth(#[from] AuthInternalAuthenticateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
