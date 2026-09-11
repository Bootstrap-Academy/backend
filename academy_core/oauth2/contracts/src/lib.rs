use std::future::Future;

use academy_models::{
    auth::{AccessToken, AuthError, Login},
    oauth2::{
        OAuth2AuthorizationUrl, OAuth2Callback, OAuth2Link, OAuth2LinkId, OAuth2ProviderId,
        OAuth2ProviderSummary, OAuth2RegistrationToken,
    },
    session::DeviceName,
    url::Url,
    user::{UserId, UserIdOrSelf},
};
use thiserror::Error;

pub mod authorization;
pub mod link;
pub mod login;
pub mod registration;

pub trait OAuth2FeatureService: Send + Sync + 'static {
    /// Separate OAuth purpose; existing links only, no registration/link/session.
    fn begin_recipient(
        &self,
        provider: OAuth2ProviderId,
        redirect: Url,
    ) -> impl Future<Output = anyhow::Result<OAuth2AuthorizationUrl>> + Send;
    fn prove_recipient(
        &self,
        callback: OAuth2Callback,
    ) -> impl Future<Output = anyhow::Result<UserId>> + Send;

    /// Return all available OAuth2 providers.
    fn list_providers(&self) -> Vec<OAuth2ProviderSummary>;

    /// Start an OAuth2 authorization flow and return the authorize URL the
    /// user agent has to be sent to.
    ///
    /// The token decides what the resulting callback may be redeemed as: a
    /// flow started while signed in can only add a login method to that
    /// account ([`OAuth2FeatureService::create_link`]), a flow started without
    /// a usable token can only create a session
    /// ([`OAuth2FeatureService::create_session`]).
    ///
    /// The token is optional, and one that cannot be authenticated is treated
    /// like none: the endpoint is reached both by a signed-in account adding a
    /// login method and by a visitor signing in, and a client whose access
    /// token has expired has to be able to sign in with it.
    fn begin_authorization(
        &self,
        token: &AccessToken,
        provider_id: OAuth2ProviderId,
        redirect_uri: Url,
    ) -> impl Future<Output = Result<OAuth2AuthorizationUrl, OAuth2BeginAuthorizationError>> + Send;

    /// Return all OAuth2 links of the given user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn list_links(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> impl Future<Output = Result<Vec<OAuth2Link>, OAuth2ListLinksError>> + Send;

    /// Create a new OAuth2 for the given user.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn create_link(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        callback: OAuth2Callback,
    ) -> impl Future<Output = Result<OAuth2Link, OAuth2CreateLinkError>> + Send;

    /// Delete the given OAuth2 link.
    ///
    /// Requires admin privileges if not used on the authenticated user.
    fn delete_link(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        link_id: OAuth2LinkId,
    ) -> impl Future<Output = Result<(), OAuth2DeleteLinkError>> + Send;

    /// Create a session via OAuth2.
    fn create_session(
        &self,
        callback: OAuth2Callback,
        device_name: Option<DeviceName>,
    ) -> impl Future<Output = Result<OAuth2CreateSessionResponse, OAuth2CreateSessionError>> + Send;
}

#[derive(Debug, Error)]
pub enum OAuth2BeginAuthorizationError {
    #[error("The provider does not exist.")]
    InvalidProvider,
    #[error("The redirect uri is not allowed.")]
    InvalidRedirectUri,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum OAuth2ListLinksError {
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum OAuth2CreateLinkError {
    #[error("The authorization flow does not exist, has expired or has already been used.")]
    InvalidState,
    #[error("The provider does not exist.")]
    InvalidProvider,
    #[error("The authorization code is invalid.")]
    InvalidCode,
    #[error("The remote user has already been linked.")]
    RemoteAlreadyLinked,
    #[error("The user does not exist.")]
    NotFound,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum OAuth2DeleteLinkError {
    #[error("The link does not exist.")]
    NotFound,
    #[error(
        "The link cannot be removed from the user because they don't have any other login methods."
    )]
    CannotRemoveLink,
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OAuth2CreateSessionResponse {
    Login(Box<Login>),
    RegistrationToken(OAuth2RegistrationToken),
}

#[derive(Debug, Error)]
pub enum OAuth2CreateSessionError {
    #[error("The authorization flow does not exist, has expired or has already been used.")]
    InvalidState,
    #[error("The provider does not exist.")]
    InvalidProvider,
    #[error("The authorization code is invalid.")]
    InvalidCode,
    #[error("The user account has been disabled.")]
    UserDisabled,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
#[error("Existing recipient authorization is invalid, consumed, or unavailable")]
pub struct RecipientProofInvalid;
