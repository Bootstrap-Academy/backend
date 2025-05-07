use std::time::Duration;

use academy_auth_contracts::{
    AuthService, AuthenticateByPasswordError, AuthenticateByRefreshTokenError, Authentication,
    Tokens, access_token::AuthAccessTokenService, refresh_token::AuthRefreshTokenService,
};
use academy_di::Build;
use academy_models::{
    auth::{AccessToken, AuthenticateError, RefreshToken},
    session::SessionId,
    user::{User, UserId, UserPassword},
};
use academy_persistence_contracts::{session::SessionRepository, user::UserRepository};
use academy_shared_contracts::{
    password::{PasswordService, PasswordVerifyError},
    time::TimeService,
};
use academy_utils::trace_instrument;
use anyhow::Context;
use tracing::trace;

pub mod access_token;
pub mod internal;
pub mod refresh_token;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct AuthServiceImpl<Time, Password, UserRepo, SessionRepo, AuthAccessToken, AuthRefreshToken>
{
    time: Time,
    password: Password,
    user_repo: UserRepo,
    session_repo: SessionRepo,
    auth_access_token: AuthAccessToken,
    auth_refresh_token: AuthRefreshToken,
    config: AuthServiceConfig,
}

#[derive(Debug, Clone, Copy)]
pub struct AuthServiceConfig {
    pub access_token_ttl: Duration,
    pub refresh_token_ttl: Duration,
    pub refresh_token_length: usize,
    pub internal_token_ttl: Duration,
}

impl<Txn, Time, Password, UserRepo, SessionRepo, AuthAccessToken, AuthRefreshToken> AuthService<Txn>
    for AuthServiceImpl<Time, Password, UserRepo, SessionRepo, AuthAccessToken, AuthRefreshToken>
where
    Txn: Send + Sync + 'static,
    Time: TimeService,
    Password: PasswordService,
    UserRepo: UserRepository<Txn>,
    SessionRepo: SessionRepository<Txn>,
    AuthAccessToken: AuthAccessTokenService,
    AuthRefreshToken: AuthRefreshTokenService,
{
    #[trace_instrument(skip(self))]
    async fn authenticate(&self, token: &AccessToken) -> Result<Authentication, AuthenticateError> {
        let auth = self
            .auth_access_token
            .verify(token)
            .ok_or(AuthenticateError::InvalidToken)?;

        if self
            .auth_access_token
            .is_invalidated(auth.refresh_token_hash)
            .await
            .context("Failed to check whether access token has been invalidated")?
        {
            trace!(?auth, "token invalidated");
            return Err(AuthenticateError::InvalidToken);
        }

        Ok(auth)
    }

    #[trace_instrument(skip(self, txn))]
    async fn authenticate_by_password(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        password: UserPassword,
    ) -> Result<(), AuthenticateByPasswordError> {
        let password_hash = self
            .user_repo
            .get_password_hash(txn, user_id)
            .await
            .context("Failed to get password hash from database")?
            .ok_or(AuthenticateByPasswordError::InvalidCredentials)
            .inspect_err(|_| trace!("no password set"))?;

        self.password
            .verify(password.into_inner().into(), password_hash)
            .await
            .map_err(|err| match err {
                PasswordVerifyError::InvalidPassword => {
                    trace!("wrong password");
                    AuthenticateByPasswordError::InvalidCredentials
                }
                PasswordVerifyError::Other(err) => {
                    err.context("Failed to verify password against hash").into()
                }
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn authenticate_by_refresh_token(
        &self,
        txn: &mut Txn,
        refresh_token: &RefreshToken,
    ) -> Result<SessionId, AuthenticateByRefreshTokenError> {
        let refresh_token_hash = self.auth_refresh_token.hash(refresh_token);

        let session = self
            .session_repo
            .get_by_refresh_token_hash(txn, refresh_token_hash)
            .await
            .context("Failed to get session from database")?
            .ok_or(AuthenticateByRefreshTokenError::Invalid)
            .inspect_err(|_| trace!("no session"))?;

        let now = self.time.now();
        if now >= session.updated_at + self.config.refresh_token_ttl {
            trace!("session expired");
            return Err(AuthenticateByRefreshTokenError::Expired(session.id));
        }

        Ok(session.id)
    }

    #[trace_instrument(skip(self))]
    fn issue_tokens(&self, user: &User, session_id: SessionId) -> anyhow::Result<Tokens> {
        let refresh_token = self.auth_refresh_token.issue();
        let refresh_token_hash = self.auth_refresh_token.hash(&refresh_token);
        let access_token = self
            .auth_access_token
            .issue(user, session_id, refresh_token_hash)
            .context("Failed to issue access token")?;

        Ok(Tokens {
            access_token,
            refresh_token,
            refresh_token_hash,
        })
    }

    #[trace_instrument(skip(self, txn))]
    async fn invalidate_access_tokens(&self, txn: &mut Txn, user_id: UserId) -> anyhow::Result<()> {
        for refresh_token_hash in self
            .session_repo
            .list_refresh_token_hashes_by_user(txn, user_id)
            .await
            .context("Failed to get session refresh token hashes from database")?
        {
            self.auth_access_token
                .invalidate(refresh_token_hash)
                .await
                .context("Failed to invalidate access token")?;
        }

        Ok(())
    }
}
