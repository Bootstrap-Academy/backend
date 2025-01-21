use std::collections::HashMap;

use academy_di::Build;
use academy_extern_contracts::oauth2::{OAuth2ApiService, OAuth2ResolveCodeError};
use academy_models::{
    oauth2::{OAuth2AuthorizationCode, OAuth2Provider, OAuth2UserInfo},
    url::Url,
};
use academy_utils::{trace_instrument, Apply};
use anyhow::{anyhow, Context};
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, RedirectUrl,
    RequestTokenError, TokenResponse, TokenUrl,
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
    fn generate_auth_url(&self, provider: &OAuth2Provider) -> Url {
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
            .finish();
        url
    }

    #[trace_instrument(skip(self))]
    async fn resolve_code(
        &self,
        provider: OAuth2Provider,
        code: OAuth2AuthorizationCode,
        redirect_url: Url,
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

        let access_token = response.access_token().secret();
        trace!(
            access_token,
            "exchanged authorization code for access token"
        );

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
        trace!(?userinfo, "fetched userinfo");

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

fn make_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(&*USER_AGENT)
        .build()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let result = sut.generate_auth_url(&provider);

        // Assert
        assert_eq!(result.as_str(), "https://oauth2.provider/auth?response_type=code&client_id=the-client-id&scope=foo+bar+baz");
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
        let result = sut.generate_auth_url(&provider);

        // Assert
        assert_eq!(
            result.as_str(),
            "https://oauth2.provider/auth?response_type=code&client_id=the-client-id"
        );
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
        }
    }
}
