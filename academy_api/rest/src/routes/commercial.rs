use crate::extractors::auth::ApiToken;
use academy_core_moderation_contracts::{
    RecipientAccessError, RecipientCredentials,
    commercial::{CommercialCredentials, CommercialFeatureService},
};
use academy_models::auth::{AccessToken, InternalToken};
use aide::axum::{ApiRouter, routing};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn router(service: Arc<impl CommercialFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route("/shop/claims/documents", routing::get(inventory))
        .api_route("/shop/learning/{operation}", routing::post(learning))
        .api_route(
            "/shop/claims/recipient/{operation}",
            routing::post(recipient),
        )
        .api_route("/shop/claims/admin/{operation}", routing::post(admin))
        .api_route(
            "/shop/claims/admin/cases/{case_id}/documents/{kind}/{id}/{variant}",
            routing::get(admin_document),
        )
        .api_route(
            "/shop/claims/purchases/{offer}/status",
            routing::get(purchase_status),
        )
        .api_route(
            "/shop/_internal/claims/{operation}",
            routing::post(internal),
        )
        .api_route(
            "/shop/claims/documents/{kind}/{id}/{variant}",
            routing::get(document),
        )
        .with_state(service)
}

async fn learning(
    State(s): State<Arc<impl CommercialFeatureService>>,
    headers: HeaderMap,
    Path(op): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let Some(key) = headers.get("x-learning-key").and_then(|v| v.to_str().ok()) else {
        return reply(Err(RecipientAccessError::Invalid.into()));
    };
    reply(s.commercial_learning(key, &op, body).await)
}

fn credentials(headers: &HeaderMap) -> Result<CommercialCredentials, RecipientAccessError> {
    // Presence selects the proof. Invalid selected bytes must never become an
    // absent credential that allows a lower-priority identity to take over.
    if let Some(key) = headers.get("x-commercial-claim-key") {
        return Ok(CommercialCredentials {
            recipient: RecipientCredentials {
                ordinary: None,
                capability: None,
            },
            claim_key: Some(
                key.to_str()
                    .map_err(|_| RecipientAccessError::Invalid)?
                    .to_owned(),
            ),
        });
    }
    if let Some(capability) = headers.get("x-moderation-capability") {
        return Ok(CommercialCredentials {
            recipient: RecipientCredentials {
                ordinary: None,
                capability: Some(
                    capability
                        .to_str()
                        .map_err(|_| RecipientAccessError::Invalid)?
                        .to_owned(),
                ),
            },
            claim_key: None,
        });
    }
    Ok(CommercialCredentials {
        recipient: RecipientCredentials {
            ordinary: headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(AccessToken::from),
            capability: None,
        },
        claim_key: None,
    })
}

async fn recipient(
    State(s): State<Arc<impl CommercialFeatureService>>,
    headers: HeaderMap,
    Path(op): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let credentials = match credentials(&headers) {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    reply(s.commercial_recipient(credentials, &op, body).await)
}
async fn admin(
    State(s): State<Arc<impl CommercialFeatureService>>,
    token: ApiToken,
    Path(op): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    reply(s.commercial_admin(&token.0, &op, body).await)
}
async fn internal(
    State(s): State<Arc<impl CommercialFeatureService>>,
    token: ApiToken<InternalToken>,
    Path(op): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    reply(s.commercial_internal(&token.0, &op, body).await)
}
async fn document(
    State(s): State<Arc<impl CommercialFeatureService>>,
    headers: HeaderMap,
    Path((kind, id, variant)): Path<(String, String, String)>,
) -> Response {
    let credentials = match credentials(&headers) {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    document_reply(
        s.commercial_document(credentials, &kind, &id, &variant)
            .await,
        &kind,
        &variant,
    )
}
async fn admin_document(
    State(s): State<Arc<impl CommercialFeatureService>>,
    token: ApiToken,
    Path((case_id, kind, id, variant)): Path<(String, String, String, String)>,
) -> Response {
    let case_id = match uuid::Uuid::parse_str(&case_id) {
        Ok(value) => value,
        Err(_) => return reply(Err(RecipientAccessError::Malformed.into())),
    };
    document_reply(
        s.commercial_admin_document(&token.0, case_id, &kind, &id, &variant)
            .await,
        &kind,
        &variant,
    )
}
fn document_reply(result: anyhow::Result<Vec<u8>>, kind: &str, variant: &str) -> Response {
    match result {
        Ok(bytes) => (
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::REFERRER_POLICY, "no-referrer"),
                (header::CONTENT_DISPOSITION, "attachment"),
                (
                    header::CONTENT_TYPE,
                    if kind == "purchase" && !matches!(variant, "terms" | "withdrawal") {
                        "text/plain; charset=utf-8"
                    } else {
                        "application/pdf"
                    },
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => reply(Err(e)),
    }
}

async fn purchase_status(
    State(s): State<Arc<impl CommercialFeatureService>>,
    headers: HeaderMap,
    Path(offer): Path<String>,
) -> Response {
    let credentials = match credentials(&headers) {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    let offer = match uuid::Uuid::parse_str(&offer) {
        Ok(value) => value,
        Err(_) => return reply(Err(RecipientAccessError::Malformed.into())),
    };
    reply(
        s.commercial_purchase_status(credentials, offer)
            .await
            .and_then(|value| serde_json::to_value(value).map_err(Into::into)),
    )
}

fn reply(result: anyhow::Result<Value>) -> Response {
    let (status, body) = match result {
        Ok(body) => (StatusCode::OK, body),
        Err(error) => {
            let status=match error.downcast_ref::<RecipientAccessError>() {
                Some(RecipientAccessError::Invalid)=>StatusCode::UNAUTHORIZED,
                Some(RecipientAccessError::Scope|RecipientAccessError::Forbidden)=>StatusCode::FORBIDDEN,
                Some(RecipientAccessError::Malformed)=>StatusCode::BAD_REQUEST,
                Some(RecipientAccessError::Conflict)=>StatusCode::CONFLICT,
                Some(RecipientAccessError::NotFound)=>StatusCode::NOT_FOUND,
                _ if error.downcast_ref::<academy_models::auth::AuthorizeError>().is_some()=>StatusCode::FORBIDDEN,
                _ if error.downcast_ref::<academy_models::auth::AuthenticateError>().is_some()
                    || error.downcast_ref::<academy_auth_contracts::internal::AuthInternalAuthenticateError>().is_some()=>StatusCode::UNAUTHORIZED,
                _ if matches!(error.downcast_ref::<academy_core_finance_contracts::FinanceDownloadError>(),Some(academy_core_finance_contracts::FinanceDownloadError::NotFound))
                    || matches!(error.downcast_ref::<academy_core_purchase_contracts::PurchaseError>(),Some(academy_core_purchase_contracts::PurchaseError::NotFound))=>StatusCode::NOT_FOUND,
                _=>StatusCode::SERVICE_UNAVAILABLE,
            };
            (
                status,
                json!({"detail":"Commercial request not confirmed; preserve the exact request and proof. Existing rights and original deadlines remain."}),
            )
        }
    };
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        Json(body),
    )
        .into_response()
}

#[cfg(test)]
#[path = "commercial_tests.rs"]
mod tests;

async fn inventory(
    State(s): State<Arc<impl CommercialFeatureService>>,
    headers: HeaderMap,
) -> Response {
    let credentials = match credentials(&headers) {
        Ok(value) => value,
        Err(error) => return reply(Err(error.into())),
    };
    reply(
        s.commercial_document_inventory(credentials)
            .await
            .and_then(|value| Ok(serde_json::to_value(value)?)),
    )
}
