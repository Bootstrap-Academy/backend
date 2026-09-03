use std::future::Future;

use academy_models::{
    auth::Login,
    session::{DeviceName, SessionId},
    user::{UserComposite, UserId},
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SessionService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Create a new session for the given user.
    ///
    /// `mfa_verified` records whether the second factor of the user was
    /// verified before the session was created. Only sessions created with a
    /// verified second factor grant administrative privileges.
    fn create(
        &self,
        txn: &mut Txn,
        user_composite: UserComposite,
        device_name: Option<DeviceName>,
        update_last_login: bool,
        mfa_verified: bool,
    ) -> impl Future<Output = anyhow::Result<Login>> + Send;

    /// Refresh the given session by invalidating the current access/refresh
    /// token pair and generating a new one.
    fn refresh(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
    ) -> impl Future<Output = Result<Login, SessionRefreshError>> + Send;

    /// Delete the given session and invalidate the current access/refresh token
    /// pair.
    fn delete(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Delete all sessions of the given user and invalidate all associated
    /// access/refresh token pairs.
    fn delete_by_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[derive(Debug, Error)]
pub enum SessionRefreshError {
    #[error("The session does not exist.")]
    NotFound,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockSessionService<Txn> {
    pub fn with_create(
        mut self,
        user_composite: UserComposite,
        device_name: Option<DeviceName>,
        update_last_login: bool,
        mfa_verified: bool,
        result: Login,
    ) -> Self {
        self.expect_create()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_composite),
                mockall::predicate::eq(device_name),
                mockall::predicate::eq(update_last_login),
                mockall::predicate::eq(mfa_verified),
            )
            .return_once(|_, _, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_refresh(
        mut self,
        session_id: SessionId,
        result: Result<Login, SessionRefreshError>,
    ) -> Self {
        self.expect_refresh()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(session_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(result)));
        self
    }

    pub fn with_delete(mut self, session_id: SessionId, result: bool) -> Self {
        self.expect_delete()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(session_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_delete_by_user(mut self, user_id: UserId) -> Self {
        self.expect_delete_by_user()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
