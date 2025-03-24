use std::{net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::{extract::State, routing, Form, Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tracing::info;

const SITEVERIFY_ROUTE: &str = "/recaptcha/api/siteverify";

pub async fn start_server(host: IpAddr, port: u16, secret: String) -> anyhow::Result<()> {
    let listener = TcpListener::bind((host, port))
        .await
        .with_context(|| format!("Failed to bind to {host}:{port}"))?;

    let url = format!("http://{}", listener.local_addr()?);
    info!("Starting recaptcha testing server on {url}");
    info!("Recaptcha siteverify endpoint: {url}{SITEVERIFY_ROUTE}");
    info!("Secret: {secret:?}");
    info!(
        "Valid recaptcha responses are \"success\" and \"success-SCORE\", where SCORE is a \
         floating point number between 0 and 1"
    );

    let router = Router::new()
        .route(SITEVERIFY_ROUTE, routing::post(siteverify))
        .with_state(secret.into());

    axum::serve(listener, router)
        .await
        .context("Failed to start HTTP server")
}

#[derive(Deserialize)]
struct SiteverifyRequest {
    secret: String,
    response: String,
}

#[derive(Serialize)]
struct SiteverifyResponse {
    success: bool,
    score: Option<f64>,
}

async fn siteverify(
    state: State<Arc<str>>,
    Form(SiteverifyRequest { secret, response }): Form<SiteverifyRequest>,
) -> Json<SiteverifyResponse> {
    let mut parts = response.split('-');
    let success = *secret == **state && parts.next() == Some("success");
    let score = success
        .then(|| parts.next())
        .flatten()
        .and_then(|score| score.parse::<f64>().ok())
        .filter(|score| (0.0..=1.0).contains(score));

    Json(SiteverifyResponse { success, score })
}
