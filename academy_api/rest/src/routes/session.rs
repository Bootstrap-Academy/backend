use std::sync::Arc;

use academy_core_session_contracts::{
    SessionCreateCommand, SessionCreateError, SessionDeleteByUserError, SessionDeleteCurrentError,
    SessionDeleteError, SessionFeatureService, SessionGetCurrentError, SessionImpersonateError,
    SessionListByUserError, SessionRefreshError,
};
use academy_models::{
    RecaptchaResponse,
    auth::RefreshToken,
    mfa::{MfaAuthentication, MfaRecoveryCode, TotpCode},
    session::{DeviceName, SessionId},
    user::{UserNameOrEmailAddress, UserPassword},
};
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{
    mfa::InvalidMfaCodeError,
    user::{UserDisabledError, UserNotFoundError},
};
use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{
        RecaptchaFailedError, auth_error, auth_error_docs, internal_server_error,
        internal_server_error_docs,
    },
    extractors::{auth::ApiToken, user_agent::UserAgent},
    models::{
        OkResponse, StringOption,
        session::{ApiLogin, ApiSession},
        user::{ApiUserIdOrSelf, PathUserId, PathUserIdOrSelf},
    },
};

pub const TAG: &str = "Session";

pub fn router(service: Arc<impl SessionFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/session",
            routing::get_with(get_current, get_current_docs)
                .put_with(refresh, refresh_docs)
                .delete_with(delete_current, delete_current_docs),
        )
        .api_route("/auth/sessions", routing::post_with(create, create_docs))
        .api_route(
            "/auth/sessions/{user_id}",
            routing::get_with(list_by_user, list_by_user_docs)
                .post_with(impersonate, impersonate_docs)
                .delete_with(delete_by_user, delete_by_user_docs),
        )
        .api_route(
            "/auth/sessions/{user_id}/{session_id}",
            routing::delete_with(delete, delete_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn get_current(
    session_service: State<Arc<impl SessionFeatureService>>,
    token: ApiToken,
) -> Response {
    match session_service.get_current_session(&token.0).await {
        Ok(session) => Json(ApiSession::from(session)).into_response(),
        Err(SessionGetCurrentError::Auth(err)) => auth_error(err),
        Err(SessionGetCurrentError::Other(err)) => internal_server_error(err),
    }
}

fn get_current_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the currently authenticated session.")
        .add_response::<ApiSession>(StatusCode::OK, None)
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn list_by_user(
    session_service: State<Arc<impl SessionFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match session_service.list_by_user(&token.0, user_id.into()).await {
        Ok(sessions) => Json(
            sessions
                .into_iter()
                .map(Into::into)
                .collect::<Vec<ApiSession>>(),
        )
        .into_response(),
        Err(SessionListByUserError::Auth(err)) => auth_error(err),
        Err(SessionListByUserError::Other(err)) => internal_server_error(err),
    }
}

fn list_by_user_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return all sessions of the given user.")
        .add_response::<Vec<ApiSession>>(StatusCode::OK, None)
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct CreateRequest {
    name_or_email: UserNameOrEmailAddress,
    password: UserPassword,
    mfa_code: StringOption<TotpCode>,
    recovery_code: StringOption<MfaRecoveryCode>,
    recaptcha_response: StringOption<RecaptchaResponse>,
}

async fn create(
    session_service: State<Arc<impl SessionFeatureService>>,
    user_agent: UserAgent,
    Json(CreateRequest {
        name_or_email,
        password,
        mfa_code,
        recovery_code,
        recaptcha_response,
    }): Json<CreateRequest>,
) -> Response {
    match session_service
        .create_session(
            SessionCreateCommand {
                name_or_email,
                password,
                device_name: user_agent.0.map(DeviceName::from_string_truncated),
                mfa: MfaAuthentication {
                    totp_code: mfa_code.into(),
                    recovery_code: recovery_code.into(),
                },
            },
            recaptcha_response.into(),
        )
        .await
    {
        Ok(result) => Json(ApiLogin::from(result)).into_response(),
        Err(SessionCreateError::InvalidCredentials) => InvalidCredentialsError.into_response(),
        Err(SessionCreateError::MfaFailed) => InvalidMfaCodeError.into_response(),
        Err(SessionCreateError::UserDisabled) => UserDisabledError.into_response(),
        Err(SessionCreateError::Recaptcha) => RecaptchaFailedError.into_response(),
        Err(SessionCreateError::Other(err)) => internal_server_error(err),
    }
}

fn create_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create a new session via username/password authentication.")
        .description(
            "If the user has MFA enabled, the current TOTP needs to provided. Alternatively, the \
             recovery code can be used to disable MFA.\n\nAfter too many failed login attempts, a \
             valid reCAPTCHA response is required, if reCAPTCHA is enabled.",
        )
        .add_response::<ApiLogin>(StatusCode::OK, "A new session has been created.")
        .add_error::<InvalidCredentialsError>()
        .add_error::<InvalidMfaCodeError>()
        .add_error::<UserDisabledError>()
        .add_error::<RecaptchaFailedError>()
        .with(internal_server_error_docs)
}

async fn impersonate(
    session_service: State<Arc<impl SessionFeatureService>>,
    token: ApiToken,
    Path(PathUserId { user_id }): Path<PathUserId>,
) -> Response {
    match session_service.impersonate(&token.0, user_id).await {
        Ok(login) => Json(ApiLogin::from(login)).into_response(),
        Err(SessionImpersonateError::NotFound) => UserNotFoundError.into_response(),
        Err(SessionImpersonateError::Auth(err)) => auth_error(err),
        Err(SessionImpersonateError::Other(err)) => internal_server_error(err),
    }
}

fn impersonate_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create a new session for the given user.")
        .add_response::<ApiLogin>(StatusCode::OK, "A new session has been created.")
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct RefreshRequest {
    refresh_token: RefreshToken,
}

async fn refresh(
    session_service: State<Arc<impl SessionFeatureService>>,
    Json(RefreshRequest { refresh_token }): Json<RefreshRequest>,
) -> Response {
    match session_service.refresh_session(&refresh_token).await {
        Ok(login) => Json(ApiLogin::from(login)).into_response(),
        Err(SessionRefreshError::InvalidRefreshToken) => InvalidRefreshTokenError.into_response(),
        Err(SessionRefreshError::Other(err)) => internal_server_error(err),
    }
}

fn refresh_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Refresh session via refresh token")
        .description(
            "Generates and returns a new access/refresh token pair and invalidates the old tokens.",
        )
        .add_response::<ApiLogin>(StatusCode::OK, "The session has been refreshed.")
        .add_error::<InvalidRefreshTokenError>()
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct DeletePath {
    user_id: ApiUserIdOrSelf,
    session_id: SessionId,
}

async fn delete(
    session_service: State<Arc<impl SessionFeatureService>>,
    token: ApiToken,
    Path(DeletePath {
        user_id,
        session_id,
    }): Path<DeletePath>,
) -> Response {
    match session_service
        .delete_session(&token.0, user_id.into(), session_id)
        .await
    {
        Ok(()) => Json(OkResponse).into_response(),
        Err(SessionDeleteError::NotFound) => SessionNotFoundError.into_response(),
        Err(SessionDeleteError::Auth(err)) => auth_error(err),
        Err(SessionDeleteError::Other(err)) => internal_server_error(err),
    }
}

fn delete_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete the given session.")
        .description("Invalidates the access/refresh token pair.")
        .add_error::<SessionNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn delete_current(
    session_service: State<Arc<impl SessionFeatureService>>,
    token: ApiToken,
) -> Response {
    match session_service.delete_current_session(&token.0).await {
        Ok(()) => Json(OkResponse).into_response(),
        Err(SessionDeleteCurrentError::Auth(err)) => auth_error(err),
        Err(SessionDeleteCurrentError::Other(err)) => internal_server_error(err),
    }
}

fn delete_current_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete the currently authenticated session.")
        .description("Invalidates the access/refresh token pair.")
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn delete_by_user(
    session_service: State<Arc<impl SessionFeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match session_service
        .delete_by_user(&token.0, user_id.into())
        .await
    {
        Ok(()) => Json(true).into_response(),
        Err(SessionDeleteByUserError::Auth(err)) => auth_error(err),
        Err(SessionDeleteByUserError::Other(err)) => internal_server_error(err),
    }
}

fn delete_by_user_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete all sessions of the given user.")
        .description("Invalidates any associated access/refresh token pair.")
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

error_code! {
    /// The user does not exist or the password is incorrect.
    InvalidCredentialsError(UNAUTHORIZED, "Invalid credentials");
    /// The session does not exist.
    SessionNotFoundError(NOT_FOUND, "Session not found");
    /// The refresh token is invalid or has expired.
    InvalidRefreshTokenError(UNAUTHORIZED, "Invalid refresh token");
}
