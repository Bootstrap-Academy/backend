use std::{collections::HashMap, net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use axum_extra::{
    headers::{authorization::Basic, Authorization},
    TypedHeader,
};
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::RwLock};
use tracing::info;

pub async fn start_server(
    host: IpAddr,
    port: u16,
    client_id: String,
    client_secret: String,
) -> anyhow::Result<()> {
    info!("Starting PayPal testing server on {host}:{port}");
    info!("Create order endpoint: http://{host}:{port}/v2/checkout/orders");
    info!("Get order endpoint: http://{host}:{port}/v2/checkout/orders/:id");
    info!("Confirm order endpoint: http://{host}:{port}/v2/checkout/orders/:id/confirm-payment-source");
    info!("Capture order endpoint: http://{host}:{port}/v2/checkout/orders/:id/capture");
    info!("Client ID: {client_id:?}");
    info!("Client secret: {client_secret:?}");

    let router = Router::new()
        .route("/v2/checkout/orders", routing::post(create_order))
        .route("/v2/checkout/orders/:id", routing::get(get_order))
        .route(
            "/v2/checkout/orders/:id/confirm-payment-source",
            routing::post(confirm_order),
        )
        .route(
            "/v2/checkout/orders/:id/capture",
            routing::post(capture_order),
        )
        .with_state(Arc::new(StateInner {
            client_id,
            client_secret,
            orders: Default::default(),
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
    orders: RwLock<HashMap<String, Order>>,
}

#[derive(Serialize)]
#[serde(tag = "status", content = "coins")]
enum Order {
    Created(u64),
    Confirmed(u64),
    Captured,
}

async fn create_order(
    state: State,
    TypedHeader(auth): TypedHeader<Authorization<Basic>>,
    Json(data): Json<CreateOrderRequest>,
) -> Response {
    if auth.username() != state.client_id || auth.password() != state.client_secret {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    if data.intent != "CAPTURE"
        || data.purchase_units.len() != 1
        || data.purchase_units[0].amount.currency_code != "EUR"
    {
        return (StatusCode::BAD_REQUEST, "bad request").into_response();
    }

    let Some(price) = data.purchase_units[0]
        .amount
        .value
        .split_once(".")
        .filter(|(a, b)| !a.is_empty() && b.len() == 2)
        .and_then(|(a, b)| Some(a.parse::<u64>().ok()? * 100 + b.parse::<u64>().ok()?))
    else {
        return (StatusCode::BAD_REQUEST, "bad request").into_response();
    };

    let order_id = generate_order_id();

    let mut orders = state.orders.write().await;
    orders.insert(order_id.clone(), Order::Created(price));

    (
        StatusCode::CREATED,
        Json(CreateOrderResponse { id: order_id }),
    )
        .into_response()
}

async fn get_order(state: State, Path(order_id): Path<String>) -> Response {
    let orders = state.orders.read().await;
    match orders.get(&order_id) {
        Some(order) => Json(order).into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn confirm_order(state: State, Path(order_id): Path<String>) -> Response {
    let mut orders = state.orders.write().await;
    let Some(order @ &mut Order::Created(coins)) = orders.get_mut(&order_id) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    *order = Order::Confirmed(coins);

    Json(order).into_response()
}

async fn capture_order(
    state: State,
    TypedHeader(auth): TypedHeader<Authorization<Basic>>,
    Path(order_id): Path<String>,
    Json(CaptureOrderRequest {}): Json<CaptureOrderRequest>,
) -> Response {
    if auth.username() != state.client_id || auth.password() != state.client_secret {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let mut orders = state.orders.write().await;
    let Some(order @ &mut Order::Confirmed(_)) = orders.get_mut(&order_id) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    *order = Order::Captured;

    (
        StatusCode::CREATED,
        Json(CaptureOrderResponse {
            status: "COMPLETED",
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
struct CreateOrderRequest {
    intent: String,
    purchase_units: Vec<PurchaseUnit>,
}

#[derive(Deserialize)]
struct PurchaseUnit {
    amount: Amount,
}

#[derive(Deserialize)]
struct Amount {
    currency_code: String,
    value: String,
}

#[derive(Serialize)]
struct CreateOrderResponse {
    id: String,
}

#[derive(Deserialize)]
struct CaptureOrderRequest {}

#[derive(Serialize)]
struct CaptureOrderResponse {
    status: &'static str,
}

fn generate_order_id() -> String {
    Alphanumeric.sample_string(&mut thread_rng(), 32)
}
