use std::future::Future;

use academy_core_auth_contracts::internal::AuthInternalAuthenticateError;
use academy_models::{
    email_address::EmailAddress,
    user::{UserComposite, UserId},
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait InternalService: Send + Sync + 'static {
    fn get_user(
        &self,
        token: &str,
        user_id: UserId,
    ) -> impl Future<Output = Result<UserComposite, InternalGetUserError>> + Send;

    fn get_user_by_email(
        &self,
        token: &str,
        email: EmailAddress,
    ) -> impl Future<Output = Result<UserComposite, InternalGetUserByEmailError>> + Send;
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
