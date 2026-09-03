use std::{net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing,
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use serde::Serialize;
use tokio::{net::TcpListener, sync::RwLock};
use tracing::info;
use uuid::Uuid;

const DELETE_USER_ROUTE: &str = "/{service}/_internal/users/{user_id}";
const DELETED_USERS_ROUTE: &str = "/deleted_users";
const EXPORT_USER_ROUTE: &str = "/{service}/_internal/users/{user_id}/export";

/// Deleting and exporting this user always fails.
const FAILING_USER_ID: Uuid = Uuid::nil();

/// Exporting this user returns a response that is larger than any sensible
/// size limit.
const OVERSIZED_USER_ID: Uuid = Uuid::max();

/// Number of padding entries in the export of [`OVERSIZED_USER_ID`].
const OVERSIZED_EXPORT_ENTRIES: usize = 1024;

pub async fn start_server(host: IpAddr, port: u16) -> anyhow::Result<()> {
    let listener = TcpListener::bind((host, port))
        .await
        .with_context(|| format!("Failed to bind to {host}:{port}"))?;

    let url = format!("http://{}", listener.local_addr()?);
    info!("Starting microservices testing server on {url}");
    info!("Delete user endpoint: {url}{DELETE_USER_ROUTE}");
    info!("Deleted users endpoint: {url}{DELETED_USERS_ROUTE}");
    info!("Export user endpoint: {url}{EXPORT_USER_ROUTE}");
    info!("Deleting the user {FAILING_USER_ID} always fails, all other users are deleted.");
    info!("Exporting the user {FAILING_USER_ID} always fails.");
    info!("Exporting the user {OVERSIZED_USER_ID} returns an oversized response.");

    let router = Router::new()
        .route(DELETE_USER_ROUTE, routing::delete(delete_user))
        .route(DELETED_USERS_ROUTE, routing::get(deleted_users))
        .route(EXPORT_USER_ROUTE, routing::get(export_user))
        .with_state(Arc::new(StateInner {
            deleted_users: Default::default(),
        }));

    axum::serve(listener, router)
        .await
        .context("Failed to start HTTP server")
}

type State = axum::extract::State<Arc<StateInner>>;
struct StateInner {
    deleted_users: RwLock<Vec<DeletedUser>>,
}

#[derive(Clone, Serialize)]
struct DeletedUser {
    service: String,
    token: String,
    user_id: Uuid,
}

async fn delete_user(
    state: State,
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Path((service, user_id)): Path<(String, Uuid)>,
) -> Response {
    state.deleted_users.write().await.push(DeletedUser {
        service,
        token: auth.token().into(),
        user_id,
    });

    if user_id == FAILING_USER_ID {
        return (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

async fn deleted_users(state: State) -> Response {
    let deleted_users = state.deleted_users.read().await.clone();
    Json(deleted_users).into_response()
}

async fn export_user(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
    Path((service, user_id)): Path<(String, Uuid)>,
) -> Response {
    if user_id == FAILING_USER_ID {
        return (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response();
    }

    let padding = (user_id == OVERSIZED_USER_ID)
        .then(|| vec!["x".repeat(OVERSIZED_EXPORT_ENTRIES); OVERSIZED_EXPORT_ENTRIES]);

    Json(UserExport {
        service,
        token: auth.token().into(),
        user_id,
        padding,
    })
    .into_response()
}

#[derive(Serialize)]
struct UserExport {
    service: String,
    token: String,
    user_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    padding: Option<Vec<String>>,
}
