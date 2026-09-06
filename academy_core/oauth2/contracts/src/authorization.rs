use std::future::Future;

use academy_models::{
    oauth2::{OAuth2AuthorizationUrl, OAuth2PendingAuthorization, OAuth2ProviderId, OAuth2State},
    url::Url,
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait OAuth2AuthorizationService: Send + Sync + 'static {
    /// Start an authorization flow for the given provider.
    ///
    /// Generates the unguessable `state` nonce (RFC 6749 §10.12) and, for
    /// providers supporting it, a PKCE code verifier (RFC 7636), remembers
    /// both for a short time and returns the authorize URL the user agent has
    /// to be sent to.
    fn begin(
        &self,
        provider_id: OAuth2ProviderId,
        redirect_uri: Url,
    ) -> impl Future<Output = Result<OAuth2AuthorizationUrl, OAuth2AuthorizationServiceError>> + Send;

    /// Invalidate the given `state` and return the authorization it was issued
    /// for.
    ///
    /// A `state` can be redeemed exactly once; every later call returns
    /// `None`, as does an unknown or expired one.
    fn consume(
        &self,
        state: &OAuth2State,
    ) -> impl Future<Output = anyhow::Result<Option<OAuth2PendingAuthorization>>> + Send;
}

#[derive(Debug, Error)]
pub enum OAuth2AuthorizationServiceError {
    #[error("The provider does not exist.")]
    InvalidProvider,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl MockOAuth2AuthorizationService {
    pub fn with_begin(
        mut self,
        provider_id: OAuth2ProviderId,
        redirect_uri: Url,
        result: Result<OAuth2AuthorizationUrl, OAuth2AuthorizationServiceError>,
    ) -> Self {
        self.expect_begin()
            .once()
            .with(
                mockall::predicate::eq(provider_id),
                mockall::predicate::eq(redirect_uri),
            )
            .return_once(|_, _| Box::pin(std::future::ready(result)));
        self
    }

    pub fn with_consume(
        mut self,
        state: OAuth2State,
        result: Option<OAuth2PendingAuthorization>,
    ) -> Self {
        self.expect_consume()
            .once()
            .with(mockall::predicate::eq(state))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
