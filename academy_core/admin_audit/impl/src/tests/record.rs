use super::{Sut, make_entry, make_request, now};
use crate::AdminAuditFeatureServiceImpl;
use academy_auth_contracts::MockAuthService;
use academy_core_admin_audit_contracts::{
    AdminAuditActor, AdminAuditCapture, AdminAuditCredential, AdminAuditFeatureService,
    AdminAuditRequest,
};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::session::{Session, SessionOrigin};
use academy_persistence_contracts::{
    MockDatabase, admin_audit::MockAdminAuditRepository, session::MockSessionRepository,
};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};

#[tokio::test]
async fn captures_durable_actor_before_the_handler() {
    for (user, session, expected) in [
        (
            ADMIN.user.clone(),
            ADMIN_1.clone(),
            AdminAuditCapture::Recorded(AdminAuditActor {
                user_id: ADMIN.user.id,
                admin_user_id: Some(ADMIN.user.id),
                impersonated: false,
            }),
        ),
        (
            FOO.user.clone(),
            Session {
                origin: SessionOrigin::Impersonation {
                    admin: Some(ADMIN.user.id),
                },
                ..FOO_1.clone()
            },
            AdminAuditCapture::Recorded(AdminAuditActor {
                user_id: FOO.user.id,
                admin_user_id: Some(ADMIN.user.id),
                impersonated: true,
            }),
        ),
        (
            FOO.user.clone(),
            Session {
                origin: SessionOrigin::Impersonation { admin: None },
                ..FOO_1.clone()
            },
            AdminAuditCapture::Recorded(AdminAuditActor {
                user_id: FOO.user.id,
                admin_user_id: None,
                impersonated: true,
            }),
        ),
        (
            FOO.user.clone(),
            FOO_1.clone(),
            AdminAuditCapture::Unrecorded,
        ),
    ] {
        let mut auth =
            MockAuthService::new().with_authenticate(Some((user.clone(), session.clone())));
        let mut session_repo =
            MockSessionRepository::new().with_get(session.id, Some(session.clone()));
        if matches!(expected, AdminAuditCapture::Recorded(_)) {
            auth = auth.with_authenticate_in_transaction(Some((user, session.clone())));
            session_repo = session_repo.with_get(session.id, Some(session));
        }
        let sut = AdminAuditFeatureServiceImpl {
            db: MockDatabase::build_expect_rollback(),
            auth,
            session_repo,
            ..Sut::default()
        };
        assert_eq!(
            sut.capture(AdminAuditCredential::Access(&"token".into()))
                .await
                .unwrap(),
            expected
        );
    }
}

#[tokio::test]
async fn invalid_credential_has_no_actor() {
    let sut = AdminAuditFeatureServiceImpl {
        db: MockDatabase::new(),
        auth: MockAuthService::new().with_authenticate(None),
        ..Sut::default()
    };
    assert_eq!(
        sut.capture(AdminAuditCredential::Access(&"token".into()))
            .await
            .unwrap(),
        AdminAuditCapture::InvalidCredential
    );
}

// Recording uses only captured facts, with no second authentication or session
// lookup that could lose an actor through rotation, revocation or account deletion.
#[tokio::test]
async fn records_captured_actor_and_final_status_without_live_credentials() {
    for (actor, route, status, target) in [
        (
            make_request().actor,
            make_request().route,
            200,
            Some(ADMIN.user.id),
        ),
        (make_request().actor, None, 403, None),
        (
            AdminAuditActor {
                user_id: FOO.user.id,
                admin_user_id: Some(ADMIN.user.id),
                impersonated: true,
            },
            None,
            401,
            Some(FOO.user.id),
        ),
        (
            AdminAuditActor {
                user_id: FOO.user.id,
                admin_user_id: None,
                impersonated: true,
            },
            None,
            200,
            Some(FOO.user.id),
        ),
    ] {
        let request = AdminAuditRequest {
            actor,
            route,
            status,
            ..make_request()
        };
        let expected = academy_models::admin_audit::AdminAuditLogEntry {
            admin_user_id: actor.admin_user_id,
            target_user_id: target,
            status,
            ..make_entry()
        };
        let sut = AdminAuditFeatureServiceImpl {
            db: MockDatabase::build(true),
            id: MockIdService::new().with_generate(expected.id),
            time: MockTimeService::new().with_now(now()),
            admin_audit_repo: MockAdminAuditRepository::new().with_create(expected),
            ..Sut::default()
        };
        assert!(sut.record(request).await.unwrap());
    }
}
