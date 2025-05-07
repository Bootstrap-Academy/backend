use std::sync::Arc;

use academy_core_config_contracts::ConfigFeatureService;
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

use crate::docs::TransformOperationExt;

pub const TAG: &str = "Config";

pub fn router(service: Arc<impl ConfigFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/recaptcha",
            routing::get_with(get_recaptcha_sitekey, get_recaptcha_sitekey_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_recaptcha_sitekey(service: State<Arc<impl ConfigFeatureService>>) -> Response {
    Json(service.get_recaptcha_sitekey()).into_response()
}

fn get_recaptcha_sitekey_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the public reCAPTCHA sitekey.")
        .description("Returns `null` if reCAPTCHA is disabled.")
        .add_response_with::<Option<&str>>(StatusCode::OK, None, |op| {
            op.example("recaptcha-sitekey")
        })
}
