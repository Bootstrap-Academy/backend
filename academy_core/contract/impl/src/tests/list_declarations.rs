use academy_auth_contracts::MockAuthService;
use academy_core_contract_contracts::{
    ContractDeclarationListQuery, ContractDeclarationListResult, ContractFeatureService,
    ContractListError,
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
    pagination::PaginationSlice,
};
use academy_persistence_contracts::{MockDatabase, contract::MockContractRepository};
use academy_utils::assert_matches;
use chrono::{TimeZone, Utc};

use crate::{
    ContractFeatureServiceImpl,
    tests::{Sut, declarant_email, declarant_name},
};

fn make_query() -> ContractDeclarationListQuery {
    ContractDeclarationListQuery {
        kind: Some(ContractDeclarationKind::Cancellation),
        pagination: PaginationSlice {
            limit: 42.try_into().unwrap(),
            offset: 7,
        },
    }
}

fn make_declaration() -> ContractDeclaration {
    ContractDeclaration {
        id: UUID1.into(),
        kind: ContractDeclarationKind::Cancellation,
        received_at: Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap(),
        name: declarant_name(),
        email: declarant_email(),
        user_id: Some(FOO.user.id),
        contract: ContractKind::Premium,
        cancellation_type: Some(ContractCancellationType::Ordinary),
        details: "Zu teuer".try_into().unwrap(),
        requested_end: None,
        effective_end: None,
        processed_at: None,
    }
}

#[tokio::test]
async fn ok() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let declaration = make_declaration();

    let contract_repo = MockContractRepository::new()
        .with_count(make_query().kind, 3)
        .with_list(
            make_query().kind,
            make_query().pagination,
            vec![declaration.clone()],
        );

    let sut = ContractFeatureServiceImpl {
        auth,
        db,
        contract_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.list_declarations(&"token".into(), make_query()).await;

    // Assert
    assert_eq!(
        result.unwrap(),
        ContractDeclarationListResult {
            total: 3,
            declarations: vec![declaration]
        }
    );
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
    let result = sut.list_declarations(&"token".into(), make_query()).await;

    // Assert
    assert_matches!(
        result,
        Err(ContractListError::Auth(AuthError::Authenticate(
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
    let result = sut.list_declarations(&"token".into(), make_query()).await;

    // Assert
    assert_matches!(
        result,
        Err(ContractListError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}
