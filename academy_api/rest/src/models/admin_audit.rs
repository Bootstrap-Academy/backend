use academy_models::{
    admin_audit::{
        AdminAuditLogEntry, AdminAuditLogEntryId, RequestId, RequestMethod, RequestPath,
    },
    user::UserId,
};
use schemars::JsonSchema;
use serde::Serialize;

use super::contract::ApiTimestamp;

/// One state changing request that was made with an administrator's access
/// token. Request bodies are never recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ApiAdminAuditLogEntry {
    pub id: AdminAuditLogEntryId,
    /// Time at which the request was answered
    pub at: ApiTimestamp,
    /// The administrator whose access token authenticated the request
    pub admin_user_id: UserId,
    /// HTTP method of the request
    pub method: RequestMethod,
    /// Path of the request, without the query string
    pub path: RequestPath,
    /// The user the request acted on, as far as the route identifies one
    pub target_user_id: Option<UserId>,
    /// HTTP status code of the response
    pub status: u16,
    /// Value of the `X-Request-Id` response header
    pub request_id: RequestId,
}

impl From<AdminAuditLogEntry> for ApiAdminAuditLogEntry {
    fn from(value: AdminAuditLogEntry) -> Self {
        Self {
            id: value.id,
            at: value.at.into(),
            admin_user_id: value.admin_user_id,
            method: value.method,
            path: value.path,
            target_user_id: value.target_user_id,
            status: value.status,
            request_id: value.request_id,
        }
    }
}
