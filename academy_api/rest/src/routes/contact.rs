use std::sync::Arc;

use academy_core_contact_contracts::{ContactFeatureService, ContactSendMessageError};
use academy_models::RecaptchaResponse;
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
use serde::Deserialize;

use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{RecaptchaFailedError, internal_server_error, internal_server_error_docs},
    models::{OkResponse, StringOption, contact::ApiContactMessage},
};

pub const TAG: &str = "Contact";

pub fn router(service: Arc<impl ContactFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/contact",
            routing::post_with(send_message, send_message_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

#[derive(Deserialize, JsonSchema)]
struct SendMessageRequest {
    #[serde(flatten)]
    message: ApiContactMessage,
    /// reCAPTCHA response. Only evaluated if reCAPTCHA is enabled.
    #[serde(default)]
    recaptcha_response: StringOption<RecaptchaResponse>,
}

async fn send_message(
    service: State<Arc<impl ContactFeatureService>>,
    Json(SendMessageRequest {
        message,
        recaptcha_response,
    }): Json<SendMessageRequest>,
) -> Response {
    match service
        .send_message(message.into(), recaptcha_response.into())
        .await
    {
        Ok(()) => Json(OkResponse).into_response(),
        Err(ContactSendMessageError::Recaptcha) => RecaptchaFailedError.into_response(),
        Err(ContactSendMessageError::Send) => CouldNotSendMessageError.into_response(),
        Err(ContactSendMessageError::Other(err)) => internal_server_error(err),
    }
}

fn send_message_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Send a message to the support team.")
        .description("A reCAPTCHA response is required if reCAPTCHA is enabled.")
        .add_response::<OkResponse>(StatusCode::OK, "The message has been sent.")
        .add_error::<RecaptchaFailedError>()
        .add_error::<CouldNotSendMessageError>()
        .with(internal_server_error_docs)
}

error_code! {
    /// The message could not be sent.
    CouldNotSendMessageError(INTERNAL_SERVER_ERROR, "Could not send message");
}
