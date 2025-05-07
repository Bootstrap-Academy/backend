//! Gracefully handle panics in request handlers

use std::panic::AssertUnwindSafe;

use aide::axum::ApiRouter;
use anyhow::anyhow;
use axum::{
    extract::Request,
    middleware::{Next, from_fn},
    response::Response,
};
use futures::FutureExt;

use crate::errors::internal_server_error;

pub fn add<S: Clone + Send + Sync + 'static>(router: ApiRouter<S>) -> ApiRouter<S> {
    router.layer(from_fn(middleware))
}

async fn middleware(request: Request, next: Next) -> Response {
    match AssertUnwindSafe(next.run(request)).catch_unwind().await {
        Ok(response) => response,
        Err(_) => internal_server_error(anyhow!("request handler panicked")),
    }
}
