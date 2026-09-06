use thiserror::Error;

use crate::{macros::nutype_string, session::Session, user::UserComposite};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub user_composite: UserComposite,
    pub session: Session,
    pub access_token: AccessToken,
    pub refresh_token: RefreshToken,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(transparent)]
    Authenticate(#[from] AuthenticateError),
    #[error(transparent)]
    Authorize(#[from] AuthorizeError),
}

#[derive(Debug, Error)]
pub enum AuthenticateError {
    #[error("The access token is invalid or has expired.")]
    InvalidToken,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum AuthorizeError {
    #[error("The user is not an administrator.")]
    Admin,
    #[error("The session of the administrator was not authenticated with a second factor.")]
    AdminMfa,
    #[error("The user's email address is not verified.")]
    EmailVerified,
}

nutype_string!(AccessToken(sensitive));
nutype_string!(RefreshToken(sensitive));
nutype_string!(InternalToken(sensitive));
