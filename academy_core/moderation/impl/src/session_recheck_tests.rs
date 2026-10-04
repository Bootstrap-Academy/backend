use super::*;
use academy_models::auth::{AuthenticateError, AuthorizeError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn expect_write_lock(s: &mut Sut, commercial: bool, operation: &'static str) -> Arc<AtomicBool> {
    s.repo = MockModerationRepository::new();
    let locked = Arc::new(AtomicBool::new(false));
    let completed = Arc::clone(&locked);
    s.repo
        .expect_lock_session_write()
        .once()
        .return_once(move |_, actor, lane, op, _| {
            assert_eq!(actor, FOO.user.id);
            assert_eq!(lane, commercial);
            assert_eq!(op, operation);
            completed.store(true, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        });
    locked
}

fn deny_after_lock(s: &mut Sut, locked: Arc<AtomicBool>, denial: &'static str) {
    s.auth
        .expect_authenticate_in_transaction()
        .once()
        .return_once(move |_, token| {
            assert!(locked.load(Ordering::SeqCst));
            assert_eq!(token.as_str(), "captured-staff");
            let mut current = staff();
            let result = match denial {
                "revoked" => Err(AuthenticateError::InvalidToken),
                "demoted" => {
                    current.admin = false;
                    Ok(current)
                }
                _ => {
                    current.mfa_verified = false;
                    Ok(current)
                }
            };
            Box::pin(async move { result })
        });
}

#[tokio::test]
async fn every_staff_mutation_rechecks_session_admin_and_mfa_after_lock_waits() {
    let backend = [
        "open",
        "decide",
        "escalate",
        "handling",
        "delivery_contact",
        "retention",
    ];
    let commercial = [
        "hold_review",
        "determine",
        "review",
        "minimize_contact",
        "release_document",
        "release_record",
        "statement_review",
        "archive_review",
    ];
    for (lane, operations) in [(false, backend.as_slice()), (true, commercial.as_slice())] {
        for &operation in operations {
            for denial in ["revoked", "demoted", "mfa"] {
                let mut s = sut(&[false]);
                s.auth = MockAuthService::new();
                staff_auth(&mut s, Ok(staff()));
                let locked = expect_write_lock(&mut s, lane, operation);
                deny_after_lock(&mut s, locked, denial);
                s.repo.expect_operation().never();
                s.repo.expect_commercial_operation().never();
                let body = if operation == "hold_review" {
                    hold_public_body()
                } else {
                    json!({"command_id":UUID1,"case_id":UUID1,"id":UUID1,"request_key":UUID1,"target_id":original()})
                };
                let error = if lane {
                    s.commercial_admin(&"captured-staff".into(), operation, body)
                        .await
                } else {
                    s.admin(&"captured-staff".into(), operation, body).await
                }
                .unwrap_err();
                match denial {
                    "revoked" => assert!(matches!(
                        error.downcast_ref::<AuthenticateError>(),
                        Some(AuthenticateError::InvalidToken)
                    )),
                    "demoted" => assert!(matches!(
                        error.downcast_ref::<AuthorizeError>(),
                        Some(AuthorizeError::Admin)
                    )),
                    _ => assert!(matches!(
                        error.downcast_ref::<AuthorizeError>(),
                        Some(AuthorizeError::AdminMfa)
                    )),
                }
            }
        }
    }
}

#[tokio::test]
async fn backend_staff_mutations_keep_exact_body_and_commit_with_current_session() {
    for operation in [
        "open",
        "decide",
        "escalate",
        "handling",
        "delivery_contact",
        "retention",
    ] {
        let mut s = sut(&[true]);
        s.auth = MockAuthService::new();
        staff_auth(&mut s, Ok(staff()));
        let locked = expect_write_lock(&mut s, false, operation);
        s.auth
            .expect_authenticate_in_transaction()
            .once()
            .return_once(move |_, _| {
                assert!(locked.load(Ordering::SeqCst));
                Box::pin(async { Ok(staff()) })
            });
        let body = json!({"case_id":UUID1,"text":" original exact request "});
        let exact = body.clone();
        s.repo
            .expect_operation()
            .once()
            .return_once(move |_, op, actor, body| {
                assert_eq!(op, operation);
                assert_eq!(actor, Some(FOO.user.id));
                assert_eq!(body, &exact);
                Box::pin(async { Ok(json!({"committed":true})) })
            });
        assert_eq!(
            s.admin(&"captured-staff".into(), operation, body)
                .await
                .unwrap(),
            json!({"committed":true})
        );
    }
}

#[tokio::test]
async fn ordinary_opened_and_complaint_recheck_without_changing_selected_capability_authority() {
    for operation in ["opened", "complain"] {
        for revoked in [false, true] {
            let commits = if operation == "opened" {
                vec![true, !revoked]
            } else {
                vec![!revoked]
            };
            let mut s = sut(&commits);
            s.auth = MockAuthService::new();
            let locked = expect_write_lock(&mut s, false, operation);
            let credentials = ordinary(&mut s).recipient;
            s.auth
                .expect_authenticate_in_transaction()
                .once()
                .return_once(move |_, token| {
                    assert_eq!(token.as_str(), "ordinary");
                    assert!(locked.load(Ordering::SeqCst));
                    Box::pin(async move {
                        if revoked {
                            Err(AuthenticateError::InvalidToken)
                        } else {
                            Ok(staff())
                        }
                    })
                });
            if operation == "opened" {
                s.repo.expect_operation().once().return_once(|_, op, _, _| {
                    assert_eq!(op, "inbox");
                    Box::pin(async { Ok(json!([{"id":UUID1}])) })
                });
                s.services
                    .expect_moderation()
                    .once()
                    .return_once(|op, _, _| {
                        assert_eq!(op, "inbox");
                        Box::pin(async { Ok(json!([])) })
                    });
            }
            let body = json!({"id":UUID1,"decision_id":UUID1,"text":"exact complaint"});
            if !revoked {
                let exact = body.clone();
                s.repo
                    .expect_operation()
                    .once()
                    .return_once(move |_, op, actor, body| {
                        assert_eq!(op, operation);
                        assert_eq!(actor, Some(FOO.user.id));
                        assert_eq!(body, &exact);
                        Box::pin(async { Ok(json!(true)) })
                    });
            }
            let result = if operation == "opened" {
                s.opened(credentials, "backend", body).await
            } else {
                s.complain(credentials, "backend", body).await
            };
            if revoked {
                assert!(matches!(
                    result.unwrap_err().downcast_ref::<AuthenticateError>(),
                    Some(AuthenticateError::InvalidToken)
                ));
            } else {
                assert_eq!(result.unwrap(), json!(true));
            }
        }
    }
}

#[tokio::test]
async fn ordinary_commercial_intake_rechecks_and_claim_intake_preserves_its_own_authority() {
    for revoked in [false, true] {
        let mut s = sut(&[!revoked]);
        s.auth = MockAuthService::new();
        let locked = expect_write_lock(&mut s, true, "open");
        let credentials = ordinary(&mut s);
        s.auth
            .expect_authenticate_in_transaction()
            .once()
            .return_once(move |_, token| {
                assert_eq!(token.as_str(), "ordinary");
                assert!(locked.load(Ordering::SeqCst));
                Box::pin(async move {
                    if revoked {
                        Err(AuthenticateError::InvalidToken)
                    } else {
                        Ok(staff())
                    }
                })
            });
        if !revoked {
            s.repo
                .expect_commercial_operation()
                .once()
                .return_once(|_, op, actor, _| {
                    assert_eq!(op, "open");
                    assert_eq!(actor, Some(FOO.user.id));
                    Box::pin(async { Ok(json!({"intake":"preserved"})) })
                });
        }
        let result = s
            .commercial_recipient(credentials, "open", json!({"command_id":UUID1}))
            .await;
        if revoked {
            assert!(matches!(
                result.unwrap_err().downcast_ref::<AuthenticateError>(),
                Some(AuthenticateError::InvalidToken)
            ));
        } else {
            assert_eq!(result.unwrap(), json!({"intake":"preserved"}));
        }
    }
    for lane in ["claim", "rights"] {
        let mut s = sut(&[true, true]);
        s.auth = MockAuthService::new();
        s.auth.expect_authenticate().never();
        s.auth.expect_authenticate_in_transaction().never();
        s.repo = MockModerationRepository::new();
        s.repo.expect_lock_session_write().never();
        let credentials = selected(&mut s, lane, true);
        s.repo
            .expect_commercial_operation()
            .once()
            .return_once(|_, op, actor, _| {
                assert_eq!(op, "open");
                assert_eq!(actor, Some(FOO.user.id));
                Box::pin(async { Ok(json!({"intake":"preserved"})) })
            });
        assert_eq!(
            s.commercial_recipient(credentials, "open", json!({"command_id":UUID1}))
                .await
                .unwrap(),
            json!({"intake":"preserved"})
        );
    }
}
