use std::future::Future;

use academy_models::{
    auth::Login,
    session::{ActiveUsersBucket, DeviceName, SessionId},
    user::{UserComposite, UserId},
};
use chrono::Duration;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveUsersRange {
    Day1,
    Day7,
    Day30,
    Day90,
}

impl ActiveUsersRange {
    pub fn duration(self) -> Duration {
        match self {
            Self::Day1 => Duration::days(1),
            Self::Day7 => Duration::days(7),
            Self::Day30 => Duration::days(30),
            Self::Day90 => Duration::days(90),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveUsersGranularity {
    Hour1,
    Day1,
    Day7,
    Day30,
}

impl ActiveUsersGranularity {
    pub fn duration(self) -> Duration {
        match self {
            Self::Hour1 => Duration::hours(1),
            Self::Day1 => Duration::days(1),
            Self::Day7 => Duration::days(7),
            Self::Day30 => Duration::days(30),
        }
    }
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SessionService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Create a new session for the given user.
    fn create(
        &self,
        txn: &mut Txn,
        user_composite: UserComposite,
        device_name: Option<DeviceName>,
        update_last_login: bool,
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

    /// Return the number of active users bucketed by the given range and
    /// granularity.
    fn active_users(
        &self,
        txn: &mut Txn,
        range: ActiveUsersRange,
        granularity: ActiveUsersGranularity,
    ) -> impl Future<Output = anyhow::Result<Vec<ActiveUsersBucket>>> + Send;
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
        result: Login,
    ) -> Self {
        self.expect_create()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_composite),
                mockall::predicate::eq(device_name),
                mockall::predicate::eq(update_last_login),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(Ok(result))));
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

    pub fn with_active_users(
        mut self,
        range: ActiveUsersRange,
        granularity: ActiveUsersGranularity,
        result: Vec<ActiveUsersBucket>,
    ) -> Self {
        self.expect_active_users()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(range),
                mockall::predicate::eq(granularity),
            )
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(result.clone()))));
        self
    }
}
