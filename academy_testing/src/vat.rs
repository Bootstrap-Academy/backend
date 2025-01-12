use std::net::IpAddr;

use anyhow::Context;
use axum::{
    extract::Path,
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use serde::Serialize;
use tokio::net::TcpListener;
use tracing::info;

const VALIDATE_ROUTE: &str = "/validate/{country}/vat/{id}";

pub async fn start_server(host: IpAddr, port: u16) -> anyhow::Result<()> {
    info!("Starting vat api testing server on {host}:{port}");
    info!("Validate endpoint: http://{host}:{port}{VALIDATE_ROUTE}");
    info!("The only valid vat id is DE0123456789, all other vat ids are rejected.");

    let router = Router::new().route(VALIDATE_ROUTE, routing::get(validate));

    let listener = TcpListener::bind((host, port))
        .await
        .with_context(|| format!("Failed to bind to {host}:{port}"))?;
    axum::serve(listener, router)
        .await
        .context("Failed to start HTTP server")
}

async fn validate(Path((country, id)): Path<(String, String)>) -> Response {
    let is_valid = country == "DE" && id == "0123456789";
    Json(ValidateResponse { is_valid }).into_response()
}

#[derive(Serialize)]
struct ValidateResponse {
    #[serde(rename = "isValid")]
    is_valid: bool,
}
