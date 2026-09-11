use crate::tests::Sut;
use academy_core_finance_contracts::{FinanceDownloadError, FinanceFeatureService};
use academy_demo::user::FOO;
use academy_models::finance::{FinancialDocumentKind, FinancialDocumentNumber};
use academy_persistence_contracts::MockDatabase;

#[tokio::test]
async fn unavailable_owner_never_reads_an_archive_or_issues_a_document() {
    let mut sut = Sut {
        db: MockDatabase::build(false),
        ..Sut::default()
    };
    sut.document_repo
        .expect_owned_original_number()
        .once()
        .return_once(|_, user, kind, number, month| {
            assert_eq!(
                (user, kind, number, month),
                (FOO.user.id, FinancialDocumentKind::Invoice, 42, 0)
            );
            Box::pin(async { Ok(None) })
        });
    assert!(matches!(
        sut.download_recipient_original(FOO.user.id, FinancialDocumentKind::Invoice, 42, 0)
            .await,
        Err(FinanceDownloadError::NotFound)
    ));
}

#[tokio::test]
async fn original_lookup_is_read_only_and_preserves_missing_bytes() {
    for pdf in [None, Some(b"%PDF-retained original".to_vec())] {
        let mut sut = Sut {
            db: MockDatabase::build(false),
            ..Sut::default()
        };
        sut.document_repo
            .expect_owned_original_number()
            .once()
            .return_once(|_, _, _, _, _| {
                Box::pin(async { Ok(Some(FinancialDocumentNumber::try_new("G202608-7").unwrap())) })
            });
        let original = pdf.clone();
        sut.finance_invoice
            .expect_get_original_pdf()
            .once()
            .return_once(|_, number, kind| {
                assert_eq!(number.as_str(), "G202608-7");
                assert_eq!(kind, FinancialDocumentKind::CreditNote);
                Box::pin(async move { Ok(original) })
            });
        let result = sut
            .download_recipient_original(FOO.user.id, FinancialDocumentKind::CreditNote, 2026, 8)
            .await;
        match pdf {
            Some(expected) => assert_eq!(result.unwrap(), expected),
            None => assert!(matches!(result, Err(FinanceDownloadError::NotFound))),
        }
    }
}
