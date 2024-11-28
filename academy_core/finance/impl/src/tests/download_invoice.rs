use academy_core_finance_contracts::{
    invoice::MockFinanceInvoiceService, FinanceDownloadInvoiceError, FinanceFeatureService,
};
use academy_demo::user::FOO;
use academy_persistence_contracts::MockDatabase;
use academy_shared_contracts::jwt::{MockJwtService, VerifyJwtError};
use academy_utils::assert_matches;

use crate::{tests::Sut, DownloadToken, FinanceFeatureServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = vec![1, 2, 3, 4];

    let jwt = MockJwtService::new().with_verify(
        "the-jwt".into(),
        Ok(DownloadToken {
            sub: FOO.user.id,
            aud: Default::default(),
        }),
    );

    let db = MockDatabase::build(false);

    let finance_invoice = MockFinanceInvoiceService::new().with_get_invoice_pdf(
        Some(FOO.user.id),
        42,
        Some(expected.clone()),
    );

    let sut = FinanceFeatureServiceImpl {
        jwt,
        db,
        finance_invoice,
        ..Sut::default()
    };

    // Act
    let result = sut.download_invoice("the-jwt", 42).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn invalid_token() {
    // Arrange
    let jwt = MockJwtService::new()
        .with_verify::<DownloadToken>("the-jwt".into(), Err(VerifyJwtError::Invalid));

    let sut = FinanceFeatureServiceImpl {
        jwt,
        ..Sut::default()
    };

    // Act
    let result = sut.download_invoice("the-jwt", 42).await;

    // Assert
    assert_matches!(result, Err(FinanceDownloadInvoiceError::InvalidToken));
}

#[tokio::test]
async fn not_found() {
    // Arrange
    let jwt = MockJwtService::new().with_verify(
        "the-jwt".into(),
        Ok(DownloadToken {
            sub: FOO.user.id,
            aud: Default::default(),
        }),
    );

    let db = MockDatabase::build(false);

    let finance_invoice =
        MockFinanceInvoiceService::new().with_get_invoice_pdf(Some(FOO.user.id), 42, None);

    let sut = FinanceFeatureServiceImpl {
        jwt,
        db,
        finance_invoice,
        ..Sut::default()
    };

    // Act
    let result = sut.download_invoice("the-jwt", 42).await;

    // Assert
    assert_matches!(result, Err(FinanceDownloadInvoiceError::NotFound));
}
