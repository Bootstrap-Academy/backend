use std::future::Future;

use academy_models::{
    session::{ActiveUsersBucket, Session, SessionId, SessionPatchRef, SessionRefreshTokenHash},
    user::UserId,
};
use chrono::{DateTime, Duration, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait SessionRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the session with the given id.
    fn get(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
    ) -> impl Future<Output = anyhow::Result<Option<Session>>> + Send;

    /// Return the session with the given refresh token hash.
    fn get_by_refresh_token_hash(
        &self,
        txn: &mut Txn,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> impl Future<Output = anyhow::Result<Option<Session>>> + Send;

    /// Return all sessions of a given user.
    fn list_by_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Vec<Session>>> + Send;

    /// Create a new session.
    fn create(
        &self,
        txn: &mut Txn,
        session: &Session,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Update an existing session.
    #[allow(clippy::needless_lifetimes, reason = "automock")]
    fn update<'a>(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
        patch: SessionPatchRef<'a>,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Delete a given session.
    fn delete(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Delete all sessions of a given user.
    fn delete_by_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return active user buckets for the provided range configuration.
    fn active_users(
        &self,
        txn: &mut Txn,
        start: DateTime<Utc>,
        bucket: Duration,
        bucket_count: i64,
    ) -> impl Future<Output = anyhow::Result<Vec<ActiveUsersBucket>>> + Send;

    /// Delete all sessions that have not been updated since `updated_at`.
    ///
    /// Returns the number of deleted sessions.
    fn delete_by_updated_at(
        &self,
        txn: &mut Txn,
        updated_at: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// Return all refresh token hashes of a given user.
    fn list_refresh_token_hashes_by_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Vec<SessionRefreshTokenHash>>> + Send;

    /// Return the refresh token hash of a given session.
    fn get_refresh_token_hash(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
    ) -> impl Future<Output = anyhow::Result<Option<SessionRefreshTokenHash>>> + Send;

    /// Save or update the refresh token hash for a given session.
    fn save_refresh_token_hash(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockSessionRepository<Txn> {
    pub fn with_get(mut self, session_id: SessionId, result: Option<Session>) -> Self {
        self.expect_get()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(session_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_get_by_refresh_token_hash(
        mut self,
        refresh_token_hash: SessionRefreshTokenHash,
        result: Option<Session>,
    ) -> Self {
        self.expect_get_by_refresh_token_hash()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(refresh_token_hash),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_list_by_user(mut self, user_id: UserId, result: Vec<Session>) -> Self {
        self.expect_list_by_user()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_create(mut self, session: Session) -> Self {
        self.expect_create()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(session),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_update(
        mut self,
        session_id: SessionId,
        patch: academy_models::session::SessionPatch,
        result: bool,
    ) -> Self {
        self.expect_update()
            .once()
            .withf(move |_, id, p| *id == session_id && p == &patch.as_ref())
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(result))));
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
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_active_users(
        mut self,
        start: DateTime<Utc>,
        bucket: Duration,
        bucket_count: i64,
        result: Vec<ActiveUsersBucket>,
    ) -> Self {
        self.expect_active_users()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(start),
                mockall::predicate::eq(bucket),
                mockall::predicate::eq(bucket_count),
            )
            .return_once(move |_, _, _, _| Box::pin(std::future::ready(Ok(result.clone()))));
        self
    }

    pub fn with_list_refresh_token_hashes_by_user(
        mut self,
        user_id: UserId,
        result: Vec<SessionRefreshTokenHash>,
    ) -> Self {
        self.expect_list_refresh_token_hashes_by_user()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_get_refresh_token_hash(
        mut self,
        session_id: SessionId,
        result: Option<SessionRefreshTokenHash>,
    ) -> Self {
        self.expect_get_refresh_token_hash()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(session_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_save_refresh_token_hash(
        mut self,
        session_id: SessionId,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> Self {
        self.expect_save_refresh_token_hash()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(session_id),
                mockall::predicate::eq(refresh_token_hash),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
