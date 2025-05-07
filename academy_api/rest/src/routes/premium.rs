use std::{collections::HashMap, sync::Arc};

use academy_core_premium_contracts::{
    PremiumFeatureService, PremiumGetStatusError, PremiumPurchaseError,
    PremiumUpdateSubscriptionError,
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

use super::{coin::NotEnoughCoinsError, user::UserNotFoundError};
use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{
        OkResponse,
        premium::{ApiPremiumPlan, ApiPremiumPlanDetails, ApiPremiumStatus},
        user::PathUserIdOrSelf,
    },
};

pub const TAG: &str = "Premium";

pub fn router(service: Arc<impl PremiumFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/shop/premium_plans",
            routing::get_with(get_plans, get_plans_docs),
        )
        .api_route(
            "/shop/premium/{user_id}",
            routing::get_with(get_status, get_status_docs),
        )
        .api_route("/shop/premium", routing::post_with(purchase, purchase_docs))
        .api_route(
            "/shop/premium/autopay",
            routing::put_with(update_subscription, update_subscription_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_plans(service: State<Arc<impl PremiumFeatureService>>) -> Response {
    let plans = service.get_plans();
    Json(
        plans
            .into_iter()
            .map(|(plan, details)| (plan.into(), details.into()))
            .collect::<HashMap<ApiPremiumPlan, ApiPremiumPlanDetails>>(),
    )
    .into_response()
}

fn get_plans_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return all available premium plans.")
        .add_response::<HashMap<ApiPremiumPlan, ApiPremiumPlanDetails>>(StatusCode::OK, None)
}

async fn get_status(
    service: State<Arc<impl PremiumFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match service.get_status(&token.0, user_id.into()).await {
        Ok(status) => Json(ApiPremiumStatus::from(status)).into_response(),
        Err(PremiumGetStatusError::NotFound) => UserNotFoundError.into_response(),
        Err(PremiumGetStatusError::Auth(err)) => auth_error(err),
        Err(PremiumGetStatusError::Other(err)) => internal_server_error(err),
    }
}

fn get_status_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the premium status of the given user.")
        .add_response::<ApiPremiumStatus>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct PurchaseRequest {
    plan: ApiPremiumPlan,
    #[serde(rename = "autopay")]
    subscribe: Option<bool>,
}

async fn purchase(
    service: State<Arc<impl PremiumFeatureService>>,
    token: ApiToken,
    Json(PurchaseRequest { plan, subscribe }): Json<PurchaseRequest>,
) -> Response {
    match service
        .purchase(&token.0, plan.into(), subscribe.unwrap_or(false))
        .await
    {
        Ok(status) => Json(ApiPremiumStatus::from(status)).into_response(),
        Err(PremiumPurchaseError::NotEnoughCoins) => NotEnoughCoinsError.into_response(),
        Err(PremiumPurchaseError::Auth(err)) => auth_error(err),
        Err(PremiumPurchaseError::Other(err)) => internal_server_error(err),
    }
}

fn purchase_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Purchase premium for the authenticated user.")
        .add_response::<ApiPremiumStatus>(StatusCode::OK, None)
        .add_error::<NotEnoughCoinsError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct UpdateSubscriptionRequest {
    plan: Option<ApiPremiumPlan>,
}

async fn update_subscription(
    service: State<Arc<impl PremiumFeatureService>>,
    token: ApiToken,
    Json(UpdateSubscriptionRequest { plan }): Json<UpdateSubscriptionRequest>,
) -> Response {
    match service
        .update_subscription(&token.0, plan.map(Into::into))
        .await
    {
        Ok(()) => Json(OkResponse).into_response(),
        Err(PremiumUpdateSubscriptionError::NoPremium) => NoPremiumError.into_response(),
        Err(PremiumUpdateSubscriptionError::Auth(err)) => auth_error(err),
        Err(PremiumUpdateSubscriptionError::Other(err)) => internal_server_error(err),
    }
}

fn update_subscription_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Update or cancel a premium subscription.")
        .add_response::<OkResponse>(
            StatusCode::OK,
            "The premium description has been updated/cancelled.",
        )
        .add_error::<NoPremiumError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// The user is not a premium member
    NoPremiumError(PRECONDITION_FAILED, "No premium");
}
