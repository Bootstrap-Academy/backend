use std::future::Future;

use academy_models::{
    VerificationCode,
    email_address::EmailAddressWithName,
    user::{UserComposite, UserId, UserPassword},
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UserEmailConfirmationService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Send a verification email to verify a user's email address.
    fn request_verification(
        &self,
        email: EmailAddressWithName,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Verify a user's email address.
    fn verify_email(
        &self,
        txn: &mut Txn,
        verification_code: &VerificationCode,
    ) -> impl Future<Output = Result<UserComposite, UserEmailConfirmationVerifyEmailError>> + Send;

    /// Send a verification email to reset a user's password.
    fn request_password_reset(
        &self,
        user_id: UserId,
        email: EmailAddressWithName,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Reset a user's password.
    fn reset_password(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        code: VerificationCode,
        new_password: UserPassword,
    ) -> impl Future<Output = Result<(), UserEmailConfirmationResetPasswordError>> + Send;

    /// Send a verification email to confirm a user's newsletter subscription.
    fn request_newsletter_subscription(
        &self,
        user_id: UserId,
        email: EmailAddressWithName,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Confirm a user's newsletter subscription.
    fn subscribe_to_newsletter(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        code: VerificationCode,
    ) -> impl Future<Output = Result<(), UserEmailConfirmationSubscribeToNewsletterError>> + Send;
}

#[derive(Debug, Error)]
pub enum UserEmailConfirmationVerifyEmailError {
    #[error("The verification code is invalid.")]
    InvalidCode,
    #[error("The user's email address has already been verified.")]
    AlreadyVerified,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserEmailConfirmationResetPasswordError {
    #[error("The verification code is invalid.")]
    InvalidCode,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserEmailConfirmationSubscribeToNewsletterError {
    #[error("The verification code is incorrect.")]
    InvalidCode,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockUserEmailConfirmationService<Txn> {
    pub fn with_request_verification(mut self, email: EmailAddressWithName) -> Self {
        self.expect_request_verification()
            .once()
            .with(mockall::predicate::eq(email))
            .return_once(|_| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_verify_email(
        mut self,
        verification_code: VerificationCode,
        result: Result<UserComposite, UserEmailConfirmationVerifyEmailError>,
    ) -> Self {
        self.expect_verify_email()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(verification_code),
            )
            .return_once(|_, _| Box::pin(std::future::ready(result)));
        self
    }

    pub fn with_request_password_reset(
        mut self,
        user_id: UserId,
        email: EmailAddressWithName,
    ) -> Self {
        self.expect_request_password_reset()
            .once()
            .with(
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(email),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_reset_password(
        mut self,
        user_id: UserId,
        code: VerificationCode,
        new_password: UserPassword,
        result: Result<(), UserEmailConfirmationResetPasswordError>,
    ) -> Self {
        self.expect_reset_password()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(code),
                mockall::predicate::eq(new_password),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(result)));
        self
    }

    pub fn with_request_newsletter_subscription(
        mut self,
        user_id: UserId,
        email: EmailAddressWithName,
    ) -> Self {
        self.expect_request_newsletter_subscription()
            .once()
            .with(
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(email),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_subscribe_to_newsletter(
        mut self,
        user_id: UserId,
        code: VerificationCode,
        result: Result<(), UserEmailConfirmationSubscribeToNewsletterError>,
    ) -> Self {
        self.expect_subscribe_to_newsletter()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(code),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(result)));
        self
    }
}
