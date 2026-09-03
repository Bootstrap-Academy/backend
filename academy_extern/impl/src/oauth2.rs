use std::collections::HashMap;

use academy_di::Build;
use academy_extern_contracts::oauth2::{OAuth2ApiService, OAuth2ResolveCodeError};
use academy_models::{
    oauth2::{
        OAuth2AuthorizationCode, OAuth2AuthorizationRequest, OAuth2CodeVerifier, OAuth2Provider,
        OAuth2UserInfo,
    },
    url::Url,
};
use academy_utils::{Apply, trace_instrument};
use anyhow::{Context, anyhow};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, RequestTokenError, TokenResponse, TokenUrl, basic::BasicClient, reqwest,
};
use tracing::trace;

use crate::http::{HttpClient, USER_AGENT};

#[derive(Debug, Clone, Build, Default)]
pub struct OAuth2ApiServiceImpl {
    #[di(default)]
    http: HttpClient,
}

impl OAuth2ApiService for OAuth2ApiServiceImpl {
    #[trace_instrument(skip(self))]
    fn generate_auth_url(
        &self,
        provider: &OAuth2Provider,
        request: &OAuth2AuthorizationRequest,
    ) -> Url {
        let code_challenge = request.code_verifier.as_ref().map(pkce_challenge);

        let mut url = provider.auth_url.clone();
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &provider.client_id)
            .apply_if(!provider.scopes.is_empty(), |q| {
                let mut it = provider.scopes.iter();
                let mut scopes = it.next().unwrap().clone();
                for scope in it {
                    scopes.push(' ');
                    scopes.push_str(scope);
                }
                q.append_pair("scope", &scopes)
            })
            .append_pair("state", &request.state)
            .append_pair("redirect_uri", request.redirect_uri.as_str())
            .apply_map(code_challenge.as_deref(), |q, challenge| {
                q.append_pair("code_challenge", challenge)
                    .append_pair("code_challenge_method", "S256")
            })
            .finish();
        url
    }

    #[trace_instrument(skip(self))]
    async fn resolve_code(
        &self,
        provider: OAuth2Provider,
        code: OAuth2AuthorizationCode,
        redirect_url: Url,
        code_verifier: Option<OAuth2CodeVerifier>,
    ) -> Result<OAuth2UserInfo, OAuth2ResolveCodeError> {
        let client = BasicClient::new(ClientId::new(provider.client_id))
            .set_client_secret(ClientSecret::new(provider.client_secret.into_inner()))
            .set_auth_uri(AuthUrl::from_url(provider.auth_url.0))
            .set_token_uri(TokenUrl::from_url(provider.token_url.0))
            .set_redirect_uri(RedirectUrl::from_url(redirect_url.0));

        let http_client = make_http_client();

        // exchange the authorization code for an access token
        let response = client
            .exchange_code(AuthorizationCode::new(code.into_inner()))
            .apply_map(code_verifier, |req, code_verifier| {
                req.set_pkce_verifier(PkceCodeVerifier::new(code_verifier.into_inner()))
            })
            .request_async(&http_client)
            .await
            .map_err(|err| match err {
                RequestTokenError::ServerResponse(_) | RequestTokenError::Parse(_, _) => {
                    OAuth2ResolveCodeError::InvalidCode
                }
                err => anyhow!(err)
                    .context("Failed to exchange authorization code")
                    .into(),
            })?;

        // never log the access token: it grants access to the remote account
        let access_token = response.access_token().secret();
        trace!("exchanged authorization code for access token");

        // use the access token to fetch the remote user's id and name
        let userinfo = self
            .http
            .get(provider.userinfo_url.0)
            .bearer_auth(access_token)
            .send()
            .await
            .context("Failed to send request to fetch userinfo")?
            .error_for_status()
            .context("Fetch userinfo request returned an error")?
            .json::<HashMap<String, serde_json::Value>>()
            .await
            .context("Failed to deserialize userinfo")?;
        // log which fields the provider sent, never their values: the payload
        // is the remote user's profile
        trace!(fields = ?userinfo.keys().collect::<Vec<_>>(), "fetched userinfo");

        let id = match userinfo.get(&provider.userinfo_id_key) {
            Some(serde_json::Value::Number(id)) => Ok(id.to_string()),
            Some(serde_json::Value::String(id)) => Ok(id.to_owned()),
            Some(x) => Err(anyhow!("Invalid user id: {x}")),
            None => Err(anyhow!("User id missing")),
        }
        .context("Failed to get user id from userinfo")?
        .try_into()
        .map_err(|id| anyhow!("Failed to deserialize remote user id {id:?}"))?;

        let name = match userinfo.get(&provider.userinfo_name_key) {
            Some(serde_json::Value::String(name)) => Ok(name.clone()),
            Some(x) => Err(anyhow!("Invalid username: {x}")),
            None => Err(anyhow!("Username missing")),
        }
        .context("Failed to get username from userinfo")?
        .try_into()
        .map_err(|name| anyhow!("Failed to deserialize remote user name {name:?}"))?;

        Ok(OAuth2UserInfo { id, name })
    }
}

/// S256 code challenge (RFC 7636 §4.2) for the given verifier.
fn pkce_challenge(code_verifier: &OAuth2CodeVerifier) -> String {
    PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(
        code_verifier.clone().into_inner(),
    ))
    .as_str()
    .to_owned()
}

fn make_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(&*USER_AGENT)
        .build()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use academy_models::oauth2::OAuth2State;

    use super::*;

    const STATE: &str = "H0AWv6JCwGB47LTMSpiEfNzKZM01dtpH0WQTNzC1TShmwJRucrpAlIYGXvBnHt9K";
    const CODE_VERIFIER: &str = "Iyw2rE8URlNi2N1UyLxjwTQxHTPtYX1Fp2jZTFhWSjNAPtPmyBEcNn6X1TchQuLU";
    /// base64url(sha256(CODE_VERIFIER)), see RFC 7636 §4.2
    const CODE_CHALLENGE: &str = "PAMbuKboe3zlu_zvOeo_fDuTtLGFH4vQXor8KsLz-H8";
    const REDIRECT_URI: &str = "https://academy/oauth/callback";
    const REDIRECT_URI_ENCODED: &str = "https%3A%2F%2Facademy%2Foauth%2Fcallback";

    #[test]
    fn make_http_client() {
        super::make_http_client();
    }

    #[test]
    fn generate_auth_url_with_scopes() {
        // Arrange
        let provider = make_provider();

        let sut = OAuth2ApiServiceImpl::default();

        // Act
        let result = sut.generate_auth_url(&provider, &make_request(None));

        // Assert
        assert_eq!(
            result.as_str(),
            format!(
                "https://oauth2.provider/auth?response_type=code&client_id=the-client-id\
                 &scope=foo+bar+baz&state={STATE}&redirect_uri={REDIRECT_URI_ENCODED}"
            )
        );
    }

    #[test]
    fn generate_auth_url_without_scopes() {
        // Arrange
        let provider = OAuth2Provider {
            scopes: Vec::new(),
            ..make_provider()
        };

        let sut = OAuth2ApiServiceImpl::default();

        // Act
        let result = sut.generate_auth_url(&provider, &make_request(None));

        // Assert
        assert_eq!(
            result.as_str(),
            format!(
                "https://oauth2.provider/auth?response_type=code&client_id=the-client-id\
                 &state={STATE}&redirect_uri={REDIRECT_URI_ENCODED}"
            )
        );
    }

    #[test]
    fn generate_auth_url_with_pkce() {
        // Arrange
        let provider = OAuth2Provider {
            scopes: Vec::new(),
            ..make_provider()
        };

        let sut = OAuth2ApiServiceImpl::default();

        // Act
        let result = sut.generate_auth_url(
            &provider,
            &make_request(Some(CODE_VERIFIER.try_into().unwrap())),
        );

        // Assert
        assert_eq!(
            result.as_str(),
            format!(
                "https://oauth2.provider/auth?response_type=code&client_id=the-client-id\
                 &state={STATE}&redirect_uri={REDIRECT_URI_ENCODED}\
                 &code_challenge={CODE_CHALLENGE}&code_challenge_method=S256"
            )
        );
    }

    #[test]
    fn pkce_challenge_matches_rfc_7636_appendix_b() {
        // Arrange
        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
            .try_into()
            .unwrap();

        // Act
        let result = pkce_challenge(&code_verifier);

        // Assert
        assert_eq!(result, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    fn make_request(code_verifier: Option<OAuth2CodeVerifier>) -> OAuth2AuthorizationRequest {
        OAuth2AuthorizationRequest {
            state: OAuth2State::try_new(STATE).unwrap(),
            redirect_uri: REDIRECT_URI.parse().unwrap(),
            code_verifier,
        }
    }

    fn make_provider() -> OAuth2Provider {
        OAuth2Provider {
            name: "test".into(),
            client_id: "the-client-id".into(),
            client_secret: "the-client-secret".into(),
            auth_url: "https://oauth2.provider/auth".parse().unwrap(),
            token_url: "http://test".parse().unwrap(),
            userinfo_url: "http://test".parse().unwrap(),
            userinfo_id_key: String::new(),
            userinfo_name_key: String::new(),
            scopes: ["foo", "bar", "baz"].map(Into::into).into(),
            pkce: true,
        }
    }
}
