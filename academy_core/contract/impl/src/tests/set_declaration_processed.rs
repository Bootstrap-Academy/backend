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
        effective_end: Some(confirmed_end()),
        note: Some("Kündigung anerkannt, Ende bestätigt".try_into().unwrap()),
    }
}

#[tokio::test]
async fn ok() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);
    let time = MockTimeService::new().with_now(now());

    let expected = ContractDeclaration {
        effective_end: Some(confirmed_end()),
        processed_at: Some(now()),
        processing_note: Some("Kündigung anerkannt, Ende bestätigt".try_into().unwrap()),
        ..make_declaration()
    };

    let contract_repo = MockContractRepository::new()
        .with_get(make_declaration().id, Some(make_declaration()))
        .with_set_processed(
            make_declaration().id,
            now(),
            expected.effective_end,
            expected.processing_note.clone(),
            Some(expected.clone()),
        );

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
async fn ok_keeps_what_is_not_given() {
    // Arrange
    let stored = ContractDeclaration {
        cancellation_type: Some(ContractCancellationType::Ordinary),
        effective_end: Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()),
        ..make_declaration()
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);
    let time = MockTimeService::new().with_now(now());

    let expected = ContractDeclaration {
        processed_at: Some(now()),
        ..stored.clone()
    };

    let contract_repo = MockContractRepository::new()
        .with_get(stored.id, Some(stored.clone()))
        .with_set_processed(
            stored.id,
            now(),
            stored.effective_end,
            None,
            Some(expected.clone()),
        );

    let sut = ContractFeatureServiceImpl {
        auth,
        db,
        time,
        contract_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .set_declaration_processed(
            &"token".into(),
            stored.id,
            ContractDeclarationProcessingUpdate::default(),
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);
    let time = MockTimeService::new().with_now(now());

    let contract_repo = MockContractRepository::new().with_get(make_declaration().id, None);

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
