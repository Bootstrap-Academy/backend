use std::sync::Arc;

use academy_core_heart_contracts::{HeartFeatureService, HeartGetError, HeartRefillError};
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

use super::{coin::NotEnoughCoinsError, user::UserNotFoundError};
use crate::{
    docs::TransformOperationExt,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{
        heart::{ApiHeartConfig, ApiHearts},
        user::PathUserIdOrSelf,
    },
};

pub const TAG: &str = "Heart";

pub fn router(service: Arc<impl HeartFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/shop/hearts/config",
            routing::get_with(get_config, get_config_docs),
        )
        .api_route("/shop/hearts/{user_id}", routing::get_with(get, get_docs))
        .api_route("/shop/hearts", routing::put_with(refill, refill_docs))
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_config(service: State<Arc<impl HeartFeatureService>>) -> Response {
    Json(ApiHeartConfig::from(service.get_config())).into_response()
}

fn get_config_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the public heart configuration.")
        .add_response::<ApiHeartConfig>(StatusCode::OK, None)
}

async fn get(
    service: State<Arc<impl HeartFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match service.get(&token.0, user_id.into()).await {
        Ok(hearts) => Json(ApiHearts::from(hearts)).into_response(),
        Err(HeartGetError::UserNotFound) => UserNotFoundError.into_response(),
        Err(HeartGetError::Auth(err)) => auth_error(err),
        Err(HeartGetError::Other(err)) => internal_server_error(err),
    }
}

fn get_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the hearts of the given user.")
        .add_response::<ApiHearts>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn refill(service: State<Arc<impl HeartFeatureService>>, token: ApiToken) -> Response {
    match service.refill(&token.0).await {
        Ok(hearts) => Json(ApiHearts::from(hearts)).into_response(),
        Err(HeartRefillError::NotEnoughCoins) => NotEnoughCoinsError.into_response(),
        Err(HeartRefillError::Auth(err)) => auth_error(err),
        Err(HeartRefillError::Other(err)) => internal_server_error(err),
    }
}

fn refill_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Manually refill hearts to maximum.")
        .add_response::<ApiHearts>(StatusCode::OK, None)
        .add_error::<NotEnoughCoinsError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}
