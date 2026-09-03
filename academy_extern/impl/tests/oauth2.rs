use std::str::FromStr;

use academy_extern_contracts::oauth2::{OAuth2ApiService, OAuth2ResolveCodeError};
use academy_extern_impl::oauth2::OAuth2ApiServiceImpl;
use academy_models::{
    oauth2::{
        OAuth2AuthorizationRequest, OAuth2CodeVerifier, OAuth2Provider, OAuth2State, OAuth2UserInfo,
    },
    url::Url,
};
use academy_utils::assert_matches;

const STATE: &str = "dqEnwjBiOOEZaRhBVoxeXqePAEnpxRhWFtMLYPCSBhZfPuMBhTvMSJVXfxTffXaW";
const CODE_VERIFIER: &str = "iQiEnKGGgVSUwoJifLfTeWfKgUVCqRAmyOVvxSofjqLnjeoZuUxxlgUozjQWQKAn";

#[tokio::test]
async fn oauth2_with_pkce() {
    let provider = get_provider();
    let sut = OAuth2ApiServiceImpl::default();

    let request = OAuth2AuthorizationRequest {
        state: OAuth2State::try_new(STATE).unwrap(),
        redirect_uri: redirect_url(),
        code_verifier: Some(OAuth2CodeVerifier::try_new(CODE_VERIFIER).unwrap()),
    };
    let url = sut.generate_auth_url(&provider, &request);

    // the testing provider rejects a code_verifier that does not match the
    // code_challenge this URL carries
    let code = authorize(&url).await;

    let result = sut
        .resolve_code(
            provider.clone(),
            code.as_str().try_into().unwrap(),
            redirect_url(),
            request.code_verifier.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        OAuth2UserInfo {
            id: "userid123".try_into().unwrap(),
            name: "theremoteusername".try_into().unwrap()
        }
    );

    let result = sut
        .resolve_code(
            provider,
            "invalidcode".try_into().unwrap(),
            redirect_url(),
            request.code_verifier,
        )
        .await;
    assert_matches!(result, Err(OAuth2ResolveCodeError::InvalidCode));
}

#[tokio::test]
async fn oauth2_wrong_code_verifier() {
    let provider = get_provider();
    let sut = OAuth2ApiServiceImpl::default();

    let url = sut.generate_auth_url(
        &provider,
        &OAuth2AuthorizationRequest {
            state: OAuth2State::try_new(STATE).unwrap(),
            redirect_uri: redirect_url(),
            code_verifier: Some(OAuth2CodeVerifier::try_new(CODE_VERIFIER).unwrap()),
        },
    );
    let code = authorize(&url).await;

    let result = sut
        .resolve_code(
            provider,
            code.as_str().try_into().unwrap(),
            redirect_url(),
            Some(OAuth2CodeVerifier::try_new("x".repeat(64)).unwrap()),
        )
        .await;
    assert_matches!(result, Err(OAuth2ResolveCodeError::InvalidCode));
}

#[tokio::test]
async fn oauth2_without_pkce() {
    let provider = OAuth2Provider {
        pkce: false,
        ..get_provider()
    };
    let sut = OAuth2ApiServiceImpl::default();

    let url = sut.generate_auth_url(
        &provider,
        &OAuth2AuthorizationRequest {
            state: OAuth2State::try_new(STATE).unwrap(),
            redirect_uri: redirect_url(),
            code_verifier: None,
        },
    );
    assert!(!url.as_str().contains("code_challenge"));

    let code = authorize(&url).await;

    let result = sut
        .resolve_code(
            provider,
            code.as_str().try_into().unwrap(),
            redirect_url(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        OAuth2UserInfo {
            id: "userid123".try_into().unwrap(),
            name: "theremoteusername".try_into().unwrap()
        }
    );
}

/// Log in at the testing provider using the given authorize URL and return the
/// authorization code it redirects back with.
async fn authorize(url: &Url) -> String {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();

    let form = [("id", "userid123"), ("name", "theremoteusername")];
    let response = client
        .post(url.0.clone())
        .form(&form)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let url = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .parse::<Url>()
        .unwrap();

    let state = url.query_pairs().find(|(k, _)| *k == "state").unwrap().1;
    assert_eq!(state, STATE);

    url.query_pairs()
        .find(|(k, _)| *k == "code")
        .unwrap()
        .1
        .into_owned()
}

fn get_provider() -> OAuth2Provider {
    let base_url = Url::from_str("http://localhost:8101").unwrap();

    OAuth2Provider {
        name: "test".into(),
        client_id: "client-id".into(),
        client_secret: "client-secret".into(),
        auth_url: base_url.join("oauth2/authorize").unwrap().into(),
        token_url: base_url.join("oauth2/token").unwrap().into(),
        userinfo_url: base_url.join("user").unwrap().into(),
        userinfo_id_key: "id".into(),
        userinfo_name_key: "name".into(),
        scopes: vec![],
        pkce: true,
    }
}

fn redirect_url() -> Url {
    Url::from_str("http://localhost/oauth2/callback").unwrap()
}
