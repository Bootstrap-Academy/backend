use std::{collections::HashMap, net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::Path,
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use tokio::{net::TcpListener, sync::RwLock};
use tracing::info;
use uuid::Uuid;

pub async fn start_server(host: IpAddr, port: u16) -> anyhow::Result<()> {
    info!("Starting internal api testing server on {host}:{port}");
    info!("Shop base url: http://{host}:{port}/shop");

    let router = Router::new()
        .route(
            "/shop/_internal/coins/{user_id}/withheld",
            routing::get(release_coins_calls).put(release_coins),
        )
        .with_state(Arc::new(StateInner {
            release_coins_calls: Default::default(),
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
    release_coins_calls: RwLock<HashMap<Uuid, usize>>,
}

async fn release_coins(state: State, Path(user_id): Path<Uuid>) -> Response {
    let mut release_coins_calls = state.release_coins_calls.write().await;
    release_coins_calls
        .entry(user_id)
        .and_modify(|x| *x += 1)
        .or_insert(1);

    Json(true).into_response()
}

async fn release_coins_calls(state: State, Path(user_id): Path<Uuid>) -> Response {
    let release_coins_calls = state.release_coins_calls.read().await;
    Json(release_coins_calls.get(&user_id).copied().unwrap_or(0)).into_response()
}
