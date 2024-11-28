use academy_models::auth::{AuthError, AuthenticateError, AuthorizeError};
use aide::transform::TransformOperation;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::Serialize;

use crate::{docs::TransformOperationExt, error_code};

/// Handle an internal server error
pub fn internal_server_error(err: impl Into<anyhow::Error>) -> Response {
    let err = err.into();
    tracing::error!("internal server error: {err}");
    InternalServerError.into_response()
}

pub fn internal_server_error_docs(op: TransformOperation) -> TransformOperation {
    op.add_error::<InternalServerError>()
}

pub fn auth_error(err: AuthError) -> Response {
    match err {
        AuthError::Authenticate(AuthenticateError::InvalidToken) => {
            InvalidTokenError.into_response()
        }
        AuthError::Authenticate(AuthenticateError::Other(err)) => internal_server_error(err),
        AuthError::Authorize(AuthorizeError::Admin) => PermissionDeniedError.into_response(),
        AuthError::Authorize(AuthorizeError::EmailVerified) => {
            EmailNotVerifiedError.into_response()
        }
    }
}

pub fn auth_error_docs(op: TransformOperation) -> TransformOperation {
    op.add_error::<InvalidTokenError>()
        .with(internal_server_error_docs)
        .add_error::<PermissionDeniedError>()
        .add_error::<EmailNotVerifiedError>()
}

/// A simple error response containing only the error code
#[derive(Serialize, JsonSchema, Default)]
pub struct ApiError<C: ApiErrorCode> {
    #[serde(rename = "detail")]
    pub code: C,
}

pub trait ApiErrorCode: Serialize + JsonSchema + Default {
    const DESCRIPTION: &str;
    const STATUS_CODE: StatusCode;
}

error_code! {
    /// Internal server error
    InternalServerError(INTERNAL_SERVER_ERROR, "Internal server error");

    /// The authentication token is invalid or has expired.
    pub InvalidTokenError(UNAUTHORIZED, "Invalid token");
    /// The authenticated user is not allowed to perform this action.
    pub PermissionDeniedError(FORBIDDEN, "Permission denied");
    /// The authenticated user has not verified their email address.
    EmailNotVerifiedError(FORBIDDEN, "Email not verified");

    /// reCAPTCHA is enabled but no valid reCAPTCHA response has been provided.
    pub RecaptchaFailedError(PRECONDITION_FAILED, "Recaptcha failed");
}
