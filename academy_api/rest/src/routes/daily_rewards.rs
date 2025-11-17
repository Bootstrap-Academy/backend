use std::sync::Arc;

use academy_core_daily_rewards_contracts::{
    DailyRewardClaimAllError, DailyRewardClaimError, DailyRewardFeatureService,
    DailyRewardGetError, DailyRewardsSnapshot,
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

use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::daily_rewards::{
        ApiDailyRewardClaimAllResponse, ApiDailyRewardClaimResponse, PathDailyRewardCategory,
    },
};

pub const TAG: &str = "DailyRewards";

pub fn router(service: Arc<impl DailyRewardFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route("/daily-rewards", routing::get_with(get, get_docs))
        .api_route(
            "/daily-rewards/{category}/claim",
            routing::post_with(claim, claim_docs),
        )
        .api_route(
            "/daily-rewards/claim-all",
            routing::post_with(claim_all, claim_all_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get(
    State(service): State<Arc<impl DailyRewardFeatureService>>,
    token: ApiToken,
) -> Response {
    match service.get_today(&token.0).await {
        Ok(response) => Json(response.snapshot).into_response(),
        Err(DailyRewardGetError::FeatureDisabled) => FeatureDisabledError.into_response(),
        Err(DailyRewardGetError::Auth(err)) => auth_error(err),
        Err(DailyRewardGetError::Other(err)) => internal_server_error(err),
    }
}

fn get_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the daily rewards snapshot for the authenticated user.")
        .add_response::<DailyRewardsSnapshot>(StatusCode::OK, None)
        .add_error::<FeatureDisabledError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn claim(
    State(service): State<Arc<impl DailyRewardFeatureService>>,
    token: ApiToken,
    Path(PathDailyRewardCategory { category }): Path<PathDailyRewardCategory>,
) -> Response {
    match service.claim(&token.0, category).await {
        Ok(result) => Json(ApiDailyRewardClaimResponse::from(result)).into_response(),
        Err(DailyRewardClaimError::FeatureDisabled) => FeatureDisabledError.into_response(),
        Err(DailyRewardClaimError::Auth(err)) => auth_error(err),
        Err(DailyRewardClaimError::NotReady) => RewardNotReadyError.into_response(),
        Err(DailyRewardClaimError::Unavailable) => RewardUnavailableError.into_response(),
        Err(DailyRewardClaimError::AlreadyClaimed) => RewardAlreadyClaimedError.into_response(),
        Err(DailyRewardClaimError::Other(err)) => internal_server_error(err),
    }
}

fn claim_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Claim a single daily reward category.")
        .add_response::<ApiDailyRewardClaimResponse>(StatusCode::OK, None)
        .add_error::<RewardNotReadyError>()
        .add_error::<RewardUnavailableError>()
        .add_error::<RewardAlreadyClaimedError>()
        .add_error::<FeatureDisabledError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn claim_all(
    State(service): State<Arc<impl DailyRewardFeatureService>>,
    token: ApiToken,
) -> Response {
    match service.claim_all(&token.0).await {
        Ok(result) => Json(ApiDailyRewardClaimAllResponse::from(result)).into_response(),
        Err(DailyRewardClaimAllError::FeatureDisabled) => FeatureDisabledError.into_response(),
        Err(DailyRewardClaimAllError::Auth(err)) => auth_error(err),
        Err(DailyRewardClaimAllError::Other(err)) => internal_server_error(err),
    }
}

fn claim_all_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Claim all available daily rewards.")
        .add_response::<ApiDailyRewardClaimAllResponse>(StatusCode::OK, None)
        .add_error::<FeatureDisabledError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// The daily rewards feature is disabled for this environment.
    pub FeatureDisabledError(NOT_FOUND, "daily_rewards_feature_disabled");
    /// The reward cannot be claimed yet.
    pub RewardNotReadyError(CONFLICT, "reward_not_ready");
    /// The reward is currently unavailable.
    pub RewardUnavailableError(CONFLICT, "reward_unavailable");
    /// The reward has already been claimed.
    pub RewardAlreadyClaimedError(CONFLICT, "reward_already_claimed");
}
