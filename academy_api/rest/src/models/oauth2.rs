use academy_models::{
    oauth2::{
        OAuth2AuthorizationCode, OAuth2AuthorizationUrl, OAuth2Callback, OAuth2Link, OAuth2LinkId,
        OAuth2ProviderId, OAuth2ProviderName, OAuth2ProviderSummary, OAuth2RemoteUserName,
        OAuth2State,
    },
    url::Url,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiOAuth2ProviderSummary {
    /// OAuth2 provider ID
    pub id: OAuth2ProviderId,
    /// Display name
    pub name: OAuth2ProviderName,
}

impl From<OAuth2ProviderSummary> for ApiOAuth2ProviderSummary {
    fn from(value: OAuth2ProviderSummary) -> Self {
        Self {
            id: value.id,
            name: value.name,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiOAuth2Link {
    /// OAuth2 link ID
    pub id: OAuth2LinkId,
    /// OAuth2 provider ID
    pub provider_id: OAuth2ProviderId,
    /// Display name of the remote user account
    pub display_name: OAuth2RemoteUserName,
}

impl From<OAuth2Link> for ApiOAuth2Link {
    fn from(value: OAuth2Link) -> Self {
        Self {
            id: value.id,
            provider_id: value.provider_id,
            display_name: value.remote_user.name,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApiOAuth2AuthorizationRequest {
    /// OAuth2 provider ID
    pub provider_id: OAuth2ProviderId,
    /// Redirect URI the provider is asked to send the user agent back to. The
    /// same URI is used for the token exchange, so it cannot change in
    /// between.
    pub redirect_uri: Url,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiOAuth2AuthorizationUrl {
    /// Remote authorize endpoint URL, ready to be opened. Includes `state`,
    /// `redirect_uri` and, for providers supporting PKCE, the code challenge.
    pub authorize_url: Url,
    /// The `state` nonce contained in `authorize_url`. Clients keep it for the
    /// duration of the flow and compare it against the `state` the provider
    /// hands back, so a callback from a flow the browser did not start is
    /// rejected.
    pub state: OAuth2State,
}

impl From<OAuth2AuthorizationUrl> for ApiOAuth2AuthorizationUrl {
    fn from(value: OAuth2AuthorizationUrl) -> Self {
        Self {
            authorize_url: value.authorize_url,
            state: value.state,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ApiOAuth2Callback {
    /// The `state` nonce the OAuth2 provider returned. It identifies the
    /// authorization flow and can be redeemed only once.
    pub state: OAuth2State,
    /// Authorization code returned by the OAuth2 provider
    pub code: OAuth2AuthorizationCode,
}

impl From<ApiOAuth2Callback> for OAuth2Callback {
    fn from(value: ApiOAuth2Callback) -> Self {
        Self {
            state: value.state,
            code: value.code,
        }
    }
}
