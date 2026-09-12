use std::sync::Arc;

use academy_auth_contracts::internal::AuthInternalAuthenticateError;
use academy_core_internal_contracts::{
    InternalAddCoinsError, InternalAddHeartsError, InternalGetHeartsError,
    InternalGetUserByEmailError, InternalGetUserError, InternalHasPremiumError,
    InternalHeartOperationError, InternalService,
};
use academy_models::{
    auth::InternalToken,
    coin::{CoinOperation, CoinOperationId, TransactionDescription},
    email_address::EmailAddress,
    heart::{HeartOperation, HeartOperationId},
    user::UserId,
};
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{
    coin::{CoinCreditNotAuthorizedError, NotEnoughCoinsError},
    user::UserNotFoundError,
};
use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{
        coin::ApiBalance,
        heart::{ApiHeartOperationReceipt, ApiHearts},
        user::{ApiUser, PathUserId},
    },
};

pub const TAG: &str = "Internal";

#[cfg(test)]
mod heart_operation_tests;

pub fn router(service: Arc<impl InternalService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/_internal/users/{user_id}",
            routing::get_with(get_user, get_user_docs),
        )
        .api_route(
            "/auth/_internal/users/by_email/{email}",
            routing::get_with(get_user_by_email, get_user_by_email_docs),
        )
        .api_route(
            "/shop/_internal/coins/{user_id}",
            routing::post_with(add_coins, add_coins_docs),
        )
        .api_route(
            "/shop/_internal/hearts/{user_id}",
            routing::get_with(get_hearts, get_hearts_docs).post_with(add_hearts, add_hearts_docs),
        )
        .api_route(
            "/shop/_internal/premium/{user_id}",
            routing::get_with(has_premium, has_premium_docs),
        )
        .api_route(
            "/shop/_internal/coin-operations/{operation_id}/{user_id}",
            routing::put_with(apply_coin_operation, add_coins_docs),
        )
        .api_route(
            "/shop/_internal/heart-operations/{operation_id}/{user_id}",
            routing::put_with(apply_heart_operation, heart_operation_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_user(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(PathUserId { user_id }): Path<PathUserId>,
) -> Response {
    match service.get_user(&token.0, user_id).await {
        Ok(user) => Json(ApiUser::from(user)).into_response(),
        Err(InternalGetUserError::NotFound) => UserNotFoundError.into_response(),
        Err(InternalGetUserError::Auth(err)) => internal_auth_error(err),
        Err(InternalGetUserError::Other(err)) => internal_server_error(err),
    }
}

fn get_user_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the user with the given id.")
        .add_response::<ApiUser>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(internal_auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct GetUserByEmailPath {
    email: EmailAddress,
}

async fn get_user_by_email(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(GetUserByEmailPath { email }): Path<GetUserByEmailPath>,
) -> Response {
    match service.get_user_by_email(&token.0, email).await {
        Ok(user) => Json(ApiUser::from(user)).into_response(),
        Err(InternalGetUserByEmailError::NotFound) => UserNotFoundError.into_response(),
        Err(InternalGetUserByEmailError::Auth(err)) => internal_auth_error(err),
        Err(InternalGetUserByEmailError::Other(err)) => internal_server_error(err),
    }
}

fn get_user_by_email_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the user with the given email address.")
        .add_response::<ApiUser>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(internal_auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct AddCoinsRequest {
    /// Number of Morphcoins to add to the user's balance. Can be negative to
    /// remove coins.
    coins: i64,
    /// Description of the transaction.
    description: Option<TransactionDescription>,
    /// Whether to include this transaction in a credit note.
    credit_note: Option<bool>,
}

async fn add_coins(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(PathUserId { user_id }): Path<PathUserId>,
    Json(AddCoinsRequest {
        coins,
        description,
        credit_note,
    }): Json<AddCoinsRequest>,
) -> Response {
    match service
        .add_coins(
            &token.0,
            user_id,
            coins,
            description,
            credit_note.unwrap_or(coins > 0),
        )
        .await
    {
        Ok(balance) => Json(ApiBalance::from(balance)).into_response(),
        Err(InternalAddCoinsError::CreditNotAuthorized) => {
            CoinCreditNotAuthorizedError.into_response()
        }
        Err(InternalAddCoinsError::OperationConflict) => CoinOperationConflictError.into_response(),
        Err(InternalAddCoinsError::UserNotFound) => UserNotFoundError.into_response(),
        Err(InternalAddCoinsError::NotEnoughCoins) => NotEnoughCoinsError.into_response(),
        Err(InternalAddCoinsError::Auth(err)) => internal_auth_error(err),
        Err(InternalAddCoinsError::Other(err)) => internal_server_error(err),
    }
}

#[derive(Deserialize, JsonSchema)]
struct CoinOperationPath {
    operation_id: CoinOperationId,
    user_id: UserId,
}

async fn apply_coin_operation(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(CoinOperationPath {
        operation_id,
        user_id,
    }): Path<CoinOperationPath>,
    Json(request): Json<AddCoinsRequest>,
) -> Response {
    let operation = CoinOperation {
        id: operation_id,
        user_id,
        coins: request.coins,
        description: request.description,
        include_in_credit_note: request.credit_note.unwrap_or(request.coins > 0),
    };
    match service.apply_coin_operation(&token.0, operation).await {
        Ok(balance) => Json(ApiBalance::from(balance)).into_response(),
        Err(InternalAddCoinsError::CreditNotAuthorized) => {
            CoinCreditNotAuthorizedError.into_response()
        }
        Err(InternalAddCoinsError::OperationConflict) => CoinOperationConflictError.into_response(),
        Err(InternalAddCoinsError::UserNotFound) => UserNotFoundError.into_response(),
        Err(InternalAddCoinsError::NotEnoughCoins) => NotEnoughCoinsError.into_response(),
        Err(InternalAddCoinsError::Auth(err)) => internal_auth_error(err),
        Err(InternalAddCoinsError::Other(err)) => internal_server_error(err),
    }
}

fn add_coins_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Apply an authorized Morphcoin operation.")
        .description("New positive credits require an exact, prepared historical reservation on the durable operation route. Unkeyed positive requests are rejected. Purchases use their owning purchase flow.")
        .add_response::<ApiBalance>(
            StatusCode::OK,
            "The operation has been applied, or its exact completed receipt replayed.",
        )
        .add_error::<UserNotFoundError>()
        .add_error::<NotEnoughCoinsError>()
        .add_error::<CoinOperationConflictError>()
        .add_error::<CoinCreditNotAuthorizedError>()
        .with(internal_auth_error_docs)
        .with(internal_server_error_docs)
}

async fn get_hearts(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(PathUserId { user_id }): Path<PathUserId>,
) -> Response {
    match service.get_hearts(&token.0, user_id).await {
        Ok(hearts) => Json(ApiHearts::from(hearts)).into_response(),
        Err(InternalGetHeartsError::UserNotFound) => UserNotFoundError.into_response(),
        Err(InternalGetHeartsError::Auth(err)) => internal_auth_error(err),
        Err(InternalGetHeartsError::Other(err)) => internal_server_error(err),
    }
}

fn get_hearts_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Get Morphhearts to the balance of the given user.")
        .add_response::<ApiHearts>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(internal_auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct AddHeartsRequest {
    /// Number of hearts to add. Can be negative to remove hearts.
    hearts: i64,
}

async fn add_hearts(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(PathUserId { user_id }): Path<PathUserId>,
    Json(AddHeartsRequest { hearts }): Json<AddHeartsRequest>,
) -> Response {
    match service.add_hearts(&token.0, user_id, hearts).await {
        Ok(_) => Json(true).into_response(),
        Err(InternalAddHeartsError::UserNotFound) => UserNotFoundError.into_response(),
        Err(InternalAddHeartsError::NotEnoughHearts) => Json(false).into_response(),
        Err(InternalAddHeartsError::Auth(err)) => internal_auth_error(err),
        Err(InternalAddHeartsError::Other(err)) => internal_server_error(err),
    }
}

fn add_hearts_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Add Morphhearts to the balance of the given user.")
        .add_response::<bool>(
            StatusCode::OK,
            "Returns whether the operation was successful.",
        )
        .add_error::<UserNotFoundError>()
        .with(internal_auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct HeartOperationPath {
    operation_id: HeartOperationId,
    user_id: UserId,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct HeartOperationRequest {
    half_hearts: u64,
    reason: String,
}

async fn apply_heart_operation(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(HeartOperationPath {
        operation_id,
        user_id,
    }): Path<HeartOperationPath>,
    Json(request): Json<HeartOperationRequest>,
) -> Response {
    let operation = HeartOperation {
        id: operation_id,
        user_id,
        half_hearts: request.half_hearts,
        reason: request.reason,
    };
    match service.apply_heart_operation(&token.0, operation).await {
        Ok(receipt) => Json(ApiHeartOperationReceipt::from(receipt)).into_response(),
        Err(InternalHeartOperationError::OperationConflict) => {
            HeartOperationConflictError.into_response()
        }
        Err(InternalHeartOperationError::InvalidRequest) => {
            InvalidHeartOperationError.into_response()
        }
        Err(InternalHeartOperationError::UserNotFound) => UserNotFoundError.into_response(),
        Err(InternalHeartOperationError::Auth(err)) => internal_auth_error(err),
        Err(InternalHeartOperationError::Other(err)) => internal_server_error(err),
    }
}

fn heart_operation_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Apply or replay one final incorrect-attempt heart debit.")
        .add_response::<ApiHeartOperationReceipt>(StatusCode::OK, None)
        .add_error::<HeartOperationConflictError>()
        .add_error::<InvalidHeartOperationError>()
        .add_error::<UserNotFoundError>()
        .with(internal_auth_error_docs)
        .with(internal_server_error_docs)
}

async fn has_premium(
    service: State<Arc<impl InternalService>>,
    token: ApiToken<InternalToken>,
    Path(PathUserId { user_id }): Path<PathUserId>,
) -> Response {
    match service.has_premium(&token.0, user_id).await {
        Ok(result) => Json(result).into_response(),
        Err(InternalHasPremiumError::UserNotFound) => UserNotFoundError.into_response(),
        Err(InternalHasPremiumError::Auth(err)) => internal_auth_error(err),
        Err(InternalHasPremiumError::Other(err)) => internal_server_error(err),
    }
}

fn has_premium_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return whether the given user is a premium member.")
        .add_response::<bool>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(internal_auth_error_docs)
        .with(internal_server_error_docs)
}

fn internal_auth_error(err: AuthInternalAuthenticateError) -> Response {
    match err {
        AuthInternalAuthenticateError::InvalidToken => InvalidTokenError.into_response(),
    }
}

fn internal_auth_error_docs(op: TransformOperation) -> TransformOperation {
    op.add_error::<InvalidTokenError>()
}

error_code! {
    /// The operation id was used for a different heart request.
    HeartOperationConflictError(CONFLICT, "Heart operation conflict");
    /// Only two half-hearts for an incorrect challenge attempt are supported.
    InvalidHeartOperationError(UNPROCESSABLE_ENTITY, "Invalid heart operation");
    /// The operation id has already been used for a different request.
    CoinOperationConflictError(CONFLICT, "Coin operation conflict");
    /// The internal authentication token is invalid or has expired.
    InvalidTokenError(UNAUTHORIZED, "Invalid token");
}
