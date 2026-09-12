use super::*;
use crate::{receipt_body, same_requested_agreement, same_submission};
use academy_core_contract_contracts::{ContractDeclareError, ContractFeatureService};
use academy_demo::UUID1;
use academy_models::contract::*;
use chrono::{TimeZone, Utc};
fn declaration() -> ContractDeclaration {
    ContractDeclaration {
        id: UUID1.into(),
        kind: ContractDeclarationKind::Cancellation,
        received_at: Utc.with_ymd_and_hms(2026, 9, 7, 12, 0, 0).unwrap(),
        name: declarant_name(),
        email: declarant_email(),
        user_id: Some(FOO.user.id),
        contract: ContractKind::Premium,
        contract_designation: Some("Meine Mitgliedschaft".try_into().unwrap()),
        cancellation_type: Some(ContractCancellationType::Ordinary),
        details: no_details(),
        requested_end: Some(Utc.with_ymd_and_hms(2026, 12, 31, 0, 0, 0).unwrap()),
        effective_end: None,
        processed_at: None,
        processing_note: None,
        delivery: vec![],
        operational_evidence: None,
    }
}
#[test]
fn receipt_contains_only_submitted_facts_and_original_time() {
    let mut d = declaration();
    let original = receipt_body(&d);
    d.user_id = None;
    d.effective_end = Some(Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap());
    d.processing_note = Some("INTERNAL ACCOUNT FACT".try_into().unwrap());
    assert_eq!(receipt_body(&d), original);
    assert!(original.contains("31.12.2026"));
    assert!(original.contains("07.09.2026 um 14:00:00"));
    assert!(!original.contains("INTERNAL"));
    assert!(original.starts_with("Hallo,\n\ndeine Kündigung ist am"));
    assert!(original.contains("Meine Mitgliedschaft"));
    assert!(!original.contains("Bestätigtes Vertragsende"));
    assert!(!original.contains("Ihrer"));
    assert!(original.find(&UUID1.to_string()).unwrap() > original.find("Vertrag:").unwrap());
    d.kind = ContractDeclarationKind::Withdrawal;
    d.requested_end = None;
    let withdrawal = receipt_body(&d);
    assert!(withdrawal.starts_with("Hallo,\n\ndein Widerruf ist am"));
    assert!(!withdrawal.contains("Art der Kündigung"));
    assert!(!withdrawal.contains("Gewünschtes Vertragsende"));
}
#[test]
fn duplicate_equality_ignores_private_processing_but_preserves_every_submission_field() {
    let a = declaration();
    let mut b = a.clone();
    b.effective_end = Some(a.received_at);
    b.user_id = None;
    b.processed_at = Some(a.received_at);
    assert!(same_submission(&a, &b));
    b.requested_end = None;
    assert!(!same_submission(&a, &b));
    b = a.clone();
    b.details = "Anderer Inhalt".try_into().unwrap();
    assert!(!same_submission(&a, &b));
}
#[tokio::test]
async fn rate_limits_distinguish_ip_and_email_budget() {
    for (ip, email) in [(RATE_LIMIT_PER_IP, 0), (0, RATE_LIMIT_PER_EMAIL)] {
        let sut = Sut {
            cache: make_exhausted_cache(ip, email),
            hash: make_hash(&declarant_email()),
            ..Sut::default()
        };
        assert!(matches!(
            sut.check_rate_limit(CLIENT_IP, &declarant_email()).await,
            Err(ContractDeclareError::RateLimit)
        ));
    }
    let sut = Sut {
        cache: make_cache(7, 0),
        hash: make_hash(&declarant_email()),
        ..Sut::default()
    };
    sut.check_rate_limit(CLIENT_IP, &declarant_email())
        .await
        .unwrap();
}
#[tokio::test]
async fn receipt_lookup_requires_the_separate_secret() {
    let key = ContractRequestKey {
        id: UUID1.into(),
        secret: academy_demo::UUID2.into(),
    };
    let mut repo = MockContractRepository::new();
    repo.expect_receipt_access()
        .once()
        .return_once(|_, _| Box::pin(async { Ok(Some("another hash".into())) }));
    let hash = MockHashService::new().with_sha256((*key.secret).to_string(), *SHA256HASH1);
    let sut = Sut {
        db: MockDatabase::build(false),
        contract_repo: repo,
        hash,
        ..Sut::default()
    };
    assert!(matches!(
        sut.lookup_receipt(key).await,
        Err(ContractDeclareError::NotFound)
    ));
}

#[test]
fn changing_an_optional_reference_cannot_reuse_the_same_request() {
    let mut d = declaration();
    let id = academy_demo::UUID1;
    d.operational_evidence = Some(
        serde_json::json!({"messages":[{"kind":"receipt","requested_agreement_id":id}]})
            .to_string(),
    );
    assert!(same_requested_agreement(&d, Some(id.into())));
    assert!(!same_requested_agreement(&d, None));
    assert!(!same_requested_agreement(
        &d,
        Some(academy_demo::UUID2.into())
    ));
}
