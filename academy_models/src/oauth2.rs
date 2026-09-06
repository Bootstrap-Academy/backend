use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    macros::{id, nutype_string},
    url::Url,
    user::UserId,
};

id!(OAuth2LinkId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2Provider {
    pub name: OAuth2ProviderName,
    pub client_id: String,
    pub client_secret: OAuth2ProviderClientSecret,
    pub auth_url: Url,
    pub token_url: Url,
    pub userinfo_url: Url,
    pub userinfo_id_key: String,
    pub userinfo_name_key: String,
    pub scopes: Vec<String>,
    /// Whether the provider understands PKCE (RFC 7636). All providers we ship
    /// do, but a deployment can turn it off for a provider that does not.
    pub pkce: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2ProviderSummary {
    pub id: OAuth2ProviderId,
    pub name: OAuth2ProviderName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2Link {
    pub id: OAuth2LinkId,
    pub user_id: UserId,
    pub provider_id: OAuth2ProviderId,
    pub created_at: DateTime<Utc>,
    pub remote_user: OAuth2UserInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuth2UserInfo {
    pub id: OAuth2RemoteUserId,
    pub name: OAuth2RemoteUserName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2Login {
    pub provider_id: OAuth2ProviderId,
    pub code: OAuth2AuthorizationCode,
    pub redirect_uri: Url,
    pub code_verifier: Option<OAuth2CodeVerifier>,
}

/// Everything an authorization flow needs beyond the provider itself: the
/// `state` nonce that ties the provider's redirect back to the browser that
/// started the flow, the redirect URI both requests have to agree on and, for
/// providers supporting PKCE, the code verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2AuthorizationRequest {
    pub state: OAuth2State,
    pub redirect_uri: Url,
    pub code_verifier: Option<OAuth2CodeVerifier>,
}

/// An authorization flow that has been started but whose `state` has not come
/// back yet. Kept in the cache under the `state` nonce until the provider
/// redirects the browser back to us (or the TTL expires).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuth2PendingAuthorization {
    pub provider_id: OAuth2ProviderId,
    pub redirect_uri: Url,
    pub code_verifier: Option<OAuth2CodeVerifier>,
}

/// The authorize URL a client has to send the user agent to, together with the
/// `state` nonce it is expected to hand back afterwards.
#[derive(Clone, PartialEq, Eq)]
pub struct OAuth2AuthorizationUrl {
    pub state: OAuth2State,
    pub authorize_url: Url,
}

impl std::fmt::Debug for OAuth2AuthorizationUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // the authorize URL repeats the state nonce and the PKCE challenge in
        // its query string, so the whole value is treated as sensitive

        // debug: use default Debug implementation
        #[cfg(debug_assertions)]
        {
            f.debug_struct("OAuth2AuthorizationUrl")
                .field("state", &self.state)
                .field("authorize_url", &self.authorize_url)
                .finish()
        }

        // release: hide secrets
        #[cfg(not(debug_assertions))]
        {
            f.write_str("OAuth2AuthorizationUrl(<redacted>)")
        }
    }
}

/// What an OAuth2 provider returns to the redirect URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuth2Callback {
    pub state: OAuth2State,
    pub code: OAuth2AuthorizationCode,
}

nutype_string!(OAuth2ProviderId);
nutype_string!(OAuth2ProviderName);
nutype_string!(OAuth2ProviderClientSecret(sensitive));

nutype_string!(OAuth2AuthorizationCode(
    sensitive,
    validate(len_char_max = 256)
));

// Unguessable per-request nonce passed to the provider as `state` (RFC 6749
// §10.12) and handed back to us by the provider's redirect.
nutype_string!(OAuth2State(
    sensitive,
    validate(
        len_char_min = OAuth2State::LEN,
        len_char_max = OAuth2State::LEN
    )
));
impl OAuth2State {
    pub const LEN: usize = 64;
}

// PKCE code verifier (RFC 7636 §4.1): 43 to 128 unreserved characters.
nutype_string!(OAuth2CodeVerifier(
    sensitive,
    validate(len_char_min = 43, len_char_max = 128)
));
impl OAuth2CodeVerifier {
    /// Length of the verifiers we generate. Alphanumeric, so every character
    /// is part of the `unreserved` set RFC 7636 allows.
    pub const LEN: usize = 64;
}

nutype_string!(OAuth2RemoteUserId(validate(len_char_max = 256)));
nutype_string!(OAuth2RemoteUserName(validate(len_char_max = 256)));

nutype_string!(OAuth2RegistrationToken(
    sensitive,
    validate(
        len_char_min = OAuth2RegistrationToken::LEN,
        len_char_max = OAuth2RegistrationToken::LEN
    )
));
impl OAuth2RegistrationToken {
    pub const LEN: usize = 64;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuth2Registration {
    pub provider_id: OAuth2ProviderId,
    pub remote_user: OAuth2UserInfo,
}
