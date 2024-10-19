use std::sync::Arc;

use academy_core_coin_contracts::{CoinFeatureService, CoinGetBalanceError};
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

use super::user::UserNotFoundError;
use crate::{
    docs::TransformOperationExt,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{coin::ApiBalance, user::PathUserIdOrSelf},
};

pub const TAG: &str = "Coins";

pub fn router(service: Arc<impl CoinFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/shop/coins/:user_id",
            routing::get_with(get_balance, get_balance_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_balance(
    service: State<Arc<impl CoinFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match service.get_balance(&token.0, user_id.into()).await {
        Ok(balance) => Json(ApiBalance::from(balance)).into_response(),
        Err(CoinGetBalanceError::NotFound) => UserNotFoundError.into_response(),
        Err(CoinGetBalanceError::Auth(err)) => auth_error(err),
        Err(CoinGetBalanceError::Other(err)) => internal_server_error(err),
    }
}

fn get_balance_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the Morphcoin balance of the given user.")
        .add_response::<ApiBalance>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}
