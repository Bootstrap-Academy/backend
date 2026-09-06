use std::future::Future;

use academy_models::{
    admin_audit::{AdminAuditLogEntry, AdminAuditLogFilter, RequestId, RequestMethod, RequestPath},
    auth::{AccessToken, AuthError},
    pagination::PaginationSlice,
    user::UserId,
};
use thiserror::Error;

pub trait AdminAuditFeatureService: Send + Sync + 'static {
    /// Record a request in the administrative audit log.
    ///
    /// Requests that were not authenticated with an administrator's access
    /// token are ignored. Returns whether an entry has been recorded.
    fn record(
        &self,
        request: AdminAuditRequest,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Return all audit log entries matching the given query.
    ///
    /// Requires admin privileges.
    fn list(
        &self,
        token: &AccessToken,
        query: AdminAuditListQuery,
    ) -> impl Future<Output = Result<AdminAuditListResult, AdminAuditListError>> + Send;
}

/// A request that has been answered and may need to be recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminAuditRequest {
    /// The access token the request was authenticated with
    pub token: AccessToken,
    /// HTTP method of the request
    pub method: RequestMethod,
    /// Path of the request, without the query string
    pub path: RequestPath,
    /// The route the request was matched against (e.g.
    /// `/auth/users/{user_id}`), used to identify the affected user
    pub route: Option<RequestPath>,
    /// HTTP status code of the response
    pub status: u16,
    /// Value of the `X-Request-Id` response header
    pub request_id: RequestId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AdminAuditListQuery {
    pub filter: AdminAuditLogFilter,
    pub pagination: PaginationSlice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminAuditListResult {
    pub total: u64,
    pub entries: Vec<AdminAuditLogEntry>,
}

#[derive(Debug, Error)]
pub enum AdminAuditListError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Return the user the given request acted on, as far as the matched route
/// identifies one.
///
/// Every route that operates on a specific user takes its id in a `{user_id}`
/// path parameter, which may also be the `me`/`self` alias for the
/// authenticated user.
pub fn target_user_id(path: &str, route: Option<&str>, actor: UserId) -> Option<UserId> {
    let route = route?;

    route
        .split('/')
        .zip(path.split('/'))
        .find(|(template, _)| *template == "{user_id}")
        .and_then(|(_, segment)| match segment.to_lowercase().as_str() {
            "me" | "self" => Some(actor),
            _ => segment.parse::<uuid::Uuid>().ok().map(UserId::new),
        })
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use uuid::uuid;

    use super::*;

    static ACTOR: LazyLock<UserId> =
        LazyLock::new(|| uuid!("e3f8a50a-a5a3-444a-9026-77336f716d03").into());
    static TARGET: LazyLock<UserId> =
        LazyLock::new(|| uuid!("7e3f8a50-a5a3-444a-9026-77336f716d04").into());

    #[test]
    fn target_user_id_from_path_parameter() {
        assert_eq!(
            target_user_id(
                &format!("/auth/users/{}", **TARGET),
                Some("/auth/users/{user_id}"),
                *ACTOR
            ),
            Some(*TARGET)
        );
    }

    #[test]
    fn target_user_id_from_alias() {
        for alias in ["me", "self", "ME"] {
            assert_eq!(
                target_user_id(
                    &format!("/auth/users/{alias}/mfa"),
                    Some("/auth/users/{user_id}/mfa"),
                    *ACTOR
                ),
                Some(*ACTOR)
            );
        }
    }

    /// The first path parameter of a route is not necessarily a user id.
    #[test]
    fn target_user_id_ignores_other_parameters() {
        assert_eq!(
            target_user_id(
                &format!("/shop/coins/paypal/orders/{}/capture", **TARGET),
                Some("/shop/coins/paypal/orders/{order_id}/capture"),
                *ACTOR
            ),
            None
        );
    }

    #[test]
    fn target_user_id_picks_the_user_id_parameter() {
        assert_eq!(
            target_user_id(
                &format!("/auth/sessions/{}/{}", **TARGET, **ACTOR),
                Some("/auth/sessions/{user_id}/{session_id}"),
                *ACTOR
            ),
            Some(*TARGET)
        );
    }

    #[test]
    fn target_user_id_without_route() {
        assert_eq!(
            target_user_id(&format!("/auth/users/{}", **TARGET), None, *ACTOR),
            None
        );
    }
}
