use std::{collections::HashMap, net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing, Form, Json, Router,
};
use axum_extra::{
    headers::{
        authorization::{Basic, Bearer},
        Authorization,
    },
    TypedHeader,
};
use oauth2::{PkceCodeChallenge, PkceCodeVerifier};
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{Mutex, RwLock},
};
use tracing::info;
use url::Url;

pub async fn start_server(
    host: IpAddr,
    port: u16,
    client_id: String,
    client_secret: String,
    redirect_url: Url,
) -> anyhow::Result<()> {
    info!("Starting OAuth2 testing server on {host}:{port}");
    info!("Authorization endpoint: http://{host}:{port}/oauth2/authorize");
    info!("Token endpoint: http://{host}:{port}/oauth2/token");
    info!("User info endpoint: http://{host}:{port}/user");
    info!("Client ID: {client_id:?}");
    info!("Client secret: {client_secret:?}");
    info!("Redirect url: {redirect_url}");
    info!("Example authorization URL: http://{host}:{port}/oauth2/authorize?response_type=code&client_id={client_id}&state=test-state&redirect_uri={redirect_url}");

    let router = Router::new()
        .route("/oauth2/authorize", routing::get(authorize).post(login))
        .route("/oauth2/token", routing::post(token))
        .route("/user", routing::get(user))
        .with_state(Arc::new(StateInner {
            client_id,
            client_secret,
            redirect_url,
            codes: Default::default(),
            logins: Default::default(),
        }));

    let listener = TcpListener::bind((host, port))
        .await
        .with_context(|| format!("Failed to bind to {host}:{port}"))?;
    axum::serve(listener, router)
        .await
        .context("Failed to start HTTP server")
}

type State = axum::extract::State<Arc<StateInner>>;
struct StateInner {
    client_id: String,
    client_secret: String,
    redirect_url: Url,
    codes: Mutex<HashMap<String, CodeState>>,
    logins: RwLock<HashMap<String, Login>>,
}

struct CodeState {
    login: Login,
    code_challenge: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorizeQuery {
    response_type: String,
    client_id: String,
    state: String,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    redirect_uri: Url,
}

async fn authorize(state: State, Query(query): Query<AuthorizeQuery>) -> Response {
    if query.response_type != "code" {
        return (StatusCode::BAD_REQUEST, "invalid response_type").into_response();
    }
    if query.code_challenge_method.is_some_and(|x| x != "S256") {
        return (StatusCode::BAD_REQUEST, "invalid code_challenge_method").into_response();
    }

    if query.client_id != state.client_id {
        return (StatusCode::BAD_REQUEST, "invalid client_id").into_response();
    }
    if query.redirect_uri != state.redirect_url {
        return (StatusCode::BAD_REQUEST, "invalid redirect_uri").into_response();
    }

    Html(
        "<form method=post>
        ID: <input name=id autofocus><br>
        Name: <input name=name><br>
        <button type=submit>Login</button>
        </form>",
    )
    .into_response()
}

#[derive(Serialize, Deserialize)]
struct Login {
    id: String,
    name: String,
}

async fn login(
    state: State,
    Query(query): Query<AuthorizeQuery>,
    Form(login): Form<Login>,
) -> Response {
    if query.response_type != "code" {
        return (StatusCode::BAD_REQUEST, "invalid response_type").into_response();
    }
    if query.code_challenge_method.is_some_and(|x| x != "S256") {
        return (StatusCode::BAD_REQUEST, "invalid code_challenge_method").into_response();
    }

    if query.client_id != state.client_id {
        return (StatusCode::UNAUTHORIZED, "invalid client_id").into_response();
    }
    if query.redirect_uri != state.redirect_url {
        return (StatusCode::FORBIDDEN, "invalid redirect_uri").into_response();
    }

    let code = generate_code();

    let mut url = query.redirect_uri;
    url.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &query.state)
        .finish();

    state.codes.lock().await.insert(
        code,
        CodeState {
            login,
            code_challenge: query.code_challenge,
        },
    );

    (Redirect::to(url.as_str())).into_response()
}

#[derive(Debug, Deserialize)]
struct TokenForm {
    code: String,
    code_verifier: Option<String>,
    grant_type: String,
    redirect_uri: Url,
}

#[derive(Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
}

async fn token(
    state: State,
    TypedHeader(auth): TypedHeader<Authorization<Basic>>,
    Form(TokenForm {
        code,
        code_verifier,
        grant_type,
        redirect_uri,
    }): Form<TokenForm>,
) -> Response {
    if grant_type != "authorization_code" {
        return (StatusCode::BAD_REQUEST, "invalid grant_type").into_response();
    }

    if auth.username() != state.client_id {
        return (StatusCode::UNAUTHORIZED, "invalid client_id").into_response();
    }
    if auth.password() != state.client_secret {
        return (StatusCode::UNAUTHORIZED, "invalid client_secret").into_response();
    }
    if redirect_uri != state.redirect_url {
        return (StatusCode::FORBIDDEN, "invalid redirect_uri").into_response();
    }

    let Some(code_state) = state.codes.lock().await.remove(&code) else {
        return (StatusCode::NOT_FOUND, "invalid code").into_response();
    };

    match (code_state.code_challenge, code_verifier) {
        (Some(challenge), Some(verifier)) => {
            if PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(verifier))
                .as_str()
                != challenge
            {
                return (StatusCode::FORBIDDEN, "invalid code_verifier").into_response();
            }
        }
        (None, None) => {}
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "pkce challenge and verifier must be either both set or unset",
            )
                .into_response()
        }
    }

    let access_token = generate_code();

    state
        .logins
        .write()
        .await
        .insert(access_token.clone(), code_state.login);

    Json(TokenResponse {
        access_token,
        token_type: "bearer",
    })
    .into_response()
}

async fn user(state: State, TypedHeader(auth): TypedHeader<Authorization<Bearer>>) -> Response {
    let logins = state.logins.read().await;
    let Some(login) = logins.get(auth.token()) else {
        return (StatusCode::UNAUTHORIZED, "invalid access token").into_response();
    };

    Json(login).into_response()
}

fn generate_code() -> String {
    Alphanumeric.sample_string(&mut thread_rng(), 32)
}
