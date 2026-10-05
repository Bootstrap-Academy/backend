use academy_auth_contracts::MockAuthService;
use academy_core_admin_audit_contracts::{AdminAuditActor, AdminAuditRequest};
use academy_demo::{UUID1, user::ADMIN};
use academy_models::admin_audit::AdminAuditLogEntry;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, admin_audit::MockAdminAuditRepository,
    session::MockSessionRepository, user::MockUserRepository,
};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
use chrono::{DateTime, TimeZone, Utc};

use crate::AdminAuditFeatureServiceImpl;

mod list;
mod record;

type Sut = AdminAuditFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockIdService,
    MockTimeService,
    MockAdminAuditRepository<MockTransaction>,
    MockSessionRepository<MockTransaction>,
    MockUserRepository<MockTransaction>,
>;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 3, 19, 30, 0).unwrap()
}

fn make_request() -> AdminAuditRequest {
    AdminAuditRequest {
        actor: AdminAuditActor {
            user_id: ADMIN.user.id,
            admin_user_id: Some(ADMIN.user.id),
            impersonated: false,
        },
        method: "PATCH".try_into().unwrap(),
        path: format!("/auth/users/{}", *ADMIN.user.id)
            .try_into()
            .unwrap(),
        route: Some("/auth/users/{user_id}".try_into().unwrap()),
        status: 200,
        request_id: "9EmXjWNfd0GcVXyIkbXQ2g".try_into().unwrap(),
    }
}

fn make_entry() -> AdminAuditLogEntry {
    let request = make_request();
    AdminAuditLogEntry {
        id: UUID1.into(),
        at: now(),
        admin_user_id: Some(ADMIN.user.id),
        method: request.method,
        path: request.path,
        target_user_id: Some(ADMIN.user.id),
        status: request.status,
        request_id: request.request_id,
    }
}
