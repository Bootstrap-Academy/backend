use std::{collections::HashMap, sync::Arc};

use academy_core_premium_contracts::{
    PremiumFeatureService, PremiumGetStatusError, PremiumUpdateSubscriptionError,
};
use academy_models::premium::PremiumRenewalConsent;
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
use serde::{Deserialize, Serialize};

use super::user::UserNotFoundError;
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
            "/shop/premium/renewal-offer",
            routing::get_with(get_renewal_offer, |op| {
                op.summary("Return the exact monthly renewal offer requiring explicit consent.")
                    .add_response::<RenewalOfferResponse>(StatusCode::OK, None)
            }),
        )
        .api_route(
            "/shop/premium/{user_id}",
            routing::get_with(get_status, get_status_docs),
        )
        .api_route(
            "/shop/premium/autopay",
            routing::put_with(update_subscription, update_subscription_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

#[derive(Serialize, JsonSchema)]
struct RenewalOfferResponse {
    id: String,
    monthly_price: u64,
    terms_version: String,
    text: String,
}

async fn get_renewal_offer(
    service: State<Arc<impl PremiumFeatureService>>,
) -> Json<RenewalOfferResponse> {
    let offer = service.get_renewal_offer();
    Json(RenewalOfferResponse {
        id: offer.id,
        monthly_price: offer.monthly_price,
        terms_version: offer.terms_version,
        text: offer.text,
    })
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
        .description(
            "`months` counts calendar months: a period ends on the same day of the month it began \
             on, or on the last day of that month if it does not have that day (§ 188 Abs. 2 and \
             Abs. 3 BGB). Twelve of them are a year.",
        )
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
struct UpdateSubscriptionRequest {
    plan: Option<ApiPremiumPlan>,
    consent: Option<RenewalConsentRequest>,
}

#[derive(Deserialize, JsonSchema)]
struct RenewalConsentRequest {
    request_id: uuid::Uuid,
    offer_id: String,
    accepted: bool,
    withdrawal_consent: bool,
}

async fn update_subscription(
    service: State<Arc<impl PremiumFeatureService>>,
    token: ApiToken,
    Json(UpdateSubscriptionRequest { plan, consent }): Json<UpdateSubscriptionRequest>,
) -> Response {
    match service
        .update_subscription(
            &token.0,
            plan.map(Into::into),
            consent.map(|c| PremiumRenewalConsent {
                request_id: c.request_id.into(),
                offer_id: c.offer_id,
                accepted: c.accepted,
                withdrawal_consent: c.withdrawal_consent,
            }),
        )
        .await
    {
        Ok(()) => Json(OkResponse).into_response(),
        Err(PremiumUpdateSubscriptionError::NoPremium) => NoPremiumError.into_response(),
        Err(PremiumUpdateSubscriptionError::RenewalConsentRequired) => {
            RenewalConsentRequiredError.into_response()
        }
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
        .add_error::<RenewalConsentRequiredError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// Explicit consent to the current monthly offer is required; no debit was made.
    RenewalConsentRequiredError(PRECONDITION_FAILED, "Current monthly renewal consent required");
    /// The user is not a premium member
    NoPremiumError(PRECONDITION_FAILED, "No premium");
}
