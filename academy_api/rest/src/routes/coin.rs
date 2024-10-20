use std::sync::Arc;

use academy_core_coin_contracts::{CoinAddCoinsError, CoinFeatureService, CoinGetBalanceError};
use academy_models::coin::TransactionDescription;
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

use super::user::UserNotFoundError;
use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{coin::ApiBalance, user::PathUserIdOrSelf, OkResponse, StringOption},
};

pub const TAG: &str = "Coins";

pub fn router(service: Arc<impl CoinFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/shop/coins/:user_id",
            routing::get_with(get_balance, get_balance_docs).post_with(add_coins, add_coins_docs),
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
        Err(CoinGetBalanceError::UserNotFound) => UserNotFoundError.into_response(),
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

#[derive(Deserialize, JsonSchema)]
struct AddCoinsRequest {
    /// Number of Morphcoins to add to the user's balance. Can be negative to
    /// remove coins.
    coins: i64,
    /// Description of the transaction.
    description: StringOption<TransactionDescription>,
    /// Whether to include this transaction in a credit note.
    credit_note: Option<bool>,
}

async fn add_coins(
    service: State<Arc<impl CoinFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
    Json(AddCoinsRequest {
        coins,
        description,
        credit_note,
    }): Json<AddCoinsRequest>,
) -> Response {
    match service
        .add_coins(
            &token.0,
            user_id.into(),
            coins,
            description.into(),
            credit_note.unwrap_or(coins > 0),
        )
        .await
    {
        Ok(_balance) => Json(OkResponse).into_response(),
        Err(CoinAddCoinsError::UserNotFound) => UserNotFoundError.into_response(),
        Err(CoinAddCoinsError::NotEnoughCoins) => NotEnoughCoinsError.into_response(),
        Err(CoinAddCoinsError::Auth(err)) => auth_error(err),
        Err(CoinAddCoinsError::Other(err)) => internal_server_error(err),
    }
}

fn add_coins_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Add Morphcoins to the balance of the given user.")
        .description(
            "If a negative amount of coins is given, coins are removed from the user's balance. \
             An error is returned if the new balance would be negative.",
        )
        .add_response::<OkResponse>(
            StatusCode::OK,
            "The given number of coins have been added to the user's balance.",
        )
        .add_error::<UserNotFoundError>()
        .add_error::<NotEnoughCoinsError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// The user does not have enough coins to perform this action.
    NotEnoughCoinsError(PRECONDITION_FAILED, "Not enough coins");
}
