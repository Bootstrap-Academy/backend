//! Trace incoming requests

use std::time::Duration;

use aide::axum::ApiRouter;
use axum::{
    extract::{MatchedPath, Request},
    response::Response,
};
use tracing::{Span, debug};

use super::request_id::RequestId;
use crate::middlewares::client_ip::ClientIp;

pub fn add<S: Clone + Send + Sync + 'static>(router: ApiRouter<S>) -> ApiRouter<S> {
    router.layer(
        tower_http::trace::TraceLayer::new_for_http()
            .make_span_with(make_span)
            .on_request(on_request)
            .on_response(on_response)
            .on_body_chunk(())
            .on_eos(())
            .on_failure(()),
    )
}

fn make_span(request: &Request) -> Span {
    let version = request.version();
    let method = request.method();
    // Credentials can be path segments (finance downloads) or query values.
    // Log the static route pattern, including for rejected requests. Never fall
    // back to an unmatched caller-controlled URI.
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("<unmatched>", MatchedPath::as_str);
    let client_ip = request.extensions().get::<ClientIp>().unwrap().0;
    let request_id = *request.extensions().get::<RequestId>().unwrap();

    tracing::debug_span!("http-request", ?version, %method, %route, %client_ip, %request_id)
}

fn on_request(_request: &Request, _span: &Span) {
    debug!("started processing request")
}

fn on_response(response: &Response, latency: Duration, _span: &Span) {
    let status = response.status();
    debug!(?latency, %status, "finished processing request")
}
