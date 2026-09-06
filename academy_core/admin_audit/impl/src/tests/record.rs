use academy_auth_contracts::MockAuthService;
use academy_core_admin_audit_contracts::{AdminAuditFeatureService, AdminAuditRequest};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::admin_audit::AdminAuditLogEntry;
use academy_persistence_contracts::{MockDatabase, admin_audit::MockAdminAuditRepository};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};

use super::{Sut, make_entry, make_request, now};
use crate::AdminAuditFeatureServiceImpl;

fn sut(expected: AdminAuditLogEntry) -> Sut {
    AdminAuditFeatureServiceImpl {
        db: MockDatabase::build(true),
        auth: MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone()))),
        id: MockIdService::new().with_generate(expected.id),
        time: MockTimeService::new().with_now(now()),
        admin_audit_repo: MockAdminAuditRepository::new().with_create(expected),
    }
}

#[tokio::test]
async fn admin() {
    // Arrange
    let sut = sut(make_entry());

    // Act
    let result = sut.record(make_request()).await;

    // Assert
    assert!(result.unwrap());
}

/// A request that was rejected is recorded with the status code it was
/// answered with.
#[tokio::test]
async fn admin_rejected_request() {
    // Arrange
    let sut = sut(AdminAuditLogEntry {
        status: 403,
        ..make_entry()
    });

    // Act
    let result = sut
        .record(AdminAuditRequest {
            status: 403,
            ..make_request()
        })
        .await;

    // Assert
    assert!(result.unwrap());
}

/// Without a matched route there is no path parameter to read the affected
/// user from.
#[tokio::test]
async fn admin_without_route() {
    // Arrange
    let sut = sut(AdminAuditLogEntry {
        target_user_id: None,
        ..make_entry()
    });

    // Act
    let result = sut
        .record(AdminAuditRequest {
            route: None,
            ..make_request()
        })
        .await;

    // Assert
    assert!(result.unwrap());
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
    let result = sut.record(make_request()).await;

    // Assert
    assert!(!result.unwrap());
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
    let result = sut.record(make_request()).await;

    // Assert
    assert!(!result.unwrap());
}
