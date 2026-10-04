use std::sync::Arc;

use academy_core_user_contracts::publication::{PublicationError, PublicationFeatureService};
use academy_models::{
    auth::{AccessToken, AuthError, AuthenticateError, InternalToken},
    publication::{
        PublicationChoice, PublicationChoiceResult, PublicationEpoch, PublicationPreview,
        PublicationSettings, PublicationSnapshot, PublicationWithdrawal,
    },
    user::UserId,
};
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Path, Request, State},
    http::{
        HeaderValue, StatusCode,
        header::{CACHE_CONTROL, VARY},
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{docs::TransformOperationExt, errors::auth_error, extractors::auth::ApiToken};

pub const SUPPORT_ROUTE: &str = "/auth/admin/users/{user_id}/publication";

pub fn router(service: Arc<impl PublicationFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            SUPPORT_ROUTE,
            routing::get_with(support_settings, support_settings_docs),
        )
        .api_route(
            "/auth/admin/users/{user_id}/publication/withdraw",
            routing::post_with(support_withdraw, support_withdraw_docs),
        )
        .api_route(
            "/auth/users/me/publication",
            routing::get_with(settings, settings_docs).put_with(choose, choice_docs),
        )
        .api_route(
            "/auth/users/me/publication-preview",
            routing::get_with(preview, preview_docs),
        )
        .api_route(
            "/auth/_internal/profile-publications/epoch",
            routing::get_with(epoch, epoch_docs),
        )
        .api_route(
            "/auth/_internal/profile-publications/snapshot",
            routing::get_with(snapshot, snapshot_docs),
        )
        .layer(middleware::from_fn(no_store))
        .with_state(service)
        .with_path_items(|op| op.tag("User"))
}

async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
        .headers_mut()
        .append(VARY, HeaderValue::from_static("Authorization"));
    response
}

fn respond<T: Serialize>(result: Result<T, PublicationError>) -> Response {
    let error = match result {
        Ok(value) => return Json(value).into_response(),
        Err(error) => error,
    };
    let (status, detail) = match error {
        PublicationError::Other(error)
        | PublicationError::Auth(AuthError::Authenticate(AuthenticateError::Other(error))) => {
            tracing::warn!("publication authority unavailable: {error:#}");
            (StatusCode::SERVICE_UNAVAILABLE, "publication_unavailable")
        }
        PublicationError::Auth(error) => return auth_error(error),
        PublicationError::InternalAuth => (StatusCode::UNAUTHORIZED, "invalid_token"),
        PublicationError::Disabled => (StatusCode::SERVICE_UNAVAILABLE, "publication_unavailable"),
        PublicationError::NotFound => (StatusCode::NOT_FOUND, "user_not_found"),
        PublicationError::Conflict => (StatusCode::CONFLICT, "publication_conflict"),
        PublicationError::InvalidPreview => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "publication_preview_required",
        ),
        PublicationError::Unverified => (StatusCode::FORBIDDEN, "email_not_verified"),
        PublicationError::NotOwnerSignIn => (StatusCode::FORBIDDEN, "owner_sign_in_required"),
    };
    (status, Json(serde_json::json!({"detail":detail}))).into_response()
}

async fn settings(
    service: State<Arc<impl PublicationFeatureService>>,
    token: ApiToken<AccessToken>,
) -> Response {
    respond(service.settings(&token.0).await)
}
async fn support_settings(
    service: State<Arc<impl PublicationFeatureService>>,
    token: ApiToken<AccessToken>,
    Path(user_id): Path<UserId>,
) -> Response {
    respond(service.support_settings(&token.0, user_id).await)
}
async fn support_withdraw(
    service: State<Arc<impl PublicationFeatureService>>,
    token: ApiToken<AccessToken>,
    Path(user_id): Path<UserId>,
    Json(withdrawal): Json<PublicationWithdrawal>,
) -> Response {
    respond(
        service
            .support_withdraw(&token.0, user_id, withdrawal)
            .await,
    )
}
async fn preview(
    service: State<Arc<impl PublicationFeatureService>>,
    token: ApiToken<AccessToken>,
) -> Response {
    respond(service.preview(&token.0).await)
}
async fn choose(
    service: State<Arc<impl PublicationFeatureService>>,
    token: ApiToken<AccessToken>,
    Json(choice): Json<PublicationChoice>,
) -> Response {
    respond(service.choose(&token.0, choice).await)
}
async fn epoch(
    service: State<Arc<impl PublicationFeatureService>>,
    token: ApiToken<InternalToken>,
) -> Response {
    respond(service.epoch(&token.0).await)
}
async fn snapshot(
    service: State<Arc<impl PublicationFeatureService>>,
    token: ApiToken<InternalToken>,
) -> Response {
    respond(service.snapshot(&token.0).await)
}

fn settings_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Read the owner's publication choice.")
        .add_response::<PublicationSettings>(StatusCode::OK, None)
}
fn support_settings_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Read a private publication choice for support (admin and MFA required).")
        .add_response::<PublicationSettings>(StatusCode::OK, None)
}
fn support_withdraw_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Withdraw a publication for support; consent cannot be given for another account.")
        .add_response::<PublicationChoiceResult>(StatusCode::OK, None)
}
fn preview_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Preview the exact publication scope without writing.")
        .add_response::<PublicationPreview>(StatusCode::OK, None)
}
fn choice_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Record an owner choice with CAS and bounded replay receipts.")
        .description(
            "Only a session from the owner's own sign-in can share, make private again or confirm \
             the notice. A session an administrator opened in the account is refused with `403` \
             and `owner_sign_in_required`, also after refreshes; so is a session from before \
             origins were recorded that has no device name. Support uses the withdrawal route.",
        )
        .add_response::<PublicationChoiceResult>(StatusCode::OK, None)
}
fn epoch_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Read the current publication authority epoch.")
        .add_response::<PublicationEpoch>(StatusCode::OK, None)
}
fn snapshot_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Read one authoritative minimal participant snapshot.")
        .add_response::<PublicationSnapshot>(StatusCode::OK, None)
}
