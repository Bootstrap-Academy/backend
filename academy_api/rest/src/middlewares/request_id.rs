//! Assign each request a unique ID

use aide::axum::ApiRouter;
use axum::{
    extract::Request,
    middleware::{Next, from_fn},
    response::{IntoResponse, Response},
};
use base64::{display::Base64Display, engine::general_purpose::STANDARD_NO_PAD};
use uuid::Uuid;

pub fn add<S: Clone + Send + Sync + 'static>(router: ApiRouter<S>) -> ApiRouter<S> {
    router.layer(from_fn(middleware))
}

async fn middleware(mut request: Request, next: Next) -> Response {
    let request_id = RequestId::new();
    request.extensions_mut().insert(request_id);
    let response = next.run(request).await;
    ([("X-Request-Id", request_id.to_string())], response).into_response()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(pub Uuid);

impl RequestId {
    fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Base64Display::new(self.0.as_bytes(), &STANDARD_NO_PAD).fmt(f)
    }
}
