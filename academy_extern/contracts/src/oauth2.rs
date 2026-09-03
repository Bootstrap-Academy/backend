use std::future::Future;

use academy_models::{
    oauth2::{
        OAuth2AuthorizationCode, OAuth2AuthorizationRequest, OAuth2CodeVerifier, OAuth2Provider,
        OAuth2UserInfo,
    },
    url::Url,
};
use thiserror::Error;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait OAuth2ApiService: Send + Sync + 'static {
    /// Build the authorize URL for the given OAuth2 provider.
    ///
    /// The URL is complete: it carries the `state` nonce, the redirect URI and,
    /// if the request comes with a code verifier, the PKCE code challenge.
    fn generate_auth_url(
        &self,
        provider: &OAuth2Provider,
        request: &OAuth2AuthorizationRequest,
    ) -> Url;

    /// Try to resolve an authorization code and return the remote user
    /// information in case of success.
    fn resolve_code(
        &self,
        provider: OAuth2Provider,
        code: OAuth2AuthorizationCode,
        redirect_url: Url,
        code_verifier: Option<OAuth2CodeVerifier>,
    ) -> impl Future<Output = Result<OAuth2UserInfo, OAuth2ResolveCodeError>> + Send;
}

#[derive(Debug, Error)]
pub enum OAuth2ResolveCodeError {
    #[error("The authorization code is invalid.")]
    InvalidCode,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl MockOAuth2ApiService {
    pub fn with_generate_auth_url(
        mut self,
        provider: OAuth2Provider,
        request: OAuth2AuthorizationRequest,
        result: Url,
    ) -> Self {
        self.expect_generate_auth_url()
            .once()
            .with(
                mockall::predicate::eq(provider),
                mockall::predicate::eq(request),
            )
            .return_once(|_, _| result);
        self
    }

    pub fn with_resolve_code(
        mut self,
        provider: OAuth2Provider,
        code: OAuth2AuthorizationCode,
        redirect_url: Url,
        code_verifier: Option<OAuth2CodeVerifier>,
        result: Result<OAuth2UserInfo, OAuth2ResolveCodeError>,
    ) -> Self {
        self.expect_resolve_code()
            .once()
            .with(
                mockall::predicate::eq(provider),
                mockall::predicate::eq(code),
                mockall::predicate::eq(redirect_url),
                mockall::predicate::eq(code_verifier),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(result)));
        self
    }
}
