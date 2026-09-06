//! Extract the client's IP address from the request.
//!
//! Uses the contents of a configured header (e.g. `X-Real-Ip`) or as a fallback
//! the socket address of the TCP client.
//!
//! The header is only read when the request comes from the configured reverse
//! proxy. Everything the header is used for — the rate limit of the consumer
//! declaration endpoints, the request log — would otherwise be under the
//! control of whoever sends the request.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use aide::axum::ApiRouter;
use axum::{
    extract::{ConnectInfo, Request},
    middleware::{Next, from_fn},
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

        if *set_from != client_ip {
            // client is not the reverse proxy, fall back to socket address
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

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use axum::body::Body;

    use super::*;

    const PROXY: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    const PEER: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));
    const FORWARDED: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 42));

    fn config() -> RestServerRealIpConfig {
        RestServerRealIpConfig {
            header: "X-Real-Ip".into(),
            set_from: PROXY,
        }
    }

    fn request(peer: IpAddr, header: Option<&str>) -> Request {
        let mut builder = Request::builder().uri("/");
        if let Some(header) = header {
            builder = builder.header("X-Real-Ip", header);
        }
        let mut request = builder.body(Body::empty()).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(peer, 1234)));
        request
    }

    /// Without a configured header the socket address is the client address.
    #[test]
    fn no_header_configured() {
        let request = request(PEER, Some("203.0.113.42"));
        assert_eq!(ClientIp::from_request(&request, None), ClientIp(PEER));
    }

    /// The header is read when the request comes from the reverse proxy.
    #[test]
    fn header_from_the_proxy() {
        let request = request(PROXY, Some("203.0.113.42"));
        assert_eq!(
            ClientIp::from_request(&request, Some(&config())),
            ClientIp(FORWARDED)
        );
    }

    /// A client that reaches the api directly cannot choose its own address.
    #[test]
    fn header_from_anybody_else_is_ignored() {
        let request = request(PEER, Some("203.0.113.42"));
        assert_eq!(
            ClientIp::from_request(&request, Some(&config())),
            ClientIp(PEER)
        );
    }

    /// A missing or unparsable header falls back to the socket address instead
    /// of putting every request into the same rate limit bucket.
    #[test]
    fn missing_or_invalid_header() {
        for header in [None, Some("not an ip"), Some("203.0.113.42, 198.51.100.7")] {
            let request = request(PROXY, header);
            assert_eq!(
                ClientIp::from_request(&request, Some(&config())),
                ClientIp(PROXY),
                "{header:?}"
            );
        }
    }
}
