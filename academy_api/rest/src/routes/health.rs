use std::sync::Arc;

use academy_core_health_contracts::HealthFeatureService;
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::docs::TransformOperationExt;

pub const TAG: &str = "Health";

pub fn router(service: Arc<impl HealthFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route("/health", routing::get_with(health, health_docs))
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

macro_rules! health {
    ($($ident:ident),* $(,)?) => {
        #[derive(Clone, Copy, Serialize, JsonSchema)]
        struct HealthResponse {
            database: bool,
            cache: bool,
            email: bool,
        }

        const HEALTHY: HealthResponse = HealthResponse {
            $($ident: true),*
        };
        const UNHEALTHY: HealthResponse = HealthResponse {
            $($ident: false),*
        };

        async fn health(service: State<Arc<impl HealthFeatureService>>) -> Response {
            let status = service.get_status().await;

            let ok = true $(&& status.$ident)*;

            let code = if ok {
                StatusCode::OK
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            let response = HealthResponse {
                $($ident: status.$ident),*
            };

            (code, Json(response)).into_response()
        }
    };
}

health!(database, cache, email);

fn health_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the current health status of the backend.")
        .description(
            "For each component a boolean is returned where `true` indicates that the component \
             is healthy, and `false` indicates that the component is unhealthy.\n\nThe response \
             status is `200 OK` if all components are healthy and `500 INTERNAL SERVER ERROR` \
             otherwise.",
        )
        .add_response_with::<HealthResponse>(StatusCode::OK, "All components are healthy", |op| {
            op.example(HEALTHY)
        })
        .add_response_with::<HealthResponse>(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Some components are unhealthy",
            |op| op.example(UNHEALTHY),
        )
}
