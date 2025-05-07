use std::sync::Arc;

use academy_core_oauth2_contracts::{
    OAuth2CreateLinkError, OAuth2CreateSessionError, OAuth2CreateSessionResponse,
    OAuth2DeleteLinkError, OAuth2FeatureService, OAuth2ListLinksError,
};
use academy_models::{
    oauth2::{OAuth2LinkId, OAuth2RegistrationToken},
    session::DeviceName,
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
use serde::{Deserialize, Serialize};

use super::user::{CannotDeleteLastLoginMethodError, UserDisabledError, UserNotFoundError};
use crate::{
    docs::TransformOperationExt,
    error_code,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::{auth::ApiToken, user_agent::UserAgent},
    models::{
        OkResponse,
        oauth2::{ApiOAuth2Link, ApiOAuth2Login, ApiOAuth2ProviderSummary},
        session::ApiLogin,
        user::{ApiUserIdOrSelf, PathUserIdOrSelf},
    },
};

pub const TAG: &str = "OAuth2";

pub fn router(service: Arc<impl OAuth2FeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route(
            "/auth/oauth/providers",
            routing::get_with(list_providers, list_providers_docs),
        )
        .api_route(
            "/auth/oauth/links/{user_id}",
            routing::get_with(list_links, list_links_docs).post_with(create_link, create_link_docs),
        )
        .api_route(
            "/auth/oauth/links/{user_id}/{link_id}",
            routing::delete_with(delete_link, delete_link_docs),
        )
        .api_route(
            "/auth/sessions/oauth",
            routing::post_with(create_session, create_session_docs),
        )
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

async fn list_providers(service: State<Arc<impl OAuth2FeatureService>>) -> Response {
    Json(
        service
            .list_providers()
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiOAuth2ProviderSummary>>(),
    )
    .into_response()
}

fn list_providers_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return all available OAuth2 providers.")
        .add_response::<Vec<ApiOAuth2ProviderSummary>>(StatusCode::OK, None)
}

async fn list_links(
    service: State<Arc<impl OAuth2FeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
) -> Response {
    match service.list_links(&token.0, user_id.into()).await {
        Ok(links) => Json(
            links
                .into_iter()
                .map(Into::into)
                .collect::<Vec<ApiOAuth2Link>>(),
        )
        .into_response(),
        Err(OAuth2ListLinksError::NotFound) => UserNotFoundError.into_response(),
        Err(OAuth2ListLinksError::Auth(err)) => auth_error(err),
        Err(OAuth2ListLinksError::Other(err)) => internal_server_error(err),
    }
}

fn list_links_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return all OAuth2 links of the given user.")
        .add_response::<Vec<ApiOAuth2Link>>(StatusCode::OK, None)
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

async fn create_link(
    service: State<Arc<impl OAuth2FeatureService>>,
    token: ApiToken,
    Path(PathUserIdOrSelf { user_id }): Path<PathUserIdOrSelf>,
    Json(login): Json<ApiOAuth2Login>,
) -> Response {
    match service
        .create_link(&token.0, user_id.into(), login.into())
        .await
    {
        Ok(link) => Json(ApiOAuth2Link::from(link)).into_response(),
        Err(OAuth2CreateLinkError::InvalidProvider) => ProviderNotFoundError.into_response(),
        Err(OAuth2CreateLinkError::InvalidCode) => InvalidCodeError.into_response(),
        Err(OAuth2CreateLinkError::RemoteAlreadyLinked) => RemoteAlreadyLinkedError.into_response(),
        Err(OAuth2CreateLinkError::NotFound) => UserNotFoundError.into_response(),
        Err(OAuth2CreateLinkError::Auth(err)) => auth_error(err),
        Err(OAuth2CreateLinkError::Other(err)) => internal_server_error(err),
    }
}

fn create_link_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create a new OAuth2 link for the given user.")
        .add_response::<ApiOAuth2Link>(StatusCode::OK, "OAuth2 link has been created.")
        .add_error::<ProviderNotFoundError>()
        .add_error::<InvalidCodeError>()
        .add_error::<RemoteAlreadyLinkedError>()
        .add_error::<UserNotFoundError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Deserialize, JsonSchema)]
struct DeleteLinkPath {
    user_id: ApiUserIdOrSelf,
    link_id: OAuth2LinkId,
}

async fn delete_link(
    service: State<Arc<impl OAuth2FeatureService>>,
    token: ApiToken,
    Path(DeleteLinkPath { user_id, link_id }): Path<DeleteLinkPath>,
) -> Response {
    match service.delete_link(&token.0, user_id.into(), link_id).await {
        Ok(()) => Json(OkResponse).into_response(),
        Err(OAuth2DeleteLinkError::NotFound) => LinkNotFoundError.into_response(),
        Err(OAuth2DeleteLinkError::CannotRemoveLink) => {
            CannotDeleteLastLoginMethodError.into_response()
        }
        Err(OAuth2DeleteLinkError::Auth(err)) => auth_error(err),
        Err(OAuth2DeleteLinkError::Other(err)) => internal_server_error(err),
    }
}

fn delete_link_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Delete the given OAuth2 link.")
        .description(
            "Deleting the last link is only possible if the user has set a password for login.",
        )
        .add_response::<OkResponse>(StatusCode::OK, "OAuth2 link has been deleted.")
        .add_error::<LinkNotFoundError>()
        .add_error::<CannotDeleteLastLoginMethodError>()
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}

#[derive(Serialize, JsonSchema)]
struct CreateSessionLoginResponse {
    login: ApiLogin,
}

#[derive(Serialize, JsonSchema)]
struct CreateSessionRegistrationTokenResponse {
    register_token: OAuth2RegistrationToken,
}

async fn create_session(
    service: State<Arc<impl OAuth2FeatureService>>,
    user_agent: UserAgent,
    Json(login): Json<ApiOAuth2Login>,
) -> Response {
    match service
        .create_session(
            login.into(),
            user_agent.0.map(DeviceName::from_string_truncated),
        )
        .await
    {
        Ok(OAuth2CreateSessionResponse::Login(login)) => Json(CreateSessionLoginResponse {
            login: ApiLogin::from(*login),
        })
        .into_response(),
        Ok(OAuth2CreateSessionResponse::RegistrationToken(register_token)) => {
            Json(CreateSessionRegistrationTokenResponse { register_token }).into_response()
        }
        Err(OAuth2CreateSessionError::InvalidProvider) => ProviderNotFoundError.into_response(),
        Err(OAuth2CreateSessionError::InvalidCode) => InvalidCodeError.into_response(),
        Err(OAuth2CreateSessionError::UserDisabled) => UserDisabledError.into_response(),
        Err(OAuth2CreateSessionError::Other(err)) => internal_server_error(err),
    }
}

fn create_session_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Create a session via OAuth2")
        .description(
            "If the remote user is not yet linked to a local user account, a registration token \
             is returned instead.",
        )
        .add_response::<CreateSessionLoginResponse>(
            StatusCode::OK,
            "A new session has been created.",
        )
        .add_response::<CreateSessionRegistrationTokenResponse>(
            StatusCode::OK,
            "A registration token has been generated.",
        )
        .add_error::<ProviderNotFoundError>()
        .add_error::<InvalidCodeError>()
        .add_error::<UserDisabledError>()
        .with(internal_server_error_docs)
}

error_code! {
    /// The OAuth2 provider does not exist.
    ProviderNotFoundError(NOT_FOUND, "Provider not found");
    /// The authorization code is invalid.
    InvalidCodeError(UNAUTHORIZED, "Invalid code");
    /// The remote user has already been linked to another account.
    pub RemoteAlreadyLinkedError(CONFLICT, "Remote already linked");
    /// The OAuth2 link does not exist.
    LinkNotFoundError(NOT_FOUND, "Connection not found");
}
