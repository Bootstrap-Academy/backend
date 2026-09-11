use super::*;
use academy_auth_contracts::{Authentication, MockAuthService, internal::MockAuthInternalService};
use academy_core_finance_contracts::MockFinanceFeatureService;
use academy_core_purchase_contracts::MockPurchaseFeatureService;
use academy_core_user_contracts::export::MockUserExportService;
use academy_demo::{SHA256HASH1, UUID1, user::FOO};
use academy_email_contracts::MockEmailService;
use academy_extern_contracts::microservices::MockMicroservicesApiService;
use academy_models::purchase::PurchaseStatus;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, moderation::MockModerationRepository,
};
use academy_shared_contracts::{hash::MockHashService, secret::MockSecretService};
use std::collections::VecDeque;
use uuid::Uuid;
#[path = "commercial_unused_tests.rs"]
mod unused;
use unused::Unused;

fn page_request(family: &str) -> Value {
    json!({"family":family,"limit":2,"cursor":null})
}
fn page_row(family: &str, number: &str) -> Value {
    let at = "2030-01-01 00:00:00.123456+00";
    match family {
        "statements" => {
            json!({"number":number,"review_due_at":at,"authorized":false,"assessment_json":null,"historical_staff_assertion":null,"issued_at":"0001-01-01 00:00:00+00 BC"})
        }
        "archives" => {
            json!({"number":number,"kind":"invoice","source":"record_disposal","recorded_at":"4714-11-24 00:00:00+00 BC","review_due_at":at,"disposal_authorized":false,"assessment_json":"90071992547409931234567890.00000001","disposal_started_at":"infinity","file_removed_at":null})
        }
        "retained_owner_associations" => {
            json!({"number":number,"kind":"","subject":ORIGINAL,"observed_at":"-infinity","review_due_at":at,"source":""})
        }
        "invoice_identity_reviews" => {
            json!({"number":number,"reason":"","source_key":"🙂","observed_at":at,"disposition":"pending_review","evidence_json":"900719925474099312345678901234567890"})
        }
        _ => {
            json!({"number":number,"subject":ORIGINAL,"basis":"","evidence_hash":"not-a-normalized-hash","evidence_json":"null","qualified":false,"observed_at":at})
        }
    }
}
fn page_result(family: &str) -> Value {
    json!({"protocol":1,"family":family,"limit":2,"observed_at":"2026-09-11 00:00:00.1+00","semantics":"live_queue","rows":[page_row(family,""),page_row(family,"🙂")],"next_cursor":null,"exhausted":true})
}
fn expect_page(s: &mut Sut, public: Value, result: anyhow::Result<Value>) {
    s.repo
        .expect_commercial_operation()
        .once()
        .return_once(move |_, op, actor, b| {
            assert_eq!(op, "admin_retention_page");
            assert_eq!(actor, Some(FOO.user.id));
            let mut expected = public;
            expected["_staff_session"] = json!(UUID1);
            expected["_staff_refresh_hash"] = json!(SHA256HASH1.to_string());
            assert_eq!(b, &expected);
            Box::pin(async move { result })
        });
}

#[tokio::test]
async fn commercial_retention_page_fixed_mapping_five_exact_rows_and_opaque_evidence() {
    for family in [
        "statements",
        "archives",
        "retained_owner_associations",
        "invoice_identity_reviews",
        "unqualified_invoice_owner_observations",
    ] {
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        let b = page_request(family);
        let p = page_result(family);
        expect_page(&mut s, b.clone(), Ok(json!({"kind":"page","value":p})));
        assert_eq!(
            s.commercial_admin(&"captured-staff".into(), "retention_page", b)
                .await
                .unwrap(),
            p
        );
    }
}

#[tokio::test]
async fn commercial_retention_page_public_shape_rejects_supplied_proof_and_wrong_controls_before_query()
 {
    let base = page_request("archives");
    let mut invalid = vec![Value::Null, json!([])];
    for k in ["family", "limit", "cursor"] {
        let mut b = base.clone();
        b.as_object_mut().unwrap().remove(k);
        invalid.push(b);
    }
    for k in [
        "_staff_session",
        "_staff_refresh_hash",
        "subject",
        "command_id",
        "offset",
    ] {
        let mut b = base.clone();
        b[k] = json!("caller");
        invalid.push(b);
    }
    for limit in [json!(0), json!(101), json!(1.0), json!(true), json!("1")] {
        let mut b = base.clone();
        b["limit"] = limit;
        invalid.push(b);
    }
    for cursor in [
        json!({}),
        json!({"protocol":1,"family":"statements","after":{"at":"infinity","number":""}}),
        json!({"protocol":1.0,"family":"archives","after":{"at":"infinity","number":"","kind":"invoice"}}),
        json!({"protocol":1,"family":"archives","after":{"at":"infinity","number":"\u{0}","kind":"invoice"}}),
        json!({"protocol":1,"family":"archives","after":{"at":"\u{0}","number":"","kind":"invoice"}}),
    ] {
        let mut b = base.clone();
        b["cursor"] = cursor;
        invalid.push(b);
    }
    let mut b = base.clone();
    b["family"] = json!("arbitrary_table");
    invalid.push(b);
    for b in invalid {
        let mut s = sut(&[]);
        staff_auth(&mut s, Ok(staff()));
        let e = s
            .commercial_admin(&"captured-staff".into(), "retention_page", b)
            .await
            .unwrap_err();
        assert!(matches!(
            e.downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Malformed)
        ));
    }
}

#[tokio::test]
async fn commercial_retention_page_strict_envelope_distinguishes_malformed_and_unavailable() {
    let b = page_request("archives");
    for result in [
        Value::Null,
        json!({}),
        json!({"kind":"malformed","value":null}),
        json!({"kind":"page"}),
        json!({"kind":"unknown","value":{}}),
        json!({"kind":"page","value":page_result("archives"),"extra":true}),
    ] {
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        expect_page(&mut s, b.clone(), Ok(result));
        let e = s
            .commercial_admin(&"captured-staff".into(), "retention_page", b.clone())
            .await
            .unwrap_err();
        assert!(e.downcast_ref::<RecipientAccessError>().is_none());
    }
    let mut s = sut(&[true]);
    staff_auth(&mut s, Ok(staff()));
    expect_page(&mut s, b.clone(), Ok(json!({"kind":"malformed"})));
    assert!(matches!(
        s.commercial_admin(&"captured-staff".into(), "retention_page", b)
            .await
            .unwrap_err()
            .downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::Malformed)
    ));
}

#[tokio::test]
async fn commercial_retention_page_projection_refuses_wrong_types_dates_order_and_cursor_relationships()
 {
    let b = page_request("archives");
    let base = page_result("archives");
    let mut invalid = Vec::new();
    for (k, v) in [
        ("protocol", json!(1.0)),
        ("family", json!("statements")),
        ("limit", json!(3)),
        ("semantics", json!("complete")),
        ("observed_at", json!("infinity")),
        ("exhausted", json!("true")),
        ("rows", Value::Null),
    ] {
        let mut p = base.clone();
        p[k] = v;
        invalid.push(p);
    }
    for (k, v) in [
        ("kind", json!("arbitrary")),
        ("assessment_json", json!({})),
        ("file_removed_at", json!("2030-01-01 00:00:00+00")),
        ("recorded_at", json!("2026-02-30 00:00:00+00")),
        ("review_due_at", json!("2026-01-01T00:00:00Z")),
        ("disposal_authorized", json!(0)),
    ] {
        let mut p = base.clone();
        p["rows"][0][k] = v;
        invalid.push(p);
    }
    let mut p = base.clone();
    p["rows"][0]
        .as_object_mut()
        .unwrap()
        .remove("assessment_json");
    invalid.push(p);
    let mut p = base.clone();
    p["rows"][0]["extra"] = Value::Null;
    invalid.push(p);
    let mut p = base.clone();
    p["rows"][1] = p["rows"][0].clone();
    invalid.push(p);
    let mut p = base.clone();
    p["rows"].as_array_mut().unwrap().reverse();
    invalid.push(p);
    let mut p = base.clone();
    p["rows"] = json!([]);
    p["exhausted"] = json!(false);
    invalid.push(p);
    let mut p = base.clone();
    p["next_cursor"] = json!({"protocol":1,"family":"archives","after":{"at":"2030-01-01 00:00:00.123456+00","number":"🙂","kind":"invoice"}});
    invalid.push(p.clone());
    p["exhausted"] = json!(false);
    p["next_cursor"]["after"]["number"] = json!("lookahead-would-skip");
    invalid.push(p);
    for p in invalid {
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        expect_page(&mut s, b.clone(), Ok(json!({"kind":"page","value":p})));
        assert!(
            s.commercial_admin(&"captured-staff".into(), "retention_page", b.clone())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn commercial_retention_page_native_range_and_valid_last_returned_cursor_preserve_text() {
    let dates = [
        "-infinity",
        "4714-11-24 00:00:00+00 BC",
        "0001-01-01 00:00:00+00 BC",
        "0001-01-01 00:00:00+00",
        "2000-02-29 23:59:59.123456+00",
        "12000-01-01 00:00:00+00",
        "294276-12-31 23:59:59.999999+00",
        "infinity",
    ];
    let rows: Vec<_> = dates
        .iter()
        .map(|at| {
            let mut r = page_row("archives", "");
            r["review_due_at"] = json!(at);
            r
        })
        .collect();
    let mut b = page_request("archives");
    b["limit"] = json!(8);
    let mut p = page_result("archives");
    p["limit"] = json!(8);
    p["rows"] = json!(rows);
    let mut s = sut(&[true]);
    staff_auth(&mut s, Ok(staff()));
    expect_page(&mut s, b.clone(), Ok(json!({"kind":"page","value":p})));
    assert_eq!(
        s.commercial_admin(&"captured-staff".into(), "retention_page", b)
            .await
            .unwrap(),
        p
    );
    let b = page_request("archives");
    let mut p = page_result("archives");
    p["exhausted"] = json!(false);
    p["next_cursor"] = json!({"protocol":1,"family":"archives","after":{"at":"2030-01-01 00:00:00.123456+00","number":"🙂","kind":"invoice"}});
    let mut s = sut(&[true]);
    staff_auth(&mut s, Ok(staff()));
    expect_page(&mut s, b.clone(), Ok(json!({"kind":"page","value":p})));
    assert_eq!(
        s.commercial_admin(&"captured-staff".into(), "retention_page", b)
            .await
            .unwrap(),
        p
    );
}

#[tokio::test]
async fn commercial_retention_page_current_admin_mfa_errors_and_read_errors_keep_existing_classification()
 {
    for mode in ["auth", "auth-other", "admin", "mfa"] {
        let mut s = sut(&[]);
        let mut a = staff();
        let auth = match mode {
            "auth" => Err(academy_models::auth::AuthenticateError::InvalidToken),
            "auth-other" => Err(academy_models::auth::AuthenticateError::Other(
                anyhow::anyhow!("unavailable"),
            )),
            "admin" => {
                a.admin = false;
                Ok(a)
            }
            _ => {
                a.mfa_verified = false;
                Ok(a)
            }
        };
        staff_auth(&mut s, auth);
        assert!(
            s.commercial_admin(
                &"captured-staff".into(),
                "retention_page",
                page_request("archives")
            )
            .await
            .is_err()
        );
    }
    for conflict in [true, false] {
        let mut s = sut(&[false]);
        staff_auth(&mut s, Ok(staff()));
        let b = page_request("archives");
        expect_page(
            &mut s,
            b.clone(),
            Err(if conflict {
                academy_persistence_contracts::moderation::ModerationConflict.into()
            } else {
                anyhow::anyhow!("data unavailable")
            }),
        );
        let e = s
            .commercial_admin(&"captured-staff".into(), "retention_page", b)
            .await
            .unwrap_err();
        assert_eq!(
            matches!(
                e.downcast_ref::<RecipientAccessError>(),
                Some(RecipientAccessError::Conflict)
            ),
            conflict
        );
    }
}

type Sut = ModerationFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockAuthInternalService,
    MockModerationRepository<MockTransaction>,
    Unused,
    MockUserExportService<MockTransaction>,
    MockMicroservicesApiService,
    MockEmailService,
    MockHashService,
    MockSecretService,
    Unused,
    MockPurchaseFeatureService,
    MockFinanceFeatureService,
    Unused,
>;
const OFFER: &str = "11111111-1111-4111-8111-111111111111";
const ORIGINAL: &str = "22222222-2222-4222-8222-222222222222";
fn offer() -> Uuid {
    OFFER.parse().unwrap()
}
fn original() -> UserId {
    Uuid::parse_str(ORIGINAL).unwrap().into()
}
fn database(commits: &[bool]) -> MockDatabase {
    let mut plan = VecDeque::from(commits.to_vec());
    let mut db = MockDatabase::new();
    db.expect_begin_transaction()
        .times(commits.len())
        .returning(move || {
            let mut txn = MockTransaction::new();
            if plan.pop_front().unwrap() {
                txn.expect_commit()
                    .once()
                    .return_once(|| Box::pin(async { Ok(()) }));
            }
            Box::pin(async move { Ok(txn) })
        });
    db
}
fn sut(commits: &[bool]) -> Sut {
    Sut {
        db: database(commits),
        auth: MockAuthService::new(),
        internal: MockAuthInternalService::new(),
        repo: MockModerationRepository::new(),
        session: Unused,
        export: MockUserExportService::new(),
        services: MockMicroservicesApiService::new(),
        email: MockEmailService::new(),
        hash: MockHashService::new(),
        secret: MockSecretService::new(),
        oauth: Unused,
        purchase: MockPurchaseFeatureService::new(),
        finance: MockFinanceFeatureService::new(),
        user_feature: Unused,
    }
}
fn ordinary(s: &mut Sut) -> CommercialCredentials {
    s.auth.expect_authenticate().once().return_once(|token| {
        assert_eq!(token.as_str(), "ordinary");
        Box::pin(async {
            Ok(Authentication {
                user_id: FOO.user.id,
                session_id: UUID1.into(),
                refresh_token_hash: (*SHA256HASH1).into(),
                admin: false,
                email_verified: false,
                mfa_verified: false,
            })
        })
    });
    CommercialCredentials {
        recipient: RecipientCredentials {
            ordinary: Some("ordinary".into()),
            capability: None,
        },
        claim_key: None,
    }
}
fn selected(s: &mut Sut, lane: &str, valid: bool) -> CommercialCredentials {
    let claim = lane == "claim";
    let value = if claim {
        "k".repeat(43)
    } else {
        "c".repeat(40)
    };
    s.hash
        .expect_sha256::<String>()
        .once()
        .return_once(move |v| {
            assert_eq!(v, &value);
            *SHA256HASH1
        });
    let answer = if valid {
        json!({"subject":FOO.user.id,"case_id":UUID1,"scope":if lane=="case"{"case"}else{"rights"}})
    } else {
        Value::Null
    };
    if claim {
        s.repo.expect_commercial_operation().once().return_once(
            move |_, operation, actor, body| {
                assert_eq!(operation, "authenticate");
                assert!(actor.is_none());
                assert_eq!(body, &json!({"hash":SHA256HASH1.to_string()}));
                Box::pin(async move { Ok(answer) })
            },
        );
    } else {
        s.repo
            .expect_operation()
            .once()
            .return_once(move |_, operation, actor, body| {
                assert_eq!(operation, "capability");
                assert!(actor.is_none());
                assert_eq!(body, &json!({"hash":SHA256HASH1.to_string()}));
                Box::pin(async move { Ok(answer) })
            });
    }
    CommercialCredentials {
        claim_key: claim.then(|| "k".repeat(43)),
        recipient: RecipientCredentials {
            ordinary: Some("unexpected fallback".into()),
            capability: Some(if claim {
                "ignored lower".into()
            } else {
                "c".repeat(40)
            }),
        },
    }
}
fn allow_owner(s: &mut Sut) {
    s.repo
        .expect_commercial_purchase_owner()
        .once()
        .return_once(|_, claimant, id| {
            assert_eq!(claimant, FOO.user.id);
            assert_eq!(id, offer());
            Box::pin(async { Ok(original()) })
        });
}
fn status() -> PurchaseStatus {
    serde_json::from_value(json!({
        "offer":{"id":OFFER,"user_id":ORIGINAL,"source":"skills",
            "created_at":"2026-09-01T00:00:00Z","expires_at":"2026-09-01T00:10:00Z",
            "recipient":"original@example.invalid","product":{"kind":"course","reference":"original","title":"Original","description":"Original","coins":7,"facts":{"original":true},"revision":"r1","service_starts_at":null},
            "document_hash":"original-document","hash":"original-offer","text":"original","declaration":"original"},
        "state":"review","accepted_at":null,"confirmation_smtp_accepted_at":null,
        "fulfillment":{"stored":true},"financial_evidence":null,"review_reason":"Original review",
        "provision_deadline":null,"provision_timing":null,"document_corrections":["timing"]
    })).unwrap()
}
fn allow_status(s: &mut Sut) {
    s.repo
        .expect_commercial_purchase_status()
        .once()
        .return_once(|_, claimant, id| {
            assert_eq!(claimant, FOO.user.id);
            assert_eq!(id, offer());
            Box::pin(async { Ok(status()) })
        });
}
const VARIANTS: [&str; 7] = [
    "terms",
    "withdrawal",
    "confirmation",
    "timing",
    "timing-original",
    "fulfillment",
    "fulfillment-original",
];

#[tokio::test]
async fn commercial_original_document_uses_only_resolved_owner_and_preserves_all_seven_bytes() {
    for variant in VARIANTS {
        let mut s = sut(&[true]);
        let credentials = ordinary(&mut s);
        allow_owner(&mut s);
        let bytes = format!("original {variant}\n\0unchanged").into_bytes();
        let expected = bytes.clone();
        s.purchase.expect_recipient_document().once().return_once(
            move |owner, id, actual_variant| {
                assert_eq!(owner, original());
                assert_eq!(id, offer());
                assert_eq!(actual_variant, variant);
                Box::pin(async move { Ok(bytes) })
            },
        );
        assert_eq!(
            s.commercial_document(credentials, "purchase", OFFER, variant)
                .await
                .unwrap(),
            expected
        );
    }
}
#[tokio::test]
async fn commercial_selected_empty_document_is_unavailable_without_variant_fallback() {
    for variant in VARIANTS {
        let mut s = sut(&[true]);
        let credentials = ordinary(&mut s);
        allow_owner(&mut s);
        s.purchase
            .expect_recipient_document()
            .once()
            .return_once(move |owner, id, v| {
                assert_eq!((owner, id, v), (original(), offer(), variant));
                Box::pin(async { Ok(vec![]) })
            });
        let error = s
            .commercial_document(credentials, "purchase", OFFER, variant)
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<PurchaseError>(),
            Some(PurchaseError::Unavailable)
        ));
    }
}
#[tokio::test]
async fn commercial_absent_foreign_or_malformed_original_never_calls_purchase() {
    for unavailable in [false, true] {
        let mut s = sut(&[false]);
        let credentials = ordinary(&mut s);
        s.repo
            .expect_commercial_purchase_owner()
            .once()
            .return_once(move |_, _, _| {
                Box::pin(async move {
                    Err(if unavailable {
                        CommercialPurchaseReadError::Unavailable
                    } else {
                        CommercialPurchaseReadError::NotFound
                    })
                })
            });
        let error = s
            .commercial_document(credentials, "purchase", OFFER, "terms")
            .await
            .unwrap_err();
        assert!(matches!(
            (unavailable, error.downcast_ref::<PurchaseError>()),
            (true, Some(PurchaseError::Unavailable)) | (false, Some(PurchaseError::NotFound))
        ));
    }
}
#[tokio::test]
async fn commercial_document_preserves_unsupported_and_missing_progress_reader_not_found() {
    for variant in ["unsupported", "terms"] {
        let mut s = sut(&[true]);
        let credentials = ordinary(&mut s);
        allow_owner(&mut s);
        s.purchase
            .expect_recipient_document()
            .once()
            .return_once(move |_, _, v| {
                assert_eq!(v, variant);
                Box::pin(async { Err(PurchaseError::NotFound) })
            });
        let error = s
            .commercial_document(credentials, "purchase", OFFER, variant)
            .await
            .unwrap_err();
        assert!(matches!(
            error.downcast_ref::<PurchaseError>(),
            Some(PurchaseError::NotFound)
        ));
    }
}
#[tokio::test]
async fn commercial_status_uses_narrow_repository_and_keeps_original_observations() {
    let mut s = sut(&[true]);
    let credentials = ordinary(&mut s);
    allow_status(&mut s);
    let actual = s
        .commercial_purchase_status(credentials, offer())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        serde_json::to_value(status()).unwrap()
    );
}
#[tokio::test]
async fn commercial_status_preserves_not_found_unavailable_and_database_failure_types() {
    for failure in ["missing", "unavailable", "database"] {
        let mut s = sut(&[false]);
        let credentials = ordinary(&mut s);
        s.repo
            .expect_commercial_purchase_status()
            .once()
            .return_once(move |_, _, _| {
                Box::pin(async move {
                    Err(match failure {
                        "missing" => CommercialPurchaseReadError::NotFound,
                        "unavailable" => CommercialPurchaseReadError::Unavailable,
                        _ => {
                            CommercialPurchaseReadError::Other(anyhow!("synthetic DB unavailable"))
                        }
                    })
                })
            });
        let error = s
            .commercial_purchase_status(credentials, offer())
            .await
            .unwrap_err();
        assert!(matches!(
            (failure, error.downcast_ref::<PurchaseError>()),
            ("missing", Some(PurchaseError::NotFound))
                | ("unavailable", Some(PurchaseError::Unavailable))
                | ("database", Some(PurchaseError::Other(_)))
        ));
    }
}
#[tokio::test]
async fn commercial_claim_and_full_rights_principals_reach_only_the_exact_read_lane() {
    for lane in ["claim", "rights"] {
        for document in [false, true] {
            let mut s = sut(&[true, true]);
            let credentials = selected(&mut s, lane, true);
            if document {
                allow_owner(&mut s);
                s.purchase
                    .expect_recipient_document()
                    .once()
                    .return_once(|owner, id, v| {
                        assert_eq!((owner, id, v), (original(), offer(), "terms"));
                        Box::pin(async { Ok(b"original".to_vec()) })
                    });
                s.commercial_document(credentials, "purchase", OFFER, "terms")
                    .await
                    .unwrap();
            } else {
                allow_status(&mut s);
                s.commercial_purchase_status(credentials, offer())
                    .await
                    .unwrap();
            }
        }
    }
}
#[tokio::test]
async fn commercial_case_only_and_expired_selected_proof_cannot_fall_through_to_ordinary() {
    for (lane, valid) in [("case", true), ("claim", false), ("rights", false)] {
        for document in [false, true] {
            let mut s = sut(&[true]);
            let credentials = selected(&mut s, lane, valid);
            let error = if document {
                s.commercial_document(credentials, "purchase", OFFER, "terms")
                    .await
                    .unwrap_err()
            } else {
                s.commercial_purchase_status(credentials, offer())
                    .await
                    .unwrap_err()
            };
            assert!(matches!(
                (lane, error.downcast_ref::<RecipientAccessError>()),
                ("case", Some(RecipientAccessError::Scope))
                    | ("claim" | "rights", Some(RecipientAccessError::Invalid))
            ));
        }
    }
}
#[tokio::test]
async fn commercial_empty_short_selected_strings_are_rejected_before_any_repository() {
    for claim in [false, true] {
        for value in ["", "short"] {
            for document in [false, true] {
                let s = sut(&[]);
                let credentials = CommercialCredentials {
                    claim_key: claim.then(|| value.into()),
                    recipient: RecipientCredentials {
                        capability: Some(if claim {
                            "ignored lower".into()
                        } else {
                            value.into()
                        }),
                        ordinary: Some("unexpected fallback".into()),
                    },
                };
                let error = if document {
                    s.commercial_document(credentials, "purchase", OFFER, "terms")
                        .await
                        .unwrap_err()
                } else {
                    s.commercial_purchase_status(credentials, offer())
                        .await
                        .unwrap_err()
                };
                assert!(matches!(
                    error.downcast_ref::<RecipientAccessError>(),
                    Some(RecipientAccessError::Invalid)
                ));
            }
        }
    }
}
#[tokio::test]
async fn commercial_finance_read_keeps_its_existing_owner_path_without_purchase_resolution() {
    let mut s = sut(&[]);
    let credentials = ordinary(&mut s);
    s.finance
        .expect_download_recipient_original()
        .once()
        .return_once(|owner, kind, number, month| {
            assert_eq!(owner, FOO.user.id);
            assert_eq!(
                kind,
                academy_models::finance::FinancialDocumentKind::Invoice
            );
            assert_eq!((number, month), (42, 0));
            Box::pin(async { Ok(b"original finance".to_vec()) })
        });
    assert_eq!(
        s.commercial_document(credentials, "invoice", "42", "unused")
            .await
            .unwrap(),
        b"original finance"
    );
}

fn inventory() -> academy_models::commercial_document::DocumentInventory {
    serde_json::from_value(json!({"protocol":1,"claimant_subject":FOO.user.id,"observed_at":"2026-09-10T00:00:00Z","scope":{"finance":"claimant_only","purchases":"claimant_and_same_case_learning_subjects","archives_scanned":false,"remote_sources_queried":false,"catalog_complete":false,"historical_owner_inventory_complete":false,"known_local_enumeration_complete":true},"records":[]})).unwrap()
}
#[tokio::test]
async fn commercial_inventory_authenticates_then_uses_only_fresh_metadata_transaction() {
    for lane in ["ordinary", "claim", "capability"] {
        let mut s = sut(if lane == "ordinary" {
            &[true]
        } else {
            &[true, true]
        });
        let credentials = if lane == "ordinary" {
            ordinary(&mut s)
        } else {
            selected(&mut s, lane, true)
        };
        s.repo
            .expect_commercial_document_inventory()
            .once()
            .return_once(|_, claimant| {
                assert_eq!(claimant, FOO.user.id);
                Box::pin(async { Ok(inventory()) })
            });
        assert_eq!(
            s.commercial_document_inventory(credentials).await.unwrap(),
            inventory()
        );
    }
}
#[tokio::test]
async fn commercial_inventory_refuses_invalid_selected_and_case_proofs_without_fallback() {
    for (lane, valid) in [("claim", false), ("capability", false), ("case", true)] {
        let mut s = sut(&[true]);
        let credentials = selected(&mut s, lane, valid);
        assert!(s.commercial_document_inventory(credentials).await.is_err());
    }
    let s = sut(&[]);
    assert!(
        s.commercial_document_inventory(CommercialCredentials {
            claim_key: None,
            recipient: RecipientCredentials {
                ordinary: None,
                capability: None
            }
        })
        .await
        .is_err()
    );
}
#[tokio::test]
async fn commercial_inventory_query_failure_is_not_empty_success_and_does_not_commit() {
    let mut s = sut(&[false]);
    let credentials = ordinary(&mut s);
    s.repo
        .expect_commercial_document_inventory()
        .once()
        .return_once(|_, _| Box::pin(async { anyhow::bail!("synthetic metadata read failure") }));
    assert!(s.commercial_document_inventory(credentials).await.is_err());
}

fn staff() -> Authentication {
    Authentication {
        user_id: FOO.user.id,
        session_id: UUID1.into(),
        refresh_token_hash: (*SHA256HASH1).into(),
        admin: true,
        email_verified: false,
        mfa_verified: true,
    }
}
fn staff_auth(
    s: &mut Sut,
    result: Result<Authentication, academy_models::auth::AuthenticateError>,
) {
    s.auth
        .expect_authenticate()
        .once()
        .return_once(move |token| {
            assert_eq!(token.as_str(), "captured-staff");
            Box::pin(async move { result })
        });
}
fn case_owner(s: &mut Sut, owner: Option<UserId>) {
    s.repo
        .expect_commercial_case_subject()
        .once()
        .return_once(move |_, id| {
            assert_eq!(id, UUID1);
            Box::pin(async move { Ok(owner) })
        });
}
#[tokio::test]
async fn commercial_staff_capacity_exact_public_body_and_read_result() {
    for body in [
        json!({}),
        json!({"case_id":UUID1}),
        json!({"case_id":null,"subject":FOO.user.id}),
        json!({"case_id":UUID1,"subject":1}),
        json!({"case_id":"bad","subject":FOO.user.id}),
        json!({"case_id":UUID1,"subject":FOO.user.id,"_staff_session":UUID1}),
        json!({"case_id":UUID1,"subject":FOO.user.id,"actor":FOO.user.id}),
        json!({"case_id":UUID1,"subject":FOO.user.id,"command_id":UUID1}),
    ] {
        let mut s = sut(&[]);
        staff_auth(&mut s, Ok(staff()));
        let err = s
            .commercial_admin(&"captured-staff".into(), "cash_capacity", body)
            .await
            .unwrap_err();
        assert!(matches!(
            err.downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Malformed)
        ));
    }
    for null in [false, true] {
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        let expected = json!({"case_id":UUID1,"subject":original(),"remaining_purchase_capacity":null,"captured_purchase_units":"9007199254740993"});
        let result = if null { Value::Null } else { expected.clone() };
        s.repo.expect_commercial_operation().once().return_once(move |_,op,actor,body| {
            assert_eq!(op,"admin_cash_capacity");assert_eq!(actor,Some(FOO.user.id));
            assert_eq!(body,&json!({"case_id":UUID1,"subject":original(),"_staff_session":UUID1,"_staff_refresh_hash":SHA256HASH1.to_string()}));
            Box::pin(async move {Ok(result)})
        });
        let result = s
            .commercial_admin(
                &"captured-staff".into(),
                "cash_capacity",
                json!({"case_id":UUID1,"subject":original()}),
            )
            .await;
        if null {
            assert!(matches!(
                result.unwrap_err().downcast_ref::<RecipientAccessError>(),
                Some(RecipientAccessError::NotFound)
            ));
        } else {
            assert_eq!(result.unwrap(), expected);
        }
    }
}
#[tokio::test]
async fn commercial_staff_reads_require_current_admin_mfa_before_repository() {
    for mode in ["invalid", "admin", "mfa"] {
        for document in [false, true] {
            let mut s = sut(&[]);
            let mut a = staff();
            a.admin = mode != "admin";
            a.mfa_verified = mode != "mfa";
            staff_auth(
                &mut s,
                if mode == "invalid" {
                    Err(academy_models::auth::AuthenticateError::InvalidToken)
                } else {
                    Ok(a)
                },
            );
            let err = if document {
                s.commercial_admin_document(
                    &"captured-staff".into(),
                    UUID1,
                    "invoice",
                    "10000000",
                    "original",
                )
                .await
                .unwrap_err()
            } else {
                s.commercial_admin(
                    &"captured-staff".into(),
                    "cash_capacity",
                    json!({"case_id":UUID1,"subject":original()}),
                )
                .await
                .unwrap_err()
            };
            assert!(
                err.downcast_ref::<academy_models::auth::AuthenticateError>()
                    .is_some()
                    || err
                        .downcast_ref::<academy_models::auth::AuthorizeError>()
                        .is_some()
            );
        }
    }
}
#[tokio::test]
async fn commercial_staff_originals_share_seven_exact_stored_owner_reads() {
    for variant in VARIANTS {
        let mut s = sut(&[true, true]);
        staff_auth(&mut s, Ok(staff()));
        case_owner(&mut s, Some(FOO.user.id));
        allow_owner(&mut s);
        let bytes = format!("stored {variant}\r\n\0unaltered").into_bytes();
        let expected = bytes.clone();
        s.purchase
            .expect_recipient_document()
            .once()
            .return_once(move |owner, id, v| {
                assert_eq!((owner, id, v), (original(), offer(), variant));
                Box::pin(async move { Ok(bytes) })
            });
        staff_auth(&mut s, Ok(staff()));
        assert_eq!(
            s.commercial_admin_document(
                &"captured-staff".into(),
                UUID1,
                "purchase",
                OFFER,
                variant
            )
            .await
            .unwrap(),
            expected
        );
    }
}
#[tokio::test]
async fn commercial_staff_original_errors_are_preserved_only_after_final_auth() {
    for mode in [
        "case_missing",
        "case_failure",
        "owner_missing",
        "owner_unavailable",
        "empty",
        "reader_missing",
        "reader_unavailable",
        "malformed",
    ] {
        let commits = match mode {
            "case_missing" | "case_failure" => vec![false],
            "malformed" => vec![true],
            "owner_missing" | "owner_unavailable" => vec![true, false],
            _ => vec![true, true],
        };
        let mut s = sut(&commits);
        staff_auth(&mut s, Ok(staff()));
        if mode == "case_failure" {
            s.repo
                .expect_commercial_case_subject()
                .once()
                .return_once(|_, _| {
                    Box::pin(async { Err(anyhow::anyhow!("synthetic repository unavailable")) })
                });
        } else {
            case_owner(
                &mut s,
                if mode == "case_missing" {
                    None
                } else {
                    Some(FOO.user.id)
                },
            );
        }
        if matches!(mode, "owner_missing" | "owner_unavailable") {
            s.repo
                .expect_commercial_purchase_owner()
                .once()
                .return_once(move |_, _, _| {
                    Box::pin(async move {
                        Err(if mode == "owner_missing" {
                            CommercialPurchaseReadError::NotFound
                        } else {
                            CommercialPurchaseReadError::Unavailable
                        })
                    })
                });
        } else if matches!(mode, "empty" | "reader_missing" | "reader_unavailable") {
            allow_owner(&mut s);
            s.purchase
                .expect_recipient_document()
                .once()
                .return_once(move |_, _, _| {
                    Box::pin(async move {
                        match mode {
                            "empty" => Ok(vec![]),
                            "reader_missing" => Err(PurchaseError::NotFound),
                            _ => Err(PurchaseError::Unavailable),
                        }
                    })
                });
        }
        staff_auth(&mut s, Ok(staff()));
        let err = s
            .commercial_admin_document(
                &"captured-staff".into(),
                UUID1,
                "purchase",
                if mode == "malformed" { "bad" } else { OFFER },
                "timing",
            )
            .await
            .unwrap_err();
        match mode {
            "case_missing" => assert!(matches!(
                err.downcast_ref::<RecipientAccessError>(),
                Some(RecipientAccessError::NotFound)
            )),
            "malformed" => assert!(matches!(
                err.downcast_ref::<RecipientAccessError>(),
                Some(RecipientAccessError::Malformed)
            )),
            "owner_missing" | "reader_missing" => assert!(matches!(
                err.downcast_ref::<PurchaseError>(),
                Some(PurchaseError::NotFound)
            )),
            "case_failure" => assert!(err.to_string().contains("synthetic repository")),
            _ => assert!(matches!(
                err.downcast_ref::<PurchaseError>(),
                Some(PurchaseError::Unavailable)
            )),
        }
    }
}
#[tokio::test]
async fn commercial_staff_held_bytes_and_reader_errors_recheck_exact_authority_after_completion() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for fail_reader in [false, true] {
        for mode in [
            "same",
            "invalid",
            "admin",
            "mfa",
            "actor",
            "session",
            "refresh",
            "auth_unavailable",
        ] {
            let mut s = sut(&[true]);
            staff_auth(&mut s, Ok(staff()));
            case_owner(&mut s, Some(original()));
            let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();
            let done = Arc::new(AtomicBool::new(false));
            let done_reader = Arc::clone(&done);
            s.finance
                .expect_download_recipient_original()
                .once()
                .return_once(move |owner, kind, number, month| {
                    assert_eq!(owner, original());
                    assert_eq!(
                        kind,
                        academy_models::finance::FinancialDocumentKind::Invoice
                    );
                    assert_eq!((number, month), (10000000, 0));
                    Box::pin(async move {
                        entered_tx.send(()).unwrap();
                        release_rx.await.unwrap();
                        done_reader.store(true, Ordering::SeqCst);
                        if fail_reader {
                            Err(academy_core_finance_contracts::FinanceDownloadError::NotFound)
                        } else {
                            Ok(b"original held bytes".to_vec())
                        }
                    })
                });
            let checked = Arc::new(AtomicBool::new(false));
            let checked_auth = Arc::clone(&checked);
            let done_auth = Arc::clone(&done);
            s.auth
                .expect_authenticate()
                .once()
                .return_once(move |token| {
                    assert_eq!(token.as_str(), "captured-staff");
                    assert!(done_auth.load(Ordering::SeqCst));
                    checked_auth.store(true, Ordering::SeqCst);
                    Box::pin(async move {
                        let mut a = staff();
                        match mode {
                            "invalid" => {
                                return Err(academy_models::auth::AuthenticateError::InvalidToken);
                            }
                            "auth_unavailable" => {
                                return Err(academy_models::auth::AuthenticateError::Other(
                                    anyhow::anyhow!("auth unavailable"),
                                ));
                            }
                            "admin" => a.admin = false,
                            "mfa" => a.mfa_verified = false,
                            "actor" => a.user_id = original(),
                            "session" => a.session_id = Uuid::new_v4().into(),
                            "refresh" => a.refresh_token_hash = (*academy_demo::SHA256HASH2).into(),
                            _ => {}
                        }
                        Ok(a)
                    })
                });
            let token = "captured-staff".into();
            let request =
                s.commercial_admin_document(&token, UUID1, "invoice", "10000000", "original");
            let driver = async {
                entered_rx.await.unwrap();
                assert!(!done.load(Ordering::SeqCst));
                assert!(!checked.load(Ordering::SeqCst));
                release_tx.send(()).unwrap();
            };
            let (result, ()) = tokio::join!(request, driver);
            assert!(checked.load(Ordering::SeqCst));
            if mode == "same" {
                if fail_reader {
                    assert!(matches!(
                        result
                            .unwrap_err()
                            .downcast_ref::<academy_core_finance_contracts::FinanceDownloadError>(),
                        Some(academy_core_finance_contracts::FinanceDownloadError::NotFound)
                    ));
                } else {
                    assert_eq!(result.unwrap(), b"original held bytes");
                }
            } else {
                let err = result.unwrap_err();
                assert!(
                    err.downcast_ref::<RecipientAccessError>().is_some()
                        || err
                            .downcast_ref::<academy_models::auth::AuthenticateError>()
                            .is_some()
                        || err
                            .downcast_ref::<academy_models::auth::AuthorizeError>()
                            .is_some()
                );
            }
        }
    }
}

fn hold_public_body() -> Value {
    json!({"version":1,"command_id":UUID1,"case_id":UUID1,"subject":original(),
        "hold":{"kind":"financial_document","record_id":" exact original / Ä 10000000 "},
        "expected":{"incarnation_id":UUID1,"review_version":"0"},"decision":"keep",
        "review_scope":"entire_existing_hold","assessment":"Specific human assessment of the entire existing hold",
        "next_review_at":"2500-01-01T00:00:00.123456+00:00"})
}
#[tokio::test]
async fn commercial_hold_exact_public_bodies_preserve_literals_and_inject_only_current_proof() {
    let mut bodies = Vec::new();
    for kind in [
        "financial_document",
        "contract_declaration",
        "renewal_agreement",
        "legacy_renewal",
    ] {
        let mut b = hold_public_body();
        b["hold"]["kind"] = json!(kind);
        if kind != "financial_document" {
            b["hold"]["record_id"] = json!(UUID1);
        }
        bodies.push(("hold_review", b));
    }
    bodies.push(("hold_queue", json!({"version":1,"limit":100,"cursor":null})));
    bodies.push(("hold_queue",json!({"version":1,"limit":1,"cursor":{"review_due_at":"infinity","kind":"financial_document","case_id":UUID1,"record_id":" exact literal ä ","incarnation_id":UUID1}})));
    for (op, b) in bodies {
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        let saved = b.clone();
        s.repo.expect_commercial_operation().once().return_once(
            move |_, operation, actor, body| {
                assert_eq!(operation, op);
                assert_eq!(actor, Some(FOO.user.id));
                let mut exact = saved;
                exact["_staff_session"] = json!(UUID1);
                exact["_staff_refresh_hash"] = json!(SHA256HASH1.to_string());
                assert_eq!(body, &exact);
                Box::pin(async { Ok(json!({"opaque":"original receipt or live observation"})) })
            },
        );
        assert_eq!(
            s.commercial_admin(&"captured-staff".into(), op, b)
                .await
                .unwrap(),
            json!({"opaque":"original receipt or live observation"})
        );
    }
}
#[tokio::test]
async fn commercial_hold_malformed_public_schema_never_reaches_repository() {
    let original_body = hold_public_body();
    let mut malformed = vec![json!({}), json!(null)];
    for (pointer, value) in [
        ("/version", json!(2)),
        ("/expected/review_version", json!(0)),
        ("/expected/review_version", json!("01")),
        ("/expected/review_version", json!("9223372036854775808")),
        ("/expected/incarnation_id", json!("bad")),
        ("/decision", json!("release")),
        ("/review_scope", json!("some_fields")),
        ("/assessment", json!(" too short ")),
        ("/hold/kind", json!("wallet")),
        ("/hold/record_id", json!("")),
        ("/subject", json!(null)),
        ("/next_review_at", json!(23)),
    ] {
        let mut b = original_body.clone();
        *b.pointer_mut(pointer).unwrap() = value;
        malformed.push(b);
    }
    for field in [
        "_staff_session",
        "_staff_refresh_hash",
        "necessary_fields",
        "actor",
    ] {
        let mut b = original_body.clone();
        b[field] = json!(UUID1);
        malformed.push(b);
    }
    let mut b = original_body.clone();
    b["hold"]["extra"] = json!(true);
    malformed.push(b);
    let mut b = original_body.clone();
    b["hold"]["kind"] = json!("contract_declaration");
    malformed.push(b);
    for b in malformed {
        let mut s = sut(&[]);
        staff_auth(&mut s, Ok(staff()));
        assert!(matches!(
            s.commercial_admin(&"captured-staff".into(), "hold_review", b)
                .await
                .unwrap_err()
                .downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Malformed)
        ));
    }
    for b in [
        json!({"version":1,"limit":0,"cursor":null}),
        json!({"version":1,"limit":101,"cursor":null}),
        json!({"version":1,"limit":1.5,"cursor":null}),
        json!({"version":1,"limit":1}),
        json!({"version":1,"limit":1,"cursor":{},"subject":original()}),
        json!({"version":1,"limit":1,"cursor":{"review_due_at":"infinity","kind":"financial_document","case_id":UUID1,"record_id":"n","incarnation_id":UUID1,"offset":0}}),
    ] {
        let mut s = sut(&[]);
        staff_auth(&mut s, Ok(staff()));
        assert!(matches!(
            s.commercial_admin(&"captured-staff".into(), "hold_queue", b)
                .await
                .unwrap_err()
                .downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Malformed)
        ));
    }
}
#[tokio::test]
async fn commercial_hold_current_staff_denial_and_database_conflict_preserve_boundaries() {
    for op in ["hold_review", "hold_queue"] {
        for mfa in [true, false] {
            let mut s = sut(&[]);
            let mut a = staff();
            a.admin = mfa;
            a.mfa_verified = !mfa;
            staff_auth(&mut s, Ok(a));
            assert!(
                s.commercial_admin(&"captured-staff".into(), op, hold_public_body())
                    .await
                    .unwrap_err()
                    .downcast_ref::<academy_models::auth::AuthorizeError>()
                    .is_some()
            );
        }
    }
    let mut s = sut(&[false]);
    staff_auth(&mut s, Ok(staff()));
    s.repo
        .expect_commercial_operation()
        .once()
        .return_once(|_, _, _, _| {
            Box::pin(async {
                Err(academy_persistence_contracts::moderation::ModerationConflict.into())
            })
        });
    assert!(matches!(
        s.commercial_admin(&"captured-staff".into(), "hold_review", hold_public_body())
            .await
            .unwrap_err()
            .downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::Conflict)
    ));
}

fn determination_status_body() -> Value {
    json!({"case_id":UUID1,"subject":original(),"obligation_id":UUID1,"command_id":null})
}
fn determination_projection() -> Value {
    json!({"protocol":1,"case_id":UUID1,"subject":original(),"observed_at":"2026-09-11 00:00:00.123456+00","obligation":{"id":UUID1,"source":"","source_key":" x ","component":"preserved component","status":"pending_evidence","units":null,"cash_units":"9223372036854775807","original_json":"9007199254740993","determination_json":"null"},"journal":null})
}
#[tokio::test]
async fn commercial_determination_status_public_shape_and_lossless_result() {
    let mut bad = Vec::new();
    for field in ["case_id", "subject", "obligation_id", "command_id"] {
        let mut b = determination_status_body();
        b.as_object_mut().unwrap().remove(field);
        bad.push(b);
        let mut b = determination_status_body();
        b[field] = json!(12);
        bad.push(b);
        let mut b = determination_status_body();
        b[field] = json!("not-uuid");
        bad.push(b);
    }
    for field in ["_staff_session", "actor", "extra"] {
        let mut b = determination_status_body();
        b[field] = json!(UUID1);
        bad.push(b);
    }
    for b in bad {
        let mut s = sut(&[]);
        staff_auth(&mut s, Ok(staff()));
        let e = s
            .commercial_admin(&"captured-staff".into(), "determination_status", b)
            .await
            .unwrap_err();
        assert!(matches!(
            e.downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Malformed)
        ));
    }
    for mutation in ["missing", "identity", "amount", "raw_json", "journal"] {
        let mut result = determination_projection();
        match mutation {
            "missing" => {
                result.as_object_mut().unwrap().remove("journal");
            }
            "identity" => result["obligation"]["id"] = json!(Uuid::new_v4()),
            "amount" => result["obligation"]["units"] = json!(9007199254740993u64),
            "raw_json" => result["obligation"]["original_json"] = json!({"must":"remain raw text"}),
            _ => result["journal"] = json!({"id":"1"}),
        }
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        staff_auth(&mut s, Ok(staff()));
        s.repo
            .expect_commercial_operation()
            .once()
            .return_once(move |_, _, _, _| Box::pin(async move { Ok(result) }));
        assert!(
            s.commercial_admin(
                &"captured-staff".into(),
                "determination_status",
                determination_status_body()
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("Unavailable determination status projection")
        );
    }
    for command in [Value::Null, json!(UUID1)] {
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        staff_auth(&mut s, Ok(staff()));
        let mut b = determination_status_body();
        b["command_id"] = command;
        let original_b = b.clone();
        let mut expected = determination_projection();
        if !b["command_id"].is_null() {
            expected["journal"] = json!({"id":"-9223372036854775808","case_id":UUID1,"obligation_id":UUID1,"actor":FOO.user.id,"command_id":UUID1,"kind":"determine","request_json":"{\"units\":9007199254740993}","result_json":"null","recorded_at":"infinity"});
        }
        let ret = expected.clone();
        s.repo
            .expect_commercial_operation()
            .once()
            .return_once(move |_, op, actor, actual| {
                assert_eq!(op, "admin_determination_status");
                assert_eq!(actor, Some(FOO.user.id));
                let mut expected_b = original_b;
                expected_b["_staff_session"] = json!(UUID1);
                expected_b["_staff_refresh_hash"] = json!(SHA256HASH1.to_string());
                assert_eq!(actual, &expected_b);
                Box::pin(async move { Ok(ret) })
            });
        assert_eq!(
            s.commercial_admin(&"captured-staff".into(), "determination_status", b)
                .await
                .unwrap(),
            expected
        );
    }
}
#[tokio::test]
async fn commercial_determination_status_entire_error_result_rechecks_auth() {
    for mode in ["begin", "query", "commit", "missing", "projection"] {
        for deny in [false, true] {
            let mut s = sut(&[]);
            staff_auth(&mut s, Ok(staff()));
            staff_auth(
                &mut s,
                if deny {
                    Err(academy_models::auth::AuthenticateError::InvalidToken)
                } else {
                    Ok(staff())
                },
            );
            if mode == "begin" {
                s.db.expect_begin_transaction()
                    .once()
                    .return_once(|| Box::pin(async { anyhow::bail!("synthetic begin error") }));
            } else {
                s.db.expect_begin_transaction().once().return_once(move || {
                    let mut t = MockTransaction::new();
                    if mode != "query" {
                        t.expect_commit().once().return_once(move || {
                            Box::pin(async move {
                                if mode == "commit" {
                                    anyhow::bail!("synthetic commit error")
                                }
                                Ok(())
                            })
                        });
                    }
                    Box::pin(async move { Ok(t) })
                });
                s.repo
                    .expect_commercial_operation()
                    .once()
                    .return_once(move |_, op, _, _| {
                        assert_eq!(op, "admin_determination_status");
                        Box::pin(async move {
                            if mode == "query" {
                                anyhow::bail!("synthetic query error")
                            }
                            Ok(if mode == "projection" {
                                json!({})
                            } else {
                                Value::Null
                            })
                        })
                    });
            }
            let e = s
                .commercial_admin(
                    &"captured-staff".into(),
                    "determination_status",
                    determination_status_body(),
                )
                .await
                .unwrap_err();
            if deny {
                assert!(
                    e.downcast_ref::<academy_models::auth::AuthenticateError>()
                        .is_some()
                );
            } else if mode == "missing" {
                assert!(matches!(
                    e.downcast_ref::<RecipientAccessError>(),
                    Some(RecipientAccessError::NotFound)
                ));
            } else {
                assert!(e.to_string().contains(mode), "{e:?}");
            }
        }
    }
}
#[tokio::test]
async fn commercial_determination_status_held_result_cannot_outlive_captured_authority() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for fail in [false, true] {
        for mode in [
            "same",
            "invalid",
            "admin",
            "mfa",
            "actor",
            "session",
            "refresh",
            "unavailable",
        ] {
            let mut s = sut(&[!fail]);
            staff_auth(&mut s, Ok(staff()));
            let (entered, wait) = tokio::sync::oneshot::channel();
            let (release, hold) = tokio::sync::oneshot::channel();
            let done = Arc::new(AtomicBool::new(false));
            let d = Arc::clone(&done);
            s.repo
                .expect_commercial_operation()
                .once()
                .return_once(move |_, op, _, _| {
                    assert_eq!(op, "admin_determination_status");
                    Box::pin(async move {
                        entered.send(()).unwrap();
                        hold.await.unwrap();
                        d.store(true, Ordering::SeqCst);
                        if fail {
                            anyhow::bail!("synthetic unavailable projection")
                        }
                        Ok(determination_projection())
                    })
                });
            let d = Arc::clone(&done);
            s.auth.expect_authenticate().once().return_once(move |t| {
                assert_eq!(t.as_str(), "captured-staff");
                assert!(d.load(Ordering::SeqCst));
                Box::pin(async move {
                    let mut a = staff();
                    match mode {
                        "invalid" => {
                            return Err(academy_models::auth::AuthenticateError::InvalidToken);
                        }
                        "unavailable" => {
                            return Err(academy_models::auth::AuthenticateError::Other(
                                anyhow::anyhow!("auth unavailable"),
                            ));
                        }
                        "admin" => a.admin = false,
                        "mfa" => a.mfa_verified = false,
                        "actor" => a.user_id = original(),
                        "session" => a.session_id = Uuid::new_v4().into(),
                        "refresh" => a.refresh_token_hash = (*academy_demo::SHA256HASH2).into(),
                        _ => {}
                    }
                    Ok(a)
                })
            });
            let token = "captured-staff".into();
            let request =
                s.commercial_admin(&token, "determination_status", determination_status_body());
            let drive = async {
                tokio::time::timeout(std::time::Duration::from_secs(2), wait)
                    .await
                    .expect("repository entry bounded wait")
                    .unwrap();
                assert!(!done.load(Ordering::SeqCst));
                release.send(()).unwrap();
            };
            let (r, ()) = tokio::join!(request, drive);
            if mode == "same" {
                if fail {
                    assert!(r.unwrap_err().to_string().contains("synthetic unavailable"));
                } else {
                    assert_eq!(r.unwrap(), determination_projection());
                }
            } else {
                let e = r.unwrap_err();
                assert!(
                    e.downcast_ref::<RecipientAccessError>().is_some()
                        || e.downcast_ref::<academy_models::auth::AuthenticateError>()
                            .is_some()
                        || e.downcast_ref::<academy_models::auth::AuthorizeError>()
                            .is_some()
                );
            }
        }
    }
}
#[tokio::test]
async fn commercial_determine_legacy_body_reaches_original_sql_admission() {
    let mut s = sut(&[true]);
    staff_auth(&mut s, Ok(staff()));
    let body = json!({"command_id":UUID1,"case_id":UUID1,"obligation_id":UUID1,"units":9007199254740993u64,"assessment":"Original exact legacy assessment","evidence":{"retained":"original"},"extra":"old admitted extra"});
    let old = body.clone();
    s.repo
        .expect_commercial_operation()
        .once()
        .return_once(move |_, op, _, b| {
            assert_eq!(op, "determine");
            let mut copy = b.clone();
            copy.as_object_mut().unwrap().remove("_staff_session");
            copy.as_object_mut().unwrap().remove("_staff_refresh_hash");
            assert_eq!(copy, old);
            Box::pin(async {
                Ok(json!({"obligation_id":UUID1,"status":"established","paid":false}))
            })
        });
    assert_eq!(
        s.commercial_admin(&"captured-staff".into(), "determine", body)
            .await
            .unwrap()["paid"],
        false
    );
}

#[tokio::test]
async fn commercial_wallet_restore_preserves_legacy_business_body_and_selected_personal_proof() {
    for lane in ["claim", "rights"] {
        for body in [
            json!({"command_id":UUID1,"obligation_id":UUID1,"units":"125","choose_coins":true,"old_extra":{"ignored":true}}),
            json!({"command_id":UUID1,"obligation_id":UUID1,"expected_subject":null,"units":125,"choose_coins":true}),
        ] {
            let mut s = sut(&[true, true]);
            // Mutation proof hashing precedes principal admission, which independently hashes it.
            s.hash
                .expect_sha256::<String>()
                .once()
                .return_once(|_| *SHA256HASH1);
            let credentials = selected(&mut s, lane, true);
            let mut exact = body.clone();
            exact[if lane == "claim" {
                "_claim_hash"
            } else {
                "_moderation_hash"
            }] = json!(SHA256HASH1.to_string());
            s.repo.expect_commercial_operation().once().return_once(
                move |_, operation, actor, b| {
                    assert_eq!(operation, "restore_credit");
                    assert_eq!(actor, Some(FOO.user.id));
                    assert_eq!(b, &exact);
                    Box::pin(async { Ok(json!({"historical":"opaque original receipt"})) })
                },
            );
            let mut public = body;
            public["_claim_hash"] = json!("untrusted");
            public["_moderation_hash"] = json!("untrusted");
            assert_eq!(
                s.commercial_recipient(credentials, "restore_credit", public)
                    .await
                    .unwrap(),
                json!({"historical":"opaque original receipt"})
            );
        }
    }
    let s = sut(&[]);
    let e = s
        .commercial_recipient(
            CommercialCredentials {
                claim_key: None,
                recipient: RecipientCredentials {
                    ordinary: Some("ordinary".into()),
                    capability: None,
                },
            },
            "restore_credit",
            json!({"command_id":UUID1}),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        e.downcast_ref::<RecipientAccessError>(),
        Some(RecipientAccessError::Scope)
    ));
}

const CLOSED_SETTLEMENTS: [&str; 5] = [
    "cash_basis_review",
    "reserve",
    "split_cash",
    "record_cash_payment",
    "outcome",
];

#[tokio::test]
async fn commercial_close_staff_settlements_reject_without_dispatch() {
    for operation in CLOSED_SETTLEMENTS {
        for body in [
            json!({}),
            json!({"command_id":UUID1,"case_id":UUID1,"subject":original(),
                "obligation_id":UUID1,"units":"9007199254740993","cash_units":null,
                "original":"preserve exact saved body","operation":"queue",
                "_staff_session":"untrusted","_staff_refresh_hash":"untrusted"}),
        ] {
            let mut s = sut(&[]);
            staff_auth(&mut s, Ok(staff()));
            let err = s
                .commercial_admin(&"captured-staff".into(), operation, body)
                .await
                .unwrap_err();
            assert!(matches!(
                err.downcast_ref::<RecipientAccessError>(),
                Some(RecipientAccessError::Malformed)
            ));
        }
    }
}

#[tokio::test]
async fn commercial_close_staff_settlements_keep_auth_before_operation_admission() {
    for operation in CLOSED_SETTLEMENTS {
        for mode in ["invalid", "admin", "mfa"] {
            let mut s = sut(&[]);
            let mut auth = staff();
            auth.admin = mode != "admin";
            auth.mfa_verified = mode != "mfa";
            staff_auth(
                &mut s,
                if mode == "invalid" {
                    Err(academy_models::auth::AuthenticateError::InvalidToken)
                } else {
                    Ok(auth)
                },
            );
            let err = s
                .commercial_admin(&"captured-staff".into(), operation, Value::Null)
                .await
                .unwrap_err();
            assert!(
                err.downcast_ref::<academy_models::auth::AuthenticateError>()
                    .is_some()
                    || err
                        .downcast_ref::<academy_models::auth::AuthorizeError>()
                        .is_some()
            );
        }
    }
}

#[tokio::test]
async fn commercial_close_settlement_names_have_no_recipient_internal_or_body_alias() {
    for operation in CLOSED_SETTLEMENTS {
        let s = sut(&[]);
        let err = s
            .commercial_recipient(
                CommercialCredentials {
                    claim_key: Some("k".repeat(43)),
                    recipient: RecipientCredentials {
                        ordinary: None,
                        capability: None,
                    },
                },
                operation,
                json!({"command_id":UUID1,"operation":"export"}),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err.downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Malformed)
        ));
        let mut s = sut(&[]);
        s.internal
            .expect_authenticate()
            .once()
            .return_once(|token, audience| {
                assert_eq!(token.as_str(), "captured-internal");
                assert_eq!(audience, "shop");
                Ok(())
            });
        let err = s
            .commercial_internal(
                &"captured-internal".into(),
                operation,
                json!({"command_id":UUID1,"operation":"inventory"}),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err.downcast_ref::<RecipientAccessError>(),
            Some(RecipientAccessError::Malformed)
        ));
        for alias in [
            format!("{operation} "),
            operation.to_uppercase(),
            format!("admin_{operation}"),
        ] {
            let mut s = sut(&[]);
            staff_auth(&mut s, Ok(staff()));
            let err = s
                .commercial_admin(
                    &"captured-staff".into(),
                    &alias,
                    json!({"operation":operation,"action":operation,"command_id":UUID1}),
                )
                .await
                .unwrap_err();
            assert!(matches!(
                err.downcast_ref::<RecipientAccessError>(),
                Some(RecipientAccessError::Malformed)
            ));
        }
    }
}

#[tokio::test]
async fn commercial_close_supported_staff_dispatch_keeps_literal_path_and_original_body() {
    for operation in [
        "queue",
        "retention_queue",
        "statement_review",
        "archive_review",
        "review",
        "minimize_contact",
        "release_document",
        "release_record",
    ] {
        let mut s = sut(&[true]);
        staff_auth(&mut s, Ok(staff()));
        let body =
            json!({"command_id":UUID1,"operation":"reserve","action":"outcome","original":"exact"});
        let mut expected = body.clone();
        expected["_staff_session"] = json!(UUID1);
        expected["_staff_refresh_hash"] = json!(SHA256HASH1.to_string());
        s.repo
            .expect_commercial_operation()
            .once()
            .return_once(move |_, op, actor, body| {
                assert_eq!(op, operation);
                assert_eq!(actor, Some(FOO.user.id));
                assert_eq!(body, &expected);
                Box::pin(async { Ok(json!({"preserved":true})) })
            });
        assert_eq!(
            s.commercial_admin(&"captured-staff".into(), operation, body)
                .await
                .unwrap(),
            json!({"preserved":true})
        );
    }
    let mut s = sut(&[true]);
    staff_auth(&mut s, Ok(staff()));
    s.repo
        .expect_commercial_operation()
        .once()
        .return_once(|_, op, actor, body| {
            assert_eq!(op, "export");
            assert_eq!(actor, Some(original()));
            assert_eq!(body, &json!({}));
            Box::pin(async { Ok(json!({"history":"original"})) })
        });
    assert_eq!(
        s.commercial_admin(
            &"captured-staff".into(),
            "detail",
            json!({"subject":original(),"operation":"reserve"})
        )
        .await
        .unwrap(),
        json!({"history":"original"})
    );
}
