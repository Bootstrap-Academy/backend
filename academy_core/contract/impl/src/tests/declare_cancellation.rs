use academy_core_contract_contracts::{
    ContractCancellationRequest, ContractDeclarationResult, ContractDeclareError,
    ContractFeatureService,
};
use academy_demo::{UUID1, user::FOO};
use academy_email_contracts::{MockEmailService, template::MockTemplateEmailService};
use academy_models::{
    contract::{
        ContractCancellationType, ContractDeclaration, ContractDeclarationKind, ContractKind,
    },
    premium::Premium,
    user::UserId,
};
use academy_persistence_contracts::{
    MockDatabase, contract::MockContractRepository, premium::MockPremiumRepository,
    user::MockUserRepository,
};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
use academy_templates_contracts::ContractCancellationConfirmationTemplate;
use academy_utils::assert_matches;
use chrono::{DateTime, TimeZone, Utc};

use crate::{
    ContractFeatureServiceImpl,
    tests::{
        CLIENT_IP, RATE_LIMIT_COUNT, Sut, declarant_email, declarant_name, make_cache,
        make_exhausted_cache, make_hash, make_internal_email, no_details, unknown_email,
    },
};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap()
}

fn requested_end() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 12, 31, 12, 0, 0).unwrap()
}

fn premium_until() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()
}

fn make_request() -> ContractCancellationRequest {
    ContractCancellationRequest {
        name: declarant_name(),
        email: declarant_email(),
        contract: ContractKind::Premium,
        cancellation_type: ContractCancellationType::Ordinary,
        details: "Zu teuer".try_into().unwrap(),
        requested_end: Some(requested_end()),
    }
}

fn make_declaration(
    user_id: Option<UserId>,
    effective_end: Option<DateTime<Utc>>,
) -> ContractDeclaration {
    ContractDeclaration {
        id: UUID1.into(),
        kind: ContractDeclarationKind::Cancellation,
        received_at: now(),
        name: declarant_name(),
        email: declarant_email(),
        user_id,
        contract: ContractKind::Premium,
        cancellation_type: Some(ContractCancellationType::Ordinary),
        details: "Zu teuer".try_into().unwrap(),
        requested_end: Some(requested_end()),
        effective_end,
        processed_at: None,
    }
}

fn make_template(effective_end: Option<&str>) -> ContractCancellationConfirmationTemplate {
    ContractCancellationConfirmationTemplate {
        received_at: "03.09.2026 um 14:00:00 Uhr".into(),
        name: "Max Mustermann".into(),
        email: "foo@example.com".into(),
        contract: "Premium-Mitgliedschaft".into(),
        cancellation_type: "ordentliche Kündigung".into(),
        details: Some("Zu teuer".into()),
        requested_end: Some("31.12.2026".into()),
        effective_end: effective_end.map(Into::into),
    }
}

/// The declarant has an account with an active premium membership: autopay is
/// switched off and the contract ends at the end of the paid period.
#[tokio::test]
async fn ok_premium_user() {
    // Arrange
    let declaration = make_declaration(Some(FOO.user.id), Some(premium_until()));

    let hash = make_hash(&declarant_email());
    let cache = make_cache(0, 0);
    let time = MockTimeService::new().with_now(now());
    let id = MockIdService::new().with_generate(declaration.id);
    let db = MockDatabase::build(true);

    let user_repo =
        MockUserRepository::new().with_get_composite_by_email(declarant_email(), Some(FOO.clone()));

    let premium_repo = MockPremiumRepository::new()
        .with_set_subscription(FOO.user.id, None)
        .with_get_latest_by_user_id(
            FOO.user.id,
            Some(Premium {
                id: UUID1.into(),
                user_id: FOO.user.id,
                since: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
                until: premium_until(),
            }),
        );

    let contract_repo = MockContractRepository::new().with_create(declaration.clone());

    let template_email = MockTemplateEmailService::new()
        .with_send_contract_cancellation_confirmation_email(
            declarant_email().with_name("Max Mustermann".into()),
            make_template(Some("01.10.2026")),
            Ok(true),
        );

    let email = MockEmailService::new().with_send(
        make_internal_email(&declaration, "[Contract] Kündigung (Premium)"),
        true,
    );

    let sut = ContractFeatureServiceImpl {
        hash,
        cache,
        time,
        id,
        db,
        user_repo,
        premium_repo,
        contract_repo,
        template_email,
        email,
        ..Sut::default()
    };

    // Act
    let result = sut.declare_cancellation(CLIENT_IP, make_request()).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        ContractDeclarationResult {
            declaration,
            confirmation_email_sent: true
        }
    );
}

/// The declarant has an account but no premium membership: the contract ends
/// immediately.
#[tokio::test]
async fn ok_user_without_premium() {
    // Arrange
    let declaration = make_declaration(Some(FOO.user.id), Some(now()));

    let hash = make_hash(&declarant_email());
    let cache = make_cache(0, 0);
    let time = MockTimeService::new().with_now(now());
    let id = MockIdService::new().with_generate(declaration.id);
    let db = MockDatabase::build(true);

    let user_repo =
        MockUserRepository::new().with_get_composite_by_email(declarant_email(), Some(FOO.clone()));

    let premium_repo = MockPremiumRepository::new()
        .with_set_subscription(FOO.user.id, None)
        .with_get_latest_by_user_id(FOO.user.id, None);

    let contract_repo = MockContractRepository::new().with_create(declaration.clone());

    let template_email = MockTemplateEmailService::new()
        .with_send_contract_cancellation_confirmation_email(
            declarant_email().with_name("Max Mustermann".into()),
            make_template(Some("03.09.2026")),
            Ok(true),
        );

    let email = MockEmailService::new().with_send(
        make_internal_email(&declaration, "[Contract] Kündigung (Premium)"),
        true,
    );

    let sut = ContractFeatureServiceImpl {
        hash,
        cache,
        time,
        id,
        db,
        user_repo,
        premium_repo,
        contract_repo,
        template_email,
        email,
        ..Sut::default()
    };

    // Act
    let result = sut.declare_cancellation(CLIENT_IP, make_request()).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        ContractDeclarationResult {
            declaration,
            confirmation_email_sent: true
        }
    );
}

/// No account matches the declarant's email address: the declaration is stored
/// and confirmed anyway.
#[tokio::test]
async fn ok_unknown_email() {
    // Arrange
    let declaration = ContractDeclaration {
        email: unknown_email(),
        details: no_details(),
        ..make_declaration(None, None)
    };

    let hash = make_hash(&unknown_email());
    let cache = make_cache(0, 0);
    let time = MockTimeService::new().with_now(now());
    let id = MockIdService::new().with_generate(declaration.id);
    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_get_composite_by_email(unknown_email(), None);

    let contract_repo = MockContractRepository::new().with_create(declaration.clone());

    let template_email = MockTemplateEmailService::new()
        .with_send_contract_cancellation_confirmation_email(
            unknown_email().with_name("Max Mustermann".into()),
            ContractCancellationConfirmationTemplate {
                email: "nobody@example.com".into(),
                details: None,
                ..make_template(None)
            },
            Ok(true),
        );

    let email = MockEmailService::new().with_send(
        make_internal_email(&declaration, "[Contract] Kündigung (Premium)"),
        true,
    );

    let sut = ContractFeatureServiceImpl {
        hash,
        cache,
        time,
        id,
        db,
        user_repo,
        contract_repo,
        template_email,
        email,
        ..Sut::default()
    };

    // Act
    let result = sut
        .declare_cancellation(
            CLIENT_IP,
            ContractCancellationRequest {
                email: unknown_email(),
                details: no_details(),
                ..make_request()
            },
        )
        .await;

    // Assert
    assert_eq!(
        result.unwrap(),
        ContractDeclarationResult {
            declaration,
            confirmation_email_sent: true
        }
    );
}

/// The confirmation email cannot be sent: the declaration is stored anyway and
/// the caller is told that no confirmation was sent.
#[tokio::test]
async fn ok_confirmation_email_failed() {
    // Arrange
    let declaration = make_declaration(Some(FOO.user.id), Some(premium_until()));

    let hash = make_hash(&declarant_email());
    let cache = make_cache(0, 0);
    let time = MockTimeService::new().with_now(now());
    let id = MockIdService::new().with_generate(declaration.id);
    let db = MockDatabase::build(true);

    let user_repo =
        MockUserRepository::new().with_get_composite_by_email(declarant_email(), Some(FOO.clone()));

    let premium_repo = MockPremiumRepository::new()
        .with_set_subscription(FOO.user.id, None)
        .with_get_latest_by_user_id(
            FOO.user.id,
            Some(Premium {
                id: UUID1.into(),
                user_id: FOO.user.id,
                since: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
                until: premium_until(),
            }),
        );

    let contract_repo = MockContractRepository::new().with_create(declaration.clone());

    let template_email = MockTemplateEmailService::new()
        .with_send_contract_cancellation_confirmation_email(
            declarant_email().with_name("Max Mustermann".into()),
            make_template(Some("01.10.2026")),
            Err(anyhow::anyhow!("smtp is down")),
        );

    let email = MockEmailService::new().with_send(
        make_internal_email(&declaration, "[Contract] Kündigung (Premium)"),
        true,
    );

    let sut = ContractFeatureServiceImpl {
        hash,
        cache,
        time,
        id,
        db,
        user_repo,
        premium_repo,
        contract_repo,
        template_email,
        email,
        ..Sut::default()
    };

    // Act
    let result = sut.declare_cancellation(CLIENT_IP, make_request()).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        ContractDeclarationResult {
            declaration,
            confirmation_email_sent: false
        }
    );
}

/// The rate limit has been exhausted: nothing is stored and no email is sent.
#[tokio::test]
async fn rate_limit() {
    // Arrange
    let hash = make_hash(&declarant_email());
    let cache = make_exhausted_cache(RATE_LIMIT_COUNT, 0);

    let sut = ContractFeatureServiceImpl {
        hash,
        cache,
        ..Sut::default()
    };

    // Act
    let result = sut.declare_cancellation(CLIENT_IP, make_request()).await;

    // Assert
    assert_matches!(result, Err(ContractDeclareError::RateLimit));
}

/// The rate limit has been exhausted for the email address only.
#[tokio::test]
async fn rate_limit_email() {
    // Arrange
    let hash = make_hash(&declarant_email());
    let cache = make_exhausted_cache(0, RATE_LIMIT_COUNT);

    let sut = ContractFeatureServiceImpl {
        hash,
        cache,
        ..Sut::default()
    };

    // Act
    let result = sut.declare_cancellation(CLIENT_IP, make_request()).await;

    // Assert
    assert_matches!(result, Err(ContractDeclareError::RateLimit));
}

#[test]
fn internal_notification_body_text() {
    let body = crate::internal_notification_body(&make_declaration(
        Some(FOO.user.id),
        Some(premium_until()),
    ));

    assert_eq!(
        body,
        format!(
            "Art der Erklärung: Kündigung\n\
             Eingegangen am: 03.09.2026 um 14:00:00 Uhr\n\
             Name: Max Mustermann\n\
             E-Mail-Adresse: foo@example.com\n\
             Konto: {}\n\
             Vertrag: Premium-Mitgliedschaft\n\
             Art der Kündigung: ordentliche Kündigung\n\
             Begründung/Angaben: Zu teuer\n\
             Gewünschter Beendigungszeitpunkt: 31.12.2026\n\
             Beendigungszeitpunkt: 01.10.2026\n\
             ID der Erklärung: {}\n",
            *FOO.user.id, UUID1
        )
    );
}
