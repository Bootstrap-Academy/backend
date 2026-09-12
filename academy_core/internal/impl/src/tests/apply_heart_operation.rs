use academy_auth_contracts::internal::MockAuthInternalService;
use academy_core_heart_contracts::heart::MockHeartService;
use academy_core_internal_contracts::{InternalHeartOperationError, InternalService};
use academy_core_premium_contracts::premium::MockPremiumService;
use academy_demo::{UUID1, user::FOO};
use academy_models::{
    heart::{
        HeartOperation, HeartOperationClaim, HeartOperationOutcome, HeartOperationReceipt, Hearts,
    },
    premium::Premium,
};
use academy_persistence_contracts::{MockDatabase, MockTransaction, heart::MockHeartRepository};

use super::Sut;

fn operation() -> HeartOperation {
    HeartOperation {
        id: UUID1.into(),
        user_id: FOO.user.id,
        half_hearts: 2,
        reason: "incorrect_challenge_attempt".into(),
    }
}

fn claimed(claim: HeartOperationClaim) -> MockHeartRepository<MockTransaction> {
    let mut repo = MockHeartRepository::new();
    repo.expect_claim_operation()
        .once()
        .withf(|_, value| value.id == UUID1.into())
        .return_once(move |_, _| Box::pin(async move { Ok(claim) }));
    repo
}

// This is a whole heart (two wire units), including the one-half-heart race
// boundary. Premium and insufficient receipts are terminal successes too.
#[tokio::test]
async fn charges_one_whole_heart_or_records_a_terminal_zero() {
    for (balance, is_premium, outcome, charged) in [
        (10, false, HeartOperationOutcome::Charged, 2),
        (2, false, HeartOperationOutcome::Charged, 2),
        (1, false, HeartOperationOutcome::Insufficient, 0),
        (0, false, HeartOperationOutcome::Insufficient, 0),
        (10, true, HeartOperationOutcome::Premium, 0),
        (0, true, HeartOperationOutcome::Premium, 0),
    ] {
        let current = Hearts {
            hearts: balance,
            last_refill: FOO.user.created_at,
        };
        let expected = HeartOperationReceipt {
            operation_id: UUID1.into(),
            user_id: FOO.user.id,
            charged_half_hearts: charged,
            hearts: balance - charged,
            outcome,
        };
        let mut repo = claimed(HeartOperationClaim::New);
        repo.expect_lock_user()
            .once()
            .withf(|_, id| *id == FOO.user.id)
            .return_once(|_, _| Box::pin(async { Ok(true) }));
        if charged != 0 {
            repo.expect_set()
                .once()
                .withf(move |_, id, hearts| {
                    *id == FOO.user.id
                        && *hearts
                            == Hearts {
                                hearts: balance - 2,
                                ..current
                            }
                })
                .return_once(|_, _, _| Box::pin(async { Ok(()) }));
        }
        repo.expect_complete_operation()
            .once()
            .withf(move |_, request, receipt| *request == operation() && *receipt == expected)
            .return_once(|_, _, _| Box::pin(async { Ok(()) }));
        let sut = Sut {
            auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
            db: MockDatabase::build(true),
            heart_repo: repo,
            heart: MockHeartService::new().with_get(FOO.user.id, current),
            premium: MockPremiumService::new().with_get_active(
                FOO.user.id,
                is_premium.then_some(Premium {
                    id: UUID1.into(),
                    user_id: FOO.user.id,
                    since: FOO.user.created_at,
                    until: FOO.user.last_login.unwrap(),
                }),
            ),
            ..Sut::default()
        };
        assert_eq!(
            sut.apply_heart_operation(&"internal token".into(), operation())
                .await
                .unwrap(),
            expected
        );
    }
}

#[tokio::test]
async fn replay_never_rechecks_balance_premium_or_user_and_cannot_become_a_debt() {
    for outcome in [
        HeartOperationOutcome::Charged,
        HeartOperationOutcome::Premium,
        HeartOperationOutcome::Insufficient,
    ] {
        let expected = HeartOperationReceipt {
            operation_id: UUID1.into(),
            user_id: FOO.user.id,
            charged_half_hearts: if outcome == HeartOperationOutcome::Charged {
                2
            } else {
                0
            },
            hearts: 0,
            outcome,
        };
        let sut = Sut {
            auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
            db: MockDatabase::build(false),
            heart_repo: claimed(HeartOperationClaim::Completed(expected)),
            ..Sut::default()
        };
        assert_eq!(
            sut.apply_heart_operation(&"internal token".into(), operation())
                .await
                .unwrap(),
            expected
        );
    }
}

#[tokio::test]
async fn conflicting_reuse_is_rejected_before_revalidation_or_debit() {
    let sut = Sut {
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
        db: MockDatabase::build(false),
        heart_repo: claimed(HeartOperationClaim::Conflict),
        ..Sut::default()
    };
    let request = HeartOperation {
        half_hearts: 1,
        ..operation()
    };
    assert!(matches!(
        sut.apply_heart_operation(&"internal token".into(), request)
            .await,
        Err(InternalHeartOperationError::OperationConflict)
    ));
}

#[tokio::test]
async fn invalid_new_requests_never_lock_or_change_the_user() {
    for request in [
        HeartOperation {
            half_hearts: 1,
            ..operation()
        },
        HeartOperation {
            half_hearts: 0,
            ..operation()
        },
        HeartOperation {
            reason: "correct_challenge_attempt".into(),
            ..operation()
        },
    ] {
        let sut = Sut {
            auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
            db: MockDatabase::build(false),
            heart_repo: claimed(HeartOperationClaim::New),
            ..Sut::default()
        };
        assert!(matches!(
            sut.apply_heart_operation(&"internal token".into(), request)
                .await,
            Err(InternalHeartOperationError::InvalidRequest)
        ));
    }
}

#[tokio::test]
async fn deleted_user_cannot_be_recreated_by_a_late_attempt() {
    let mut repo = claimed(HeartOperationClaim::New);
    repo.expect_lock_user()
        .once()
        .return_once(|_, _| Box::pin(async { Ok(false) }));
    let sut = Sut {
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
        db: MockDatabase::build(false),
        heart_repo: repo,
        ..Sut::default()
    };
    assert!(matches!(
        sut.apply_heart_operation(&"internal token".into(), operation())
            .await,
        Err(InternalHeartOperationError::UserNotFound)
    ));
}

#[tokio::test]
async fn auth_is_required_even_for_a_replay() {
    let sut = Sut {
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", false),
        ..Sut::default()
    };
    assert!(matches!(
        sut.apply_heart_operation(&"internal token".into(), operation())
            .await,
        Err(InternalHeartOperationError::Auth(_))
    ));
}

#[tokio::test]
async fn failed_receipt_does_not_commit_the_debit() {
    let mut repo = claimed(HeartOperationClaim::New);
    repo.expect_lock_user()
        .once()
        .return_once(|_, _| Box::pin(async { Ok(true) }));
    repo.expect_set()
        .once()
        .return_once(|_, _, _| Box::pin(async { Ok(()) }));
    repo.expect_complete_operation()
        .once()
        .return_once(|_, _, _| Box::pin(async { Err(anyhow::anyhow!("fixture receipt failure")) }));
    let sut = Sut {
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
        db: MockDatabase::build(false),
        heart_repo: repo,
        heart: MockHeartService::new().with_get(
            FOO.user.id,
            Hearts {
                hearts: 2,
                last_refill: FOO.user.created_at,
            },
        ),
        premium: MockPremiumService::new().with_get_active(FOO.user.id, None),
        ..Sut::default()
    };
    assert!(matches!(
        sut.apply_heart_operation(&"internal token".into(), operation())
            .await,
        Err(InternalHeartOperationError::Other(_))
    ));
}
