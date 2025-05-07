use std::future::Future;

use academy_models::{
    RecaptchaResponse,
    auth::{AccessToken, AuthError, Login, RefreshToken},
    mfa::MfaAuthentication,
    session::{DeviceName, Session, SessionId},
    user::{UserId, UserIdOrSelf, UserNameOrEmailAddress, UserPassword},
};
use thiserror::Error;

pub mod failed_auth_count;
pub mod session;

pub trait SessionFeatureService: Send + Sync + 'static {
    /// Return the currently authenticated session.
    fn get_current_session(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<Session, SessionGetCurrentError>> + Send;

    /// Return all sessions of the given user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn list_by_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<Vec<Session>, SessionListByUserError>> + Send;

    /// Create a new session by authenticating via username/password and MFA (if
    /// enabled).
    fn create_session(
        &self,
        cmd: SessionCreateCommand,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> impl Future<Output = Result<Login, SessionCreateError>> + Send;

    /// Impersonate a user by creating a new session for them.
    ///
    /// Requires admin privileges.
    fn impersonate(
        &self,
        token: &AccessToken,
        user_id: UserId,
    ) -> impl Future<Output = Result<Login, SessionImpersonateError>> + Send;

    /// Refresh a session using a refresh token.
    ///
    /// This will generate a new access and refresh token pair and invalidate
    /// the previous one.
    fn refresh_session(
        &self,
        refresh_token: &RefreshToken,
    ) -> impl Future<Output = Result<Login, SessionRefreshError>> + Send;

    /// Delete the given session and invalidate the access and refresh tokens
    /// associated with it.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn delete_session(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        session_id: SessionId,
    ) -> impl Future<Output = Result<(), SessionDeleteError>> + Send;

    /// Delete the currently authenticated session and invalidate its access
    /// and refresh tokens.
    fn delete_current_session(
        &self,
        token: &AccessToken,
    ) -> impl Future<Output = Result<(), SessionDeleteCurrentError>> + Send;

    /// Delete all sessions of the given user and invalidate all access and
    /// refresh tokens associated with them.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn delete_by_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<(), SessionDeleteByUserError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCreateCommand {
    pub name_or_email: UserNameOrEmailAddress,
    pub password: UserPassword,
    pub mfa: MfaAuthentication,
    pub device_name: Option<DeviceName>,
}

#[derive(Debug, Error)]
pub enum SessionGetCurrentError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum SessionListByUserError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum SessionCreateError {
    #[error("The user does not exist or the password is incorrect.")]
    InvalidCredentials,
    #[error("The user has mfa enabled but no valid authentication was provided.")]
    MfaFailed,
    #[error("The user account has been disabled.")]
    UserDisabled,
    #[error("Invalid recaptcha response")]
    Recaptcha,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum SessionImpersonateError {
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum SessionRefreshError {
    #[error("The refresh token is invalid or has expired.")]
    InvalidRefreshToken,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum SessionDeleteError {
    #[error("The session does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum SessionDeleteCurrentError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum SessionDeleteByUserError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
