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
/// The emails are counted in buckets in the cache, each kind of code on its
/// own. Every bucket is updated atomically, so concurrent requests cannot slip
/// past the limit together, and a bucket that refuses an email does not count
/// it. The buckets are checked one after the other; a bucket checked earlier
/// keeps its count when a later one refuses. The order is chosen so that such
/// a count always lands in a bucket of the requester and never locks somebody
/// else out.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UserEmailLimitService: Send + Sync + 'static {
    /// Count a verification email for the given account to the given address,
    /// or refuse it.
    ///
    /// A mailbox whose shared budget is used up refuses before anything is
    /// counted. Then the buckets, in this order: the account together with the
    /// mailbox the address delivers to (cooldown and budget per address), the
    /// account (budget per account), and the mailbox across all accounts
    /// (budget per mailbox, above what one account can fill).
    fn reserve_verification(
        &self,
        user_id: UserId,
        email: &EmailAddress,
    ) -> impl Future<Output = Result<(), UserEmailLimitError>> + Send;

    /// Count a password reset request for the given address before it is
    /// known whether an account has it, or refuse it.
    ///
    /// The address is counted exactly as requested. The code only goes out
    /// when the requested address equals the stored one exactly, so only a
    /// request that reaches the owner can fill the bucket of the owner's
    /// address.
    fn reserve_password_reset_address(
        &self,
        email: &EmailAddress,
    ) -> impl Future<Output = Result<(), UserEmailLimitError>> + Send;

    /// Count a password reset email for the given account, or refuse it if the
    /// account has used up its budget for the window.
    fn reserve_password_reset_account(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = Result<(), UserEmailLimitError>> + Send;
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
    pub fn with_reserve_verification(
        mut self,
        user_id: UserId,
        email: EmailAddress,
        result: Result<(), Duration>,
    ) -> Self {
        self.expect_reserve_verification()
            .once()
            .with(
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(email),
            )
            .return_once(move |_, _| {
                Box::pin(std::future::ready(
                    result.map_err(UserEmailLimitError::Limited),
                ))
            });
        self
    }

    pub fn with_reserve_password_reset_address(
        mut self,
        email: EmailAddress,
        result: Result<(), Duration>,
    ) -> Self {
        self.expect_reserve_password_reset_address()
            .once()
            .with(mockall::predicate::eq(email))
            .return_once(move |_| {
                Box::pin(std::future::ready(
                    result.map_err(UserEmailLimitError::Limited),
                ))
            });
        self
    }

    pub fn with_reserve_password_reset_account(
        mut self,
        user_id: UserId,
        result: Result<(), Duration>,
    ) -> Self {
        self.expect_reserve_password_reset_account()
            .once()
            .with(mockall::predicate::eq(user_id))
            .return_once(move |_| {
                Box::pin(std::future::ready(
                    result.map_err(UserEmailLimitError::Limited),
                ))
            });
        self
    }
}
