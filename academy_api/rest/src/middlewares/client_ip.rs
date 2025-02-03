//! Extract the client's IP address from the request.
//!
//! Uses the contents of a configured header (e.g. `X-Real-Ip`) or as a fallback
//! the socket address of the TCP client.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use aide::axum::ApiRouter;
use axum::{
    extract::{ConnectInfo, Request},
    middleware::{from_fn, Next},
};
use tracing::{debug, error, warn};

use crate::RestServerRealIpConfig;

pub fn add<S: Clone + Send + Sync + 'static>(
    real_ip_config: Option<Arc<RestServerRealIpConfig>>,
) -> impl FnOnce(ApiRouter<S>) -> ApiRouter<S> {
    |router| {
        router.layer(from_fn(move |mut request: Request, next: Next| {
            let client_ip = ClientIp::from_request(&request, real_ip_config.as_deref());
            request.extensions_mut().insert(client_ip);
            next.run(request)
        }))
    }
}

/// The IP address of the HTTP client
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientIp(pub IpAddr);

impl ClientIp {
    fn from_request(request: &Request, real_ip_config: Option<&RestServerRealIpConfig>) -> Self {
        let client_ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .unwrap()
            .ip();

        let Some(RestServerRealIpConfig { header, set_from }) = real_ip_config else {
            // no real ip header configured, fall back to socket address
            return Self(client_ip);
        };

        let header_value = request.headers().get(header);

        if set_from.is_some_and(|set_from| set_from != client_ip) {
            // client is not trusted, fall back to socket address
            if let Some(header_value) = header_value {
                debug!(%client_ip, ?header_value, "ignoring real ip header value from untrusted source");
            }
            return Self(client_ip);
        }

        let Some(header_value) = header_value else {
            // client did not include the expected real ip header,
            // fall back to socket address
            warn!(%client_ip, "real ip header not found");
            return Self(client_ip);
        };

        let Some(real_ip) = header_value
            .to_str()
            .ok()
            .and_then(|real_ip| real_ip.parse().ok())
        else {
            // invalid real ip header value, fall back to socket address
            error!(%client_ip, ?header_value, "failed to parse real ip header value");
            return Self(client_ip);
        };

        ClientIp(real_ip)
    }
}
