use std::sync::Arc;

use academy_core_paypal_contracts::{
    PaypalCaptureCoinOrderError, PaypalCreateCoinOrderError, PaypalFeatureService,
};
use academy_models::paypal::PaypalOrderId;
use aide::{
    axum::{routing, ApiRouter},
    transform::TransformOperation,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::coin::ApiBalance,
};

pub const TAG: &str = "PayPal";

pub fn router(service: Arc<impl PaypalFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/shop/coins/paypal",
            routing::get_with(get_client_id, get_client_id_docs),
        )
        .api_route(
            "/shop/coins/paypal/orders",
            routing::post_with(create_coin_order, create_coin_order_docs),
        )
        .api_route(
            "/shop/coins/paypal/orders/:order_id/capture",
            routing::post_with(capture_coin_order, capture_coin_order_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_client_id(service: State<Arc<impl PaypalFeatureService>>) -> Response {
    Json(service.get_client_id()).into_response()
}

fn get_client_id_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the public PayPal client id.")
        .add_response::<&str>(StatusCode::OK, None)
}

#[derive(Deserialize, JsonSchema)]
struct CreateCoinOrderRequest {
    /// The number of Morphcoins to buy.
    coins: u64,
}

async fn create_coin_order(
    service: State<Arc<impl PaypalFeatureService>>,
    token: ApiToken,
    Json(CreateCoinOrderRequest { coins }): Json<CreateCoinOrderRequest>,
) -> Response {
    match service.create_coin_order(&token.0, coins).await {
        Ok(order_id) => Json(order_id).into_response(),
        Err(PaypalCreateCoinOrderError::InvalidAmount(_)) => InvalidAmountError.into_response(),
        Err(PaypalCreateCoinOrderError::IncompleteInvoiceInfo) => {
            UserInfoMissingError.into_response()
        }
        Err(PaypalCreateCoinOrderError::CreateOrderFailure) => {
            CouldNotCreateOrderError.into_response()
        }
        Err(PaypalCreateCoinOrderError::Auth(err)) => auth_error(err),
        Err(PaypalCreateCoinOrderError::Other(err)) => internal_server_error(err),
    }
}

fn create_coin_order_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create a new PayPal order to purchase the specified number of Morphcoins.")
        .add_response::<String>(StatusCode::OK, "The order has been created.")
        .add_error::<InvalidAmountError>()
        .add_error::<UserInfoMissingError>()
        .add_error::<CouldNotCreateOrderError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct CaptureCoinOrderPath {
    order_id: PaypalOrderId,
}

async fn capture_coin_order(
    service: State<Arc<impl PaypalFeatureService>>,
    token: ApiToken,
    Path(CaptureCoinOrderPath { order_id }): Path<CaptureCoinOrderPath>,
) -> Response {
    match service.capture_coin_order(&token.0, order_id).await {
        Ok(balance) => Json(ApiBalance::from(balance)).into_response(),
        Err(PaypalCaptureCoinOrderError::NotFound) => OrderNotFoundError.into_response(),
        Err(PaypalCaptureCoinOrderError::IncompleteInvoiceInfo) => {
            UserInfoMissingError.into_response()
        }
        Err(PaypalCaptureCoinOrderError::CaptureOrderFailure) => {
            CouldNotCaptureOrderError.into_response()
        }
        Err(PaypalCaptureCoinOrderError::Auth(err)) => auth_error(err),
        Err(PaypalCaptureCoinOrderError::Other(err)) => internal_server_error(err),
    }
}

fn capture_coin_order_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Complete Morphcoin purchase.")
        .add_response::<ApiBalance>(
            StatusCode::OK,
            "Payment has been captured and the purchased Morphcoins have been added to the user's \
             balance.",
        )
        .add_error::<OrderNotFoundError>()
        .add_error::<UserInfoMissingError>()
        .add_error::<CouldNotCaptureOrderError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// The user cannot buy coins because some information about them is missing
    UserInfoMissingError(PRECONDITION_FAILED, "User Infos missing");
    /// The order could not be created.
    CouldNotCreateOrderError(INTERNAL_SERVER_ERROR, "Could not create order");
    /// The order does not exist.
    OrderNotFoundError(NOT_FOUND, "Order not found");
    /// The order could not be captured.
    CouldNotCaptureOrderError(BAD_REQUEST, "Could not capture order");
    /// The specified number of Morphcoins is outside of the allowed range.
    InvalidAmountError(BAD_REQUEST, "Invalid amount");
}
