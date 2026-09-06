use academy_auth_contracts::MockAuthService;
use academy_core_finance_contracts::{
    FinanceFeatureService, FinanceListError, FinancialDocumentListQuery,
    FinancialDocumentListResult,
};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    finance::{FinancialDocument, FinancialDocumentKind, RETENTION_MARKER},
    pagination::PaginationSlice,
};
use academy_persistence_contracts::{MockDatabase, finance::MockFinancialDocumentRepository};
use academy_utils::assert_matches;
use chrono::{TimeZone, Utc};

use crate::{FinanceFeatureServiceImpl, tests::Sut};

fn make_query() -> FinancialDocumentListQuery {
    FinancialDocumentListQuery {
        kind: Some(FinancialDocumentKind::FinalStatement),
        search: Some("a@a".into()),
        pagination: PaginationSlice {
            limit: 42.try_into().unwrap(),
            offset: 7,
        },
    }
}

/// A document whose account has been deleted.
fn make_document() -> FinancialDocument {
    FinancialDocument {
        number: "R0000042".try_into().unwrap(),
        kind: FinancialDocumentKind::Invoice,
        user_id: None,
        issued_at: Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).unwrap(),
        customer_details: Some(vec![RETENTION_MARKER.into()]),
        coins: Some(1337),
        net_total_cents: Some(1123),
        vat_total_cents: Some(214),
        gross_total_cents: Some(1337),
    }
}

/// Documents of deleted accounts have no `user_id` and are listed like any
/// other document.
#[tokio::test]
async fn ok() {
    // Arrange
    let query = make_query();
    let expected = FinancialDocumentListResult {
        total: 17,
        documents: vec![make_document()],
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));
    let db = MockDatabase::build(false);
    let document_repo = MockFinancialDocumentRepository::new()
        .with_count(query.kind, query.search.clone(), expected.total)
        .with_list(
            query.kind,
            query.search.clone(),
            query.pagination,
            expected.documents.clone(),
        );

    let sut = FinanceFeatureServiceImpl {
        db,
        auth,
        document_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.list_documents(&"token".into(), query).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = FinanceFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list_documents(&"token".into(), make_query()).await;

    // Assert
    assert_matches!(
        result,
        Err(FinanceListError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn no_admin() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = FinanceFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.list_documents(&"token".into(), make_query()).await;

    // Assert
    assert_matches!(
        result,
        Err(FinanceListError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}
