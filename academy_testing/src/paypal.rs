use std::{collections::HashMap, net::IpAddr, sync::Arc};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing,
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Basic},
};
use rand::{
    distr::{Alphanumeric, SampleString},
    rng,
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
    let listener = TcpListener::bind((host, port))
        .await
        .with_context(|| format!("Failed to bind to {host}:{port}"))?;

    let url = format!("http://{}", listener.local_addr()?);
    info!("Starting PayPal testing server on {url}");
    info!("Create order endpoint: {url}/v2/checkout/orders");
    info!("Get order endpoint: {url}/v2/checkout/orders/{{id}}");
    info!("Confirm order endpoint: {url}/v2/checkout/orders/{{id}}/confirm-payment-source");
    info!("Capture order endpoint: {url}/v2/checkout/orders/{{id}}/capture");
    info!("Client ID: {client_id:?}");
    info!("Client secret: {client_secret:?}");

    let router = Router::new()
        .route("/v1/oauth2/token", routing::post(token))
        .route("/v2/checkout/orders", routing::post(create_order))
        .route("/v2/checkout/orders/{id}", routing::get(get_order))
        .route(
            "/v2/checkout/orders/{id}/confirm-payment-source",
            routing::post(confirm_order),
        )
        .route(
            "/v2/checkout/orders/{id}/capture",
            routing::post(capture_order),
        )
        .with_state(Arc::new(StateInner {
            client_id,
            client_secret,
            orders: Default::default(),
        }));

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

struct Order {
    coins: u64,
    status: &'static str,
    capture: Option<(String, String, String)>,
}

fn order_json(id: &str, order: &Order) -> serde_json::Value {
    let amount = serde_json::json!({"currency_code":"EUR", "value":format!("{}.{:02}",order.coins/100,order.coins%100)});
    let captures = order
        .capture
        .as_ref()
        .map(|(id, at, _)| {
            serde_json::json!([{
                "id":id,"status":"COMPLETED","amount":amount,"create_time":at
            }])
        })
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::json!({"id":id,"intent":"CAPTURE","status":order.status,"purchase_units":[{
        "amount":amount,"payee":{"merchant_id":"TESTMERCHANT"},"payments":{"captures":captures}
    }]})
}

async fn token(state: State, TypedHeader(auth): TypedHeader<Authorization<Basic>>) -> Response {
    if auth.username() != state.client_id || auth.password() != state.client_secret {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(serde_json::json!({"access_token":"synthetic-paypal-token","token_type":"Bearer"}))
        .into_response()
}
fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .is_some_and(|h| h == "Bearer synthetic-paypal-token")
}

async fn create_order(
    state: State,
    headers: HeaderMap,
    Json(data): Json<CreateOrderRequest>,
) -> Response {
    if !authorized(&headers) {
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
    orders.insert(
        order_id.clone(),
        Order {
            coins: price,
            status: "CREATED",
            capture: None,
        },
    );

    (
        StatusCode::CREATED,
        Json(CreateOrderResponse { id: order_id }),
    )
        .into_response()
}

async fn get_order(state: State, Path(order_id): Path<String>) -> Response {
    let orders = state.orders.read().await;
    match orders.get(&order_id) {
        Some(order) => Json(order_json(&order_id, order)).into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn confirm_order(state: State, Path(order_id): Path<String>) -> Response {
    let mut orders = state.orders.write().await;
    let Some(order) = orders.get_mut(&order_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if order.status == "CREATED" {
        order.status = "APPROVED";
    }
    Json(order_json(&order_id, order)).into_response()
}

async fn capture_order(
    state: State,
    headers: HeaderMap,
    Path(order_id): Path<String>,
    Json(_): Json<serde_json::Value>,
) -> Response {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(request_id) = headers
        .get("paypal-request-id")
        .and_then(|h| h.to_str().ok())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let mut orders = state.orders.write().await;
    let Some(order) = orders.get_mut(&order_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if let Some((_, _, key)) = &order.capture {
        if key != request_id {
            return StatusCode::UNPROCESSABLE_ENTITY.into_response();
        }
        return Json(order_json(&order_id, order)).into_response();
    }
    if order.status != "APPROVED" {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    order.status = "COMPLETED";
    order.capture = Some((
        generate_order_id(),
        chrono::Utc::now().to_rfc3339(),
        request_id.into(),
    ));
    (StatusCode::CREATED, Json(order_json(&order_id, order))).into_response()
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

fn generate_order_id() -> String {
    Alphanumeric.sample_string(&mut rng(), 32)
}
