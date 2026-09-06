//! Record every state changing request made with an administrator's access
//! token in the administrative audit log, plus the reads listed in
//! [`AUDITED_READ_ROUTES`].

use std::sync::Arc;

use academy_core_admin_audit_contracts::{AdminAuditFeatureService, AdminAuditRequest};
use academy_models::{
    admin_audit::{RequestId as AuditRequestId, RequestMethod, RequestPath},
    auth::AccessToken,
};
use aide::axum::ApiRouter;
use axum::{
    extract::{MatchedPath, Request},
    http::{Method, header::AUTHORIZATION},
    middleware::{Next, from_fn},
    response::Response,
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
/// to leave a trace like a change would.
const AUDITED_READ_ROUTES: &[&str] = &[crate::routes::user::EXPORT_ROUTE];

async fn middleware(
    service: Arc<impl AdminAuditFeatureService>,
    request: Request,
    next: Next,
) -> Response {
    if !is_recorded(&request) {
        return next.run(request).await;
    }

    let Some(token) = access_token(&request) else {
        return next.run(request).await;
    };

    let method = RequestMethod::from_string_truncated(request.method().to_string());
    // The query string may contain secrets and is therefore not recorded.
    let path = RequestPath::from_string_truncated(request.uri().path().to_owned());
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched_path| RequestPath::from_string_truncated(matched_path.as_str().to_owned()));
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|request_id| AuditRequestId::from_string_truncated(request_id.to_string()));

    let response = next.run(request).await;

    // Without a request id the entry could not be tied back to the logs, and
    // its absence means the request id middleware is missing.
    let Some(request_id) = request_id else {
        error!("cannot record an administrative request without a request id");
        return response;
    };

    if let Err(err) = service
        .record(AdminAuditRequest {
            token,
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
