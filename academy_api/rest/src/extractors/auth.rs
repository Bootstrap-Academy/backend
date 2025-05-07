use std::convert::Infallible;

use academy_models::auth::{AccessToken, InternalToken};
use aide::{OperationInput, generate::GenContext, openapi::Operation};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts},
};

/// Extract Bearer API tokens from the Authorization header
pub struct ApiToken<T: ApiTokenType = AccessToken>(pub T);

pub trait ApiTokenType: for<'a> From<&'a str> + private::Sealed {
    const NAME: &str;
}

impl ApiTokenType for AccessToken {
    const NAME: &str = "Token";
}
impl ApiTokenType for InternalToken {
    const NAME: &str = "InternalToken";
}

mod private {
    use super::*;
    pub trait Sealed {}
    impl Sealed for AccessToken {}
    impl Sealed for InternalToken {}
}

impl<S: Send + Sync, T: ApiTokenType> FromRequestParts<S> for ApiToken<T> {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .headers
                .get(AUTHORIZATION)
                .and_then(|x| x.to_str().ok())
                .map(|x| x.strip_prefix("Bearer ").unwrap_or(x))
                .unwrap_or_default()
                .into(),
        ))
    }
}

impl<T: ApiTokenType> OperationInput for ApiToken<T> {
    fn operation_input(_ctx: &mut GenContext, operation: &mut Operation) {
        operation
            .security
            .push([(T::NAME.into(), Vec::new())].into());
    }
}
