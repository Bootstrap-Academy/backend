use academy_auth_contracts::MockAuthService;
use academy_core_admin_audit_contracts::{
    AdminAuditFeatureService, AdminAuditListError, AdminAuditListQuery, AdminAuditListResult,
};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::{
    admin_audit::AdminAuditLogFilter,
    auth::{AuthError, AuthenticateError, AuthorizeError},
    pagination::PaginationSlice,
};
use academy_persistence_contracts::{MockDatabase, admin_audit::MockAdminAuditRepository};
use academy_utils::assert_matches;

use super::{Sut, make_entry};
use crate::AdminAuditFeatureServiceImpl;

fn make_query() -> AdminAuditListQuery {
    AdminAuditListQuery {
        filter: AdminAuditLogFilter {
            admin_user_id: Some(ADMIN.user.id),
            target_user_id: None,
        },
        pagination: PaginationSlice {
            limit: 42.try_into().unwrap(),
            offset: 7,
        },
    }
}

#[tokio::test]
async fn ok() {
    // Arrange
    let query = make_query();
    let expected = AdminAuditListResult {
        total: 17,
        entries: vec![make_entry()],
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));
    let db = MockDatabase::build(false);
    let admin_audit_repo = MockAdminAuditRepository::new()
        .with_count(query.filter, expected.total)
        .with_list(query.filter, query.pagination, expected.entries.clone());

    let sut = AdminAuditFeatureServiceImpl {
        db,
        auth,
        admin_audit_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.list(&"token".into(), query).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = AdminAuditFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list(&"token".into(), make_query()).await;

    // Assert
    assert_matches!(
        result,
        Err(AdminAuditListError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn no_admin() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = AdminAuditFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list(&"token".into(), make_query()).await;

    // Assert
    assert_matches!(
        result,
        Err(AdminAuditListError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

/// An administrator whose session was not authenticated with a second factor
/// cannot read the audit log.
#[tokio::test]
async fn admin_without_mfa() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), FOO_1.clone())));

    let sut = AdminAuditFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list(&"token".into(), make_query()).await;

    // Assert
    assert_matches!(
        result,
        Err(AdminAuditListError::Auth(AuthError::Authorize(
            AuthorizeError::AdminMfa
        )))
    );
}
