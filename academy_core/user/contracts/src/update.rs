use std::future::Future;

use academy_models::{
    email_address::EmailAddress,
    user::{
        TermsVersion, User, UserId, UserInvoiceInfo, UserInvoiceInfoPatch, UserName, UserPassword,
    },
};
use chrono::{DateTime, Utc};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait UserUpdateService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Update a user's name.
    fn update_name(
        &self,
        txn: &mut Txn,
        user: User,
        name: UserName,
        rate_limit_policy: UserUpdateNameRateLimitPolicy,
    ) -> impl Future<Output = Result<User, UserUpdateNameError>> + Send;

    /// Update a user's email address.
    fn update_email(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        email: &Option<EmailAddress>,
        email_verified: bool,
    ) -> impl Future<Output = Result<bool, UserUpdateEmailError>> + Send;

    /// Update a user's password.
    fn update_password(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        password: UserPassword,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Update whether a user is enabled or not.
    fn update_enabled(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        enabled: bool,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Update whether a user is an administrator or not.
    fn update_admin(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        admin: bool,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Record that a user accepted the given version of the terms and
    /// conditions.
    ///
    /// `terms_accepted_at` is set to the current time. `age_confirmed_at` is
    /// only set if the user has not confirmed their age before, so that the
    /// original confirmation is never overwritten. A previously recorded
    /// refusal is cleared.
    fn accept_terms(
        &self,
        txn: &mut Txn,
        user: User,
        terms_version: TermsVersion,
    ) -> impl Future<Output = anyhow::Result<User>> + Send;

    /// Record that a user declined to accept the current version of the terms
    /// and conditions.
    ///
    /// `terms_declined_at` is set to the current time; the version the user
    /// accepted before is left untouched and keeps applying. Repeating the
    /// refusal is allowed and only moves the timestamp forward.
    fn decline_terms(
        &self,
        txn: &mut Txn,
        user: User,
    ) -> impl Future<Output = anyhow::Result<User>> + Send;

    /// Update a user's invoice information.
    fn update_invoice_info(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        invoice_info: UserInvoiceInfo,
        patch: UserInvoiceInfoPatch,
    ) -> impl Future<Output = anyhow::Result<UserInvoiceInfo>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserUpdateNameRateLimitPolicy {
    Enforce,
    Bypass,
}

#[derive(Debug, Error)]
pub enum UserUpdateNameError {
    #[error("The user cannot change their name until {until}.")]
    RateLimit { until: DateTime<Utc> },
    #[error("A user with the same name already exists.")]
    Conflict,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum UserUpdateEmailError {
    #[error("A user with the same email address already exists.")]
    Conflict,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockUserUpdateService<Txn> {
    pub fn with_update_name(
        mut self,
        user: User,
        name: UserName,
        rate_limit_policy: UserUpdateNameRateLimitPolicy,
        result: Result<User, UserUpdateNameError>,
    ) -> Self {
        self.expect_update_name()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user),
                mockall::predicate::eq(name),
                mockall::predicate::eq(rate_limit_policy),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(result)));
        self
    }

    pub fn with_update_email(
        mut self,
        user_id: UserId,
        email: EmailAddress,
        email_verified: bool,
        result: Result<bool, UserUpdateEmailError>,
    ) -> Self {
        self.expect_update_email()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(Some(email)),
                mockall::predicate::eq(email_verified),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(result)));
        self
    }

    pub fn with_update_password(mut self, user_id: UserId, password: UserPassword) -> Self {
        self.expect_update_password()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(password),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_update_enabled(mut self, user_id: UserId, enabled: bool, result: bool) -> Self {
        self.expect_update_enabled()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(enabled),
            )
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_update_admin(mut self, user_id: UserId, admin: bool, result: bool) -> Self {
        self.expect_update_admin()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(admin),
            )
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_accept_terms(
        mut self,
        user: User,
        terms_version: TermsVersion,
        result: User,
    ) -> Self {
        self.expect_accept_terms()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user),
                mockall::predicate::eq(terms_version),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_decline_terms(mut self, user: User, result: User) -> Self {
        self.expect_decline_terms()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(user))
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_update_invoice_info(
        mut self,
        user_id: UserId,
        invoice_info: UserInvoiceInfo,
        patch: UserInvoiceInfoPatch,
        result: UserInvoiceInfo,
    ) -> Self {
        self.expect_update_invoice_info()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(invoice_info),
                mockall::predicate::eq(patch),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
