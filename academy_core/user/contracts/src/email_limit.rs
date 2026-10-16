use std::{future::Future, time::Duration};

use academy_models::{email_address::EmailAddress, user::UserId};
use thiserror::Error;

/// Brake on the emails that carry a code: the email verification code and the
/// password reset code.
///
/// Both go to an address that anybody can point at somebody else's mailbox:
/// an account can set its own address to any address, and a password reset
/// names the address in the request. Without a limit such a mailbox could be
/// sent any number of emails.
///
/// The emails are counted per kind of code, once per target address and once
/// per account, in the cache. Every bucket is updated atomically, so concurrent
/// requests cannot slip past the limit together.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UserEmailLimitService: Send + Sync + 'static {
    /// Count an email with a code of the given kind to the given address, or
    /// refuse it if the address has received one within the cooldown or has
    /// used up its budget for the window.
    ///
    /// A refused email is not counted.
    fn reserve_address(
        &self,
        kind: UserEmailCodeKind,
        email: &EmailAddress,
    ) -> impl Future<Output = Result<(), UserEmailLimitError>> + Send;

    /// Count an email with a code of the given kind for the given account, or
    /// refuse it if the account has used up its budget for the window.
    ///
    /// A refused email is not counted.
    fn reserve_account(
        &self,
        kind: UserEmailCodeKind,
        user_id: UserId,
    ) -> impl Future<Output = Result<(), UserEmailLimitError>> + Send;
}

/// The kind of code an email carries. Each kind is counted on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserEmailCodeKind {
    /// The code that verifies an email address.
    Verification,
    /// The code that resets a password.
    PasswordReset,
}

#[derive(Debug, Error)]
pub enum UserEmailLimitError {
    /// No further email of this kind is sent before the given time has passed.
    #[error("Too many emails with a code")]
    Limited(Duration),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl MockUserEmailLimitService {
    pub fn with_reserve_address(
        mut self,
        kind: UserEmailCodeKind,
        email: EmailAddress,
        result: Result<(), Duration>,
    ) -> Self {
        self.expect_reserve_address()
            .once()
            .with(mockall::predicate::eq(kind), mockall::predicate::eq(email))
            .return_once(move |_, _| {
                Box::pin(std::future::ready(
                    result.map_err(UserEmailLimitError::Limited),
                ))
            });
        self
    }

    pub fn with_reserve_account(
        mut self,
        kind: UserEmailCodeKind,
        user_id: UserId,
        result: Result<(), Duration>,
    ) -> Self {
        self.expect_reserve_account()
            .once()
            .with(
                mockall::predicate::eq(kind),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| {
                Box::pin(std::future::ready(
                    result.map_err(UserEmailLimitError::Limited),
                ))
            });
        self
    }
}
