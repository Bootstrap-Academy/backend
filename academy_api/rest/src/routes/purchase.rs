use crate::extractors::auth::ApiToken;
use academy_core_purchase_contracts::{PurchaseError, PurchaseFeatureService};
use academy_models::{
    auth::{AccessToken, InternalToken},
    purchase::{PurchaseAcceptance, PurchaseProduct},
};
use aide::axum::ApiRouter;
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

pub fn router(service: Arc<impl PurchaseFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .route("/shop/purchases", routing::get(list))
        .route("/shop/purchases/offers/{kind}", routing::post(offer))
        .route("/shop/purchases/accept", routing::post(accept))
        .route("/shop/purchases/{id}", routing::get(get))
        .route(
            "/shop/purchases/{id}/documents/{kind}",
            routing::get(document),
        )
        .route(
            "/shop/_internal/purchase-offers/{source}/{user}",
            routing::post(external_offer),
        )
        .route(
            "/shop/_internal/purchases/{source}/{user}",
            routing::post(external_accept),
        )
        .route(
            "/shop/_internal/purchase-status/{user}/{id}",
            routing::get(external_get),
        )
        .route(
            "/shop/_internal/purchase-fulfillment/{source}/{user}/{id}",
            routing::post(external_complete),
        )
        .with_state(service)
}
fn response<T: serde::Serialize>(result: Result<T, PurchaseError>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            let status = match e {
                PurchaseError::NotFound => StatusCode::NOT_FOUND,
                PurchaseError::OfferRequired => StatusCode::CONFLICT,
                PurchaseError::ContactRequired | PurchaseError::Unavailable => {
                    StatusCode::PRECONDITION_FAILED
                }
                PurchaseError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let detail = if status == StatusCode::INTERNAL_SERVER_ERROR {
                "Purchase processing unavailable; retain order identity".into()
            } else {
                e.to_string()
            };
            (status, Json(serde_json::json!({"detail":detail}))).into_response()
        }
    }
}
async fn offer(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<AccessToken>,
    Path(kind): Path<String>,
) -> Response {
    response(s.offer(&t.0, &kind).await)
}
async fn accept(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<AccessToken>,
    Json(a): Json<PurchaseAcceptance>,
) -> Response {
    response(s.accept(&t.0, a).await)
}
async fn get(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<AccessToken>,
    Path(id): Path<Uuid>,
) -> Response {
    response(s.get(&t.0, id).await)
}
async fn list(s: State<Arc<impl PurchaseFeatureService>>, t: ApiToken<AccessToken>) -> Response {
    response(s.list(&t.0).await)
}
async fn document(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<AccessToken>,
    Path((id, kind)): Path<(Uuid, String)>,
) -> Response {
    match s.document(&t.0, id, &kind).await {
        Ok(bytes) => (
            [
                (
                    header::CONTENT_TYPE,
                    if matches!(
                        kind.as_str(),
                        "confirmation"
                            | "fulfillment"
                            | "timing"
                            | "fulfillment-original"
                            | "timing-original"
                    ) {
                        "text/plain; charset=utf-8"
                    } else {
                        "application/pdf"
                    },
                ),
                (header::CONTENT_DISPOSITION, "attachment"),
                (header::CACHE_CONTROL, "private, no-store"),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => response::<()>(Err(e)),
    }
}
#[derive(Deserialize)]
struct ExternalPath {
    source: String,
    user: Uuid,
}
async fn external_offer(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<InternalToken>,
    Path(p): Path<ExternalPath>,
    Json(product): Json<PurchaseProduct>,
) -> Response {
    response(
        s.external_offer(&t.0, p.user.into(), &p.source, product)
            .await,
    )
}
async fn external_accept(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<InternalToken>,
    Path(p): Path<ExternalPath>,
    Json(a): Json<PurchaseAcceptance>,
) -> Response {
    response(s.external_accept(&t.0, p.user.into(), &p.source, a).await)
}
async fn external_get(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<InternalToken>,
    Path((user, id)): Path<(Uuid, Uuid)>,
) -> Response {
    response(s.external_get(&t.0, user.into(), id).await)
}

async fn external_complete(
    s: State<Arc<impl PurchaseFeatureService>>,
    t: ApiToken<InternalToken>,
    Path((source, user, id)): Path<(String, Uuid, Uuid)>,
    Json(result): Json<serde_json::Value>,
) -> Response {
    response(
        s.external_complete(&t.0, user.into(), &source, id, result)
            .await,
    )
}
