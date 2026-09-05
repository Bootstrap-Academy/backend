use std::sync::Arc;

use academy_core_admin_audit_contracts::{
    AdminAuditFeatureService, AdminAuditListError, AdminAuditListQuery, AdminAuditListResult,
};
use academy_models::{admin_audit::AdminAuditLogFilter, user::UserId};
use aide::{
    axum::{ApiRouter, routing},
    transform::TransformOperation,
};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    docs::TransformOperationExt,
    errors::{auth_error, auth_error_docs, internal_server_error, internal_server_error_docs},
    extractors::auth::ApiToken,
    models::{ApiPaginationSlice, admin_audit::ApiAdminAuditLogEntry},
};

pub const TAG: &str = "Admin Audit Log";

pub fn router(service: Arc<impl AdminAuditFeatureService>) -> ApiRouter<()> {
    ApiRouter::new()
        .api_route("/admin/audit-log", routing::get_with(list, list_docs))
        .with_state(service)
        .with_path_items(|op| op.tag(TAG))
}

#[derive(Deserialize, JsonSchema)]
struct ListFilter {
    /// Only return entries recorded for this administrator
    admin_user_id: Option<UserId>,
    /// Only return entries whose request acted on this user
    target_user_id: Option<UserId>,
}

#[derive(Serialize, JsonSchema)]
struct ListResult {
    /// The total number of entries matching the given query
    total: u64,
    /// The paginated list of entries matching the given query
    entries: Vec<ApiAdminAuditLogEntry>,
}

async fn list(
    service: State<Arc<impl AdminAuditFeatureService>>,
    token: ApiToken,
    Query(pagination): Query<ApiPaginationSlice>,
    Query(ListFilter {
        admin_user_id,
        target_user_id,
    }): Query<ListFilter>,
) -> Response {
    match service
        .list(
            &token.0,
            AdminAuditListQuery {
                filter: AdminAuditLogFilter {
                    admin_user_id,
                    target_user_id,
                },
                pagination: pagination.into(),
            },
        )
        .await
    {
        Ok(AdminAuditListResult { total, entries }) => Json(ListResult {
            total,
            entries: entries.into_iter().map(Into::into).collect(),
        })
        .into_response(),
        Err(AdminAuditListError::Auth(err)) => auth_error(err),
        Err(AdminAuditListError::Other(err)) => internal_server_error(err),
    }
}

fn list_docs(op: TransformOperation) -> TransformOperation {
    op.summary("Return the administrative audit log.")
        .description(
            "Every request that changes state and is authenticated with an administrator's access \
             token is recorded here, most recent first. Request bodies are never \
             recorded.\n\nRequires admin privileges.",
        )
        .add_response::<ListResult>(StatusCode::OK, None)
        .with(auth_error_docs)
        .with(internal_server_error_docs)
}
