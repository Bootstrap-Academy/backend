use std::convert::Infallible;

use aide::OperationInput;
use axum::{
    extract::FromRequestParts,
    http::{header::USER_AGENT, request::Parts},
};

/// Extract the contents of the User-Agent header
pub struct UserAgent(pub Option<String>);

impl<S: Sync> FromRequestParts<S> for UserAgent {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(header) = parts.headers.get(USER_AGENT) else {
            return Ok(Self(None));
        };

        let value = String::from_utf8_lossy(header.as_bytes()).into_owned();

        Ok(Self(Some(value)))
    }
}

impl OperationInput for UserAgent {}
