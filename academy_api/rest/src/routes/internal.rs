use std::sync::Arc;

use academy_auth_contracts::internal::AuthInternalAuthenticateError;
use academy_core_internal_contracts::{
    InternalAddCoinsError, InternalGetUserByEmailError, InternalGetUserError, InternalService,
};
use academy_models::{
    auth::InternalToken, coin::TransactionDescription, email_address::EmailAddress,
};
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

use super::{coin::NotEnoughCoinsError, user::UserNotFoundError};
use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{
        coin::ApiBalance,
        user::{ApiUser, PathUserId},
    },
};

pub const TAG: &str = "Internal";

pub fn router(service: Arc<impl InternalService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/_internal/users/:user_id",
            routing::get_with(get_user, get_user_docs),
        )
        .api_route(
            "/auth/_internal/users/by_email/:email",
            routing::get_with(get_user_by_email, get_user_by_email_docs),
        )
        .api_route(
            "/shop/_internal/coins/:user_id",
            routing::post_with(add_coins, add_coins_docs),
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
        Err(InternalAddCoinsError::UserNotFound) => UserNotFoundError.into_response(),
        Err(InternalAddCoinsError::NotEnoughCoins) => NotEnoughCoinsError.into_response(),
        Err(InternalAddCoinsError::Auth(err)) => internal_auth_error(err),
        Err(InternalAddCoinsError::Other(err)) => internal_server_error(err),
    }
}

fn add_coins_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Add Morphcoins to the balance of the given user.")
        .add_response::<ApiBalance>(
            StatusCode::OK,
            "The given number of coins have been added to the user's balance.",
        )
        .add_error::<UserNotFoundError>()
        .add_error::<NotEnoughCoinsError>()
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
    /// The internal authentication token is invalid or has expired.
    InvalidTokenError(UNAUTHORIZED, "Invalid token");
}
