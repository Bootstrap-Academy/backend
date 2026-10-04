use academy_auth_contracts::MockAuthService;
use academy_core_admin_audit_contracts::{AdminAuditFeatureService, AdminAuditRequest};
use academy_demo::{
    UUID1,
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::{
    admin_audit::AdminAuditLogEntry,
    session::{Session, SessionOrigin},
};
use academy_persistence_contracts::{
    MockDatabase, admin_audit::MockAdminAuditRepository, session::MockSessionRepository,
};
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
        session_repo: MockSessionRepository::new().with_get(ADMIN_1.id, Some(ADMIN_1.clone())),
    }
}

/// A session the administrator opened in FOO's account.
fn impersonation(admin: Option<academy_models::user::UserId>) -> Session {
    Session {
        id: UUID1.into(),
        device_name: None,
        mfa_verified: false,
        origin: SessionOrigin::Impersonation { admin },
        ..FOO_1.clone()
    }
}

fn publication_request() -> AdminAuditRequest {
    AdminAuditRequest {
        method: "PUT".try_into().unwrap(),
        path: "/auth/users/me/publication".try_into().unwrap(),
        route: Some("/auth/users/me/publication".try_into().unwrap()),
        status: 403,
        ..make_request()
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

/// Requests in a session an administrator opened in someone else's account
/// carry that account's token. They are recorded for the administrator and act
/// on that account, including a refused attempt to share its profile.
#[tokio::test]
async fn impersonation_session_recorded_for_admin() {
    // Arrange
    let session = impersonation(Some(ADMIN.user.id));
    let request = publication_request();
    let expected = AdminAuditLogEntry {
        id: UUID1.into(),
        at: now(),
        admin_user_id: ADMIN.user.id,
        method: request.method.clone(),
        path: request.path.clone(),
        target_user_id: Some(FOO.user.id),
        status: 403,
        request_id: request.request_id.clone(),
    };

    let sut = AdminAuditFeatureServiceImpl {
        db: MockDatabase::build(true),
        auth: MockAuthService::new().with_authenticate(Some((FOO.user.clone(), session.clone()))),
        id: MockIdService::new().with_generate(expected.id),
        time: MockTimeService::new().with_now(now()),
        admin_audit_repo: MockAdminAuditRepository::new().with_create(expected),
        session_repo: MockSessionRepository::new().with_get(session.id, Some(session)),
    };

    // Act
    let result = sut.record(request).await;

    // Assert
    assert!(result.unwrap());
}

/// The CLI signs in without an administrator account, so there is nobody to
/// attribute its requests to.
#[tokio::test]
async fn cli_impersonation_session() {
    // Arrange
    let session = impersonation(None);

    let sut = AdminAuditFeatureServiceImpl {
        db: MockDatabase::build(false),
        auth: MockAuthService::new().with_authenticate(Some((FOO.user.clone(), session.clone()))),
        session_repo: MockSessionRepository::new().with_get(session.id, Some(session)),
        ..Sut::default()
    };

    // Act
    let result = sut.record(publication_request()).await;

    // Assert
    assert!(!result.unwrap());
}

#[tokio::test]
async fn no_admin() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = AdminAuditFeatureServiceImpl {
        auth,
        db: MockDatabase::build(false),
        session_repo: MockSessionRepository::new().with_get(FOO_1.id, Some(FOO_1.clone())),
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
