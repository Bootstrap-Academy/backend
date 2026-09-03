use std::future::Future;

use academy_models::{
    auth::AccessToken,
    session::{SessionId, SessionRefreshTokenHash},
    user::User,
};

use crate::Authentication;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait AuthAccessTokenService: Send + Sync + 'static {
    /// Generate a new access token for the given user and session.
    fn issue(
        &self,
        user: &User,
        session_id: SessionId,
        refresh_token_hash: SessionRefreshTokenHash,
        mfa_verified: bool,
    ) -> anyhow::Result<AccessToken>;

    /// Verify the given access token and return its content if it is valid.
    fn verify(&self, access_token: &AccessToken) -> Option<Authentication>;

    /// Manually invalidate a previously issued access token before it expires.
    fn invalidate(
        &self,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return whether an access token has been manually invalidated.
    fn is_invalidated(
        &self,
        refresh_token_hash: SessionRefreshTokenHash,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
}

#[cfg(feature = "mock")]
impl MockAuthAccessTokenService {
    pub fn with_issue(
        mut self,
        user: User,
        session_id: SessionId,
        refresh_token_hash: SessionRefreshTokenHash,
        mfa_verified: bool,
        result: AccessToken,
    ) -> Self {
        self.expect_issue()
            .once()
            .with(
                mockall::predicate::eq(user),
                mockall::predicate::eq(session_id),
                mockall::predicate::eq(refresh_token_hash),
                mockall::predicate::eq(mfa_verified),
            )
            .return_once(|_, _, _, _| Ok(result));
        self
    }

    pub fn with_verify(
        mut self,
        access_token: AccessToken,
        result: Option<Authentication>,
    ) -> Self {
        self.expect_verify()
            .once()
            .with(mockall::predicate::eq(access_token))
            .return_once(move |_| result);
        self
    }

    pub fn with_invalidate(mut self, refresh_token_hash: SessionRefreshTokenHash) -> Self {
        self.expect_invalidate()
            .once()
            .with(mockall::predicate::eq(refresh_token_hash))
            .return_once(|_| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_is_invalidated(
        mut self,
        refresh_token_hash: SessionRefreshTokenHash,
        result: bool,
    ) -> Self {
        self.expect_is_invalidated()
            .once()
            .with(mockall::predicate::eq(refresh_token_hash))
            .return_once(move |_| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
