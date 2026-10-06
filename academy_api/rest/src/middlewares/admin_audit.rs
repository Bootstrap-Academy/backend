//! Record every state changing request made with an administrator's access
//! token, or in a session an administrator opened in someone else's account, in
//! the administrative audit log, plus the reads listed in
//! [`AUDITED_READ_ROUTES`].

use std::sync::Arc;

use academy_core_admin_audit_contracts::{
    AdminAuditCapture, AdminAuditCredential, AdminAuditFeatureService, AdminAuditRequest,
};
use academy_models::{
    admin_audit::{RequestId as AuditRequestId, RequestMethod, RequestPath},
    auth::AccessToken,
};
use aide::axum::ApiRouter;
use axum::{
    body::{Body, to_bytes},
    extract::{MatchedPath, Request},
    http::{Method, StatusCode, header::AUTHORIZATION},
    middleware::{Next, from_fn},
    response::{IntoResponse, Response},
};
use tracing::error;

use super::request_id::RequestId;

pub fn add<S: Clone + Send + Sync + 'static>(
    service: Arc<impl AdminAuditFeatureService>,
) -> impl FnOnce(ApiRouter<S>) -> ApiRouter<S> {
    |router| {
        router.layer(from_fn(move |request: Request, next: Next| {
            let service = Arc::clone(&service);
            middleware(service, request, next)
        }))
    }
}

/// Routes whose `GET` is recorded even though it changes nothing.
///
/// The data export hands an administrator everything the platform stores about
/// another user, which is the most far reaching read the API offers, so it has
/// to leave a trace like a change would. The document listing is recorded for
/// the same reason: it is searchable by name and email address, and the final
/// statements in it still name people whose account has been deleted. The user
/// listing carries the full invoice address of every account and is searchable
/// by name and email address, and the declaration listing carries the name, the
/// email address and the free text of every cancellation and withdrawal
/// declaration, so both are of the same reach.
const AUDITED_READ_ROUTES: &[&str] = &[
    crate::routes::publication::SUPPORT_ROUTE,
    crate::routes::user::USERS_ROUTE,
    crate::routes::user::EXPORT_ROUTE,
    crate::routes::finance::DOCUMENTS_ROUTE,
    crate::routes::contract::DECLARATIONS_ROUTE,
];

async fn middleware(
    service: Arc<impl AdminAuditFeatureService>,
    request: Request,
    next: Next,
) -> Response {
    if !is_recorded(&request) {
        return next.run(request).await;
    }

    let method = RequestMethod::from_string_truncated(request.method().to_string());
    // Query values and finance path bearers must never reach audit storage or
    // its instrumented services, including rejected methods and unknown routes.
    let path = audit_path(request.uri().path());
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched_path| RequestPath::from_string_truncated(matched_path.as_str().to_owned()));
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|request_id| AuditRequestId::from_string_truncated(request_id.to_string()));

    let mut rejected = None;
    let (request, captured) =
        if request.method() == Method::PUT && request.uri().path() == "/auth/session" {
            let (parts, body) = request.into_parts();
            let bytes = match to_bytes(body, 2 * 1024 * 1024).await {
                Ok(bytes) => bytes,
                Err(_) => {
                    rejected = Some(StatusCode::PAYLOAD_TOO_LARGE.into_response());
                    Default::default()
                }
            };
            // Use the actual refresh credential even if an unrelated/expired
            // bearer was supplied; it is the refresh credential that acts here.
            let mut captured =
                match serde_json::from_slice::<crate::routes::session::RefreshRequest>(&bytes) {
                    Ok(body) => {
                        service
                            .capture(AdminAuditCredential::Refresh(&body.refresh_token))
                            .await
                    }
                    Err(_) => Ok(AdminAuditCapture::InvalidCredential),
                };
            let request = Request::from_parts(parts, Body::from(bytes));
            // Rejected refresh requests still retain a supplied administrative
            // bearer. A valid ordinary refresh credential must take precedence.
            if matches!(captured, Ok(AdminAuditCapture::InvalidCredential))
                && let Some(token) = access_token(&request)
            {
                captured = service.capture(AdminAuditCredential::Access(&token)).await;
            }
            (request, captured)
        } else {
            let captured = match access_token(&request) {
                Some(token) => service.capture(AdminAuditCredential::Access(&token)).await,
                None => Ok(AdminAuditCapture::InvalidCredential),
            };
            (request, captured)
        };
    let actor = match captured {
        Ok(AdminAuditCapture::Recorded(actor)) => Some(actor),
        Ok(AdminAuditCapture::Unrecorded | AdminAuditCapture::InvalidCredential) => None,
        Err(err) => {
            error!("failed to capture administrative audit attribution: {err:#}");
            if route.as_deref().is_some_and(|route| {
                matches!(
                    route.as_str(),
                    "/auth/users/me/publication"
                        | "/auth/admin/users/{user_id}/publication"
                        | "/auth/admin/users/{user_id}/publication/withdraw"
                )
            }) {
                return crate::routes::publication::audit_unavailable(err);
            }
            return crate::errors::internal_server_error(err);
        }
    };

    let response = match rejected {
        Some(response) => response,
        None => next.run(request).await,
    };

    let Some(actor) = actor else {
        return response;
    };

    // Without a request id the entry could not be tied back to the logs, and
    // its absence means the request id middleware is missing.
    let Some(request_id) = request_id else {
        error!("cannot record an administrative request without a request id");
        return response;
    };

    if let Err(err) = service
        .record(AdminAuditRequest {
            actor,
            method,
            path,
            route,
            status: response.status().as_u16(),
            request_id,
        })
        .await
    {
        error!("failed to record administrative request in the audit log: {err:#}");
    }

    response
}

fn audit_path(path: &str) -> RequestPath {
    for prefix in ["/finance/invoices/", "/finance/credit_notes/"] {
        if path.starts_with(prefix) {
            // A rejected route may repeat the bearer in other segments. Keep
            // the operation, method, status and request id without its URI data.
            return RequestPath::from_string_truncated(format!("{prefix}<redacted>"));
        }
    }
    RequestPath::from_string_truncated(path.to_owned())
}

/// Whether the given request has to be recorded if it is authenticated with an
/// administrator's access token.
fn is_recorded(request: &Request) -> bool {
    is_state_changing(request.method()) || is_audited_read(request)
}

fn is_state_changing(method: &Method) -> bool {
    matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

/// Reading data is not recorded, unless the route is one of the few that hand
/// out more than the endpoints an ordinary user can reach.
fn is_audited_read(request: &Request) -> bool {
    request.method() == Method::GET
        && request
            .extensions()
            .get::<MatchedPath>()
            .is_some_and(|matched_path| AUDITED_READ_ROUTES.contains(&matched_path.as_str()))
}

fn access_token(request: &Request) -> Option<AccessToken> {
    request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.strip_prefix("Bearer ").unwrap_or(value))
        .filter(|value| !value.is_empty())
        .map(Into::into)
}
