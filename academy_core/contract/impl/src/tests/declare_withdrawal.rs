use academy_core_contract_contracts::{
    ContractDeclarationResult, ContractDeclareError, ContractFeatureService,
    ContractWithdrawalRequest,
};
use academy_demo::{UUID2, user::FOO};
use academy_email_contracts::{MockEmailService, template::MockTemplateEmailService};
use academy_models::contract::{ContractDeclaration, ContractDeclarationKind, ContractKind};
use academy_persistence_contracts::{
    MockDatabase, contract::MockContractRepository, user::MockUserRepository,
};
use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
use academy_templates_contracts::ContractWithdrawalConfirmationTemplate;
use academy_utils::assert_matches;
use chrono::{DateTime, TimeZone, Utc};

use crate::{
    ContractFeatureServiceImpl,
    tests::{
        CLIENT_IP, RATE_LIMIT_COUNT, Sut, declarant_email, declarant_name, make_cache,
        make_exhausted_cache, make_hash, make_internal_email,
    },
};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap()
}

fn make_request() -> ContractWithdrawalRequest {
    ContractWithdrawalRequest {
        name: declarant_name(),
        email: declarant_email(),
        contract: ContractKind::Coins,
        details: "Bestellung vom 01.09.2026".try_into().unwrap(),
    }
}

fn make_declaration() -> ContractDeclaration {
    ContractDeclaration {
        id: UUID2.into(),
        kind: ContractDeclarationKind::Withdrawal,
        received_at: now(),
        name: declarant_name(),
        email: declarant_email(),
        user_id: Some(FOO.user.id),
        contract: ContractKind::Coins,
        cancellation_type: None,
        details: "Bestellung vom 01.09.2026".try_into().unwrap(),
        requested_end: None,
        effective_end: None,
        processed_at: None,
    }
}

#[tokio::test]
async fn ok() {
    // Arrange
    let declaration = make_declaration();

    let hash = make_hash(&declarant_email());
    let cache = make_cache(0, 0);
    let time = MockTimeService::new().with_now(now());
    let id = MockIdService::new().with_generate(declaration.id);
    let db = MockDatabase::build(true);

    let user_repo =
        MockUserRepository::new().with_get_composite_by_email(declarant_email(), Some(FOO.clone()));

    let contract_repo = MockContractRepository::new().with_create(declaration.clone());

    let template_email = MockTemplateEmailService::new()
        .with_send_contract_withdrawal_confirmation_email(
            declarant_email().with_name("Max Mustermann".into()),
            ContractWithdrawalConfirmationTemplate {
                received_at: "03.09.2026 um 14:00:00 Uhr".into(),
                name: "Max Mustermann".into(),
                email: "foo@example.com".into(),
                contract: "MorphCoins-Kauf".into(),
                details: Some("Bestellung vom 01.09.2026".into()),
            },
            Ok(true),
        );

    let email = MockEmailService::new().with_send(
        make_internal_email(&declaration, "[Contract] Widerruf (Coins)"),
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
    let result = sut.declare_withdrawal(CLIENT_IP, make_request()).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        ContractDeclarationResult {
            declaration,
            confirmation_email_sent: true
        }
    );
}

#[tokio::test]
async fn rate_limit() {
    // Arrange
    let hash = make_hash(&declarant_email());
    let cache = make_exhausted_cache(0, RATE_LIMIT_COUNT + 3);

    let sut = ContractFeatureServiceImpl {
        hash,
        cache,
        ..Sut::default()
    };

    // Act
    let result = sut.declare_withdrawal(CLIENT_IP, make_request()).await;

    // Assert
    assert_matches!(result, Err(ContractDeclareError::RateLimit));
}
