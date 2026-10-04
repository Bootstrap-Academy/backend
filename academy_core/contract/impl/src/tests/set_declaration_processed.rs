use academy_auth_contracts::MockAuthService;
use academy_core_contract_contracts::{
    ContractDeclarationProcessingUpdate, ContractFeatureService, ContractSetProcessedError,
};
use academy_demo::{
    UUID1,
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    contract::{
        ContractCancellationType, ContractDeclaration, ContractDeclarationKind, ContractKind,
    },
};
use academy_persistence_contracts::{MockDatabase, contract::MockContractRepository};
use academy_shared_contracts::time::MockTimeService;
use academy_utils::assert_matches;
use chrono::{DateTime, TimeZone, Utc};

use crate::{
    ContractFeatureServiceImpl,
    tests::{Sut, declarant_email, declarant_name},
};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 8, 9, 30, 0).unwrap()
}

fn confirmed_end() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 30, 21, 59, 59).unwrap()
}

/// An extraordinary cancellation, which has no end date until somebody has
/// looked at it.
fn make_declaration() -> ContractDeclaration {
    ContractDeclaration {
        delivery: Vec::new(),
        operational_evidence: None,
        id: UUID1.into(),
        kind: ContractDeclarationKind::Cancellation,
        received_at: Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap(),
        name: declarant_name(),
        email: declarant_email(),
        user_id: Some(FOO.user.id),
        contract: ContractKind::Premium,
        contract_designation: None,
        cancellation_type: Some(ContractCancellationType::Extraordinary),
        details: "Leistung nicht verfügbar".try_into().unwrap(),
        requested_end: None,
        effective_end: None,
        processed_at: None,
        processing_note: None,
    }
}

fn make_update() -> ContractDeclarationProcessingUpdate {
    ContractDeclarationProcessingUpdate {
        identity_verified: true,
        action: Default::default(),
        verified_user_id: None,
        renewal_agreement_id: None,
        effective_end: Some(confirmed_end()),
        note: Some("Kündigung anerkannt, Ende bestätigt".try_into().unwrap()),
    }
}

#[tokio::test]
async fn ok() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())))
        .with_authenticate_in_transaction(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);
    let time = MockTimeService::new().with_now(now());

    let expected = ContractDeclaration {
        delivery: Vec::new(),
        operational_evidence: None,
        effective_end: Some(confirmed_end()),
        processed_at: Some(now()),
        processing_note: Some("Kündigung anerkannt, Ende bestätigt".try_into().unwrap()),
        ..make_declaration()
    };

    let mut contract_repo = MockContractRepository::new()
        .with_get(make_declaration().id, Some(make_declaration()))
        .with_set_processed(
            make_declaration().id,
            now(),
            expected.effective_end,
            expected.processing_note.clone(),
            Some(expected.clone()),
        );

    contract_repo
        .expect_lock_request()
        .once()
        .return_once(|_, _| Box::pin(async { Ok(()) }));
    contract_repo
        .expect_lock_session_processing()
        .once()
        .return_once(|_, _, _, _| Box::pin(async { Ok(()) }));
    contract_repo
        .expect_lock_processing()
        .once()
        .return_once(|_, _| Box::pin(async { Ok(()) }));
    let sut = ContractFeatureServiceImpl {
        auth,
        db,
        time,
        contract_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .set_declaration_processed(&"token".into(), make_declaration().id, make_update())
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

/// A declaration whose end date the backend already determined keeps it when
/// the administrator only adds a note.
#[tokio::test]
async fn completion_requires_identity_and_resolution_evidence() {
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));
    let sut = Sut {
        auth,
        ..Sut::default()
    };
    assert_matches!(
        sut.set_declaration_processed(&"token".into(), make_declaration().id, Default::default())
            .await,
        Err(ContractSetProcessedError::Invalid)
    );
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth = MockAuthService::new()
        .with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())))
        .with_authenticate_in_transaction(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);
    let time = MockTimeService::new();

    let mut contract_repo = MockContractRepository::new().with_get(make_declaration().id, None);

    contract_repo
        .expect_lock_request()
        .once()
        .return_once(|_, _| Box::pin(async { Ok(()) }));
    contract_repo
        .expect_lock_session_processing()
        .once()
        .return_once(|_, _, _, _| Box::pin(async { Ok(()) }));
    contract_repo
        .expect_lock_processing()
        .once()
        .return_once(|_, _| Box::pin(async { Ok(()) }));
    let sut = ContractFeatureServiceImpl {
        auth,
        db,
        time,
        contract_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .set_declaration_processed(&"token".into(), make_declaration().id, make_update())
        .await;

    // Assert
    assert_matches!(result, Err(ContractSetProcessedError::NotFound));
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = ContractFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .set_declaration_processed(&"token".into(), make_declaration().id, make_update())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(ContractSetProcessedError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = ContractFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .set_declaration_processed(&"token".into(), make_declaration().id, make_update())
        .await;

    // Assert
    assert_matches!(
        result,
        Err(ContractSetProcessedError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn processing_rechecks_session_and_admin_mfa_after_request_and_account_waits() {
    use academy_core_contract_contracts::ContractProcessingAction;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    for schedule in [false, true] {
        for denial in ["revoked", "demoted", "mfa"] {
            let step = Arc::new(AtomicUsize::new(0));
            let mut auth = MockAuthService::new()
                .with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));
            let checked = Arc::clone(&step);
            auth.expect_authenticate_in_transaction()
                .once()
                .return_once(move |_, token| {
                    assert_eq!(token.as_str(), "token");
                    assert_eq!(checked.load(Ordering::SeqCst), 2);
                    let result = if denial == "revoked" {
                        Err(AuthenticateError::InvalidToken)
                    } else {
                        Ok(academy_auth_contracts::Authentication {
                            user_id: ADMIN.user.id,
                            session_id: ADMIN_1.id,
                            refresh_token_hash: (*academy_demo::SHA256HASH1).into(),
                            admin: denial != "demoted",
                            email_verified: true,
                            mfa_verified: denial != "mfa",
                        })
                    };
                    Box::pin(async move { result })
                });
            let mut repo = MockContractRepository::new();
            let request = Arc::clone(&step);
            repo.expect_lock_request().once().return_once(move |_, id| {
                assert_eq!(id, make_declaration().id);
                request.store(1, Ordering::SeqCst);
                Box::pin(async { Ok(()) })
            });
            let accounts = Arc::clone(&step);
            repo.expect_lock_session_processing().once().return_once(
                move |_, id, actor, target| {
                    assert_eq!(accounts.load(Ordering::SeqCst), 1);
                    assert_eq!(id, make_declaration().id);
                    assert_eq!(actor, ADMIN.user.id);
                    assert_eq!(target, schedule.then_some(FOO.user.id));
                    accounts.store(2, Ordering::SeqCst);
                    Box::pin(async { Ok(()) })
                },
            );
            repo.expect_lock_processing().never();
            repo.expect_set_processed().never();
            repo.expect_schedule_cancellation().never();
            let sut = Sut {
                auth,
                db: MockDatabase::build(false),
                contract_repo: repo,
                ..Sut::default()
            };
            let mut update = make_update();
            if schedule {
                update.action = ContractProcessingAction::SchedulePremiumCancellation;
                update.verified_user_id = Some(FOO.user.id);
            }
            let error = sut
                .set_declaration_processed(&"token".into(), make_declaration().id, update)
                .await
                .unwrap_err();
            match denial {
                "revoked" => assert_matches!(
                    error,
                    ContractSetProcessedError::Auth(AuthError::Authenticate(
                        AuthenticateError::InvalidToken
                    ))
                ),
                "demoted" => assert_matches!(
                    error,
                    ContractSetProcessedError::Auth(AuthError::Authorize(AuthorizeError::Admin))
                ),
                _ => assert_matches!(
                    error,
                    ContractSetProcessedError::Auth(AuthError::Authorize(AuthorizeError::AdminMfa))
                ),
            }
        }
    }
}
