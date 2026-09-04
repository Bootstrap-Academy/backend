use academy_demo::user::{BAR, FOO};
use academy_models::finance::{FinancialDocument, FinancialDocumentKind, RETENTION_MARKER};
use academy_persistence_contracts::{
    Database, Transaction, finance::FinancialDocumentRepository, user::UserRepository,
};
use academy_persistence_postgres::{
    finance::PostgresFinancialDocumentRepository, user::PostgresUserRepository,
};
use chrono::{DateTime, TimeZone, Utc};

use crate::common::setup;

const REPO: PostgresFinancialDocumentRepository = PostgresFinancialDocumentRepository;

fn invoice(number: &str, issued_at: DateTime<Utc>) -> FinancialDocument {
    FinancialDocument {
        number: number.try_into().unwrap(),
        kind: FinancialDocumentKind::Invoice,
        user_id: Some(FOO.user.id),
        issued_at,
        customer_details: Some(vec!["Foo 42".into(), "foo@example.com".into()]),
        coins: Some(1337),
        net_total_cents: Some(1124),
        vat_total_cents: Some(213),
        gross_total_cents: Some(1337),
    }
}

fn date(year: i32, month: u32, day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, 13, 37, 42).unwrap()
}

#[tokio::test]
async fn record_and_get() {
    let db = setup().await;
    let document = invoice("R0000042", date(2024, 3, 14));

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap(),
        None,
        "the document does not exist yet"
    );

    REPO.record(&mut txn, &document).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap().unwrap(),
        document
    );
}

/// An issued document must not change, so recording it again keeps the values
/// it was issued with.
#[tokio::test]
async fn record_keeps_the_values_a_document_was_issued_with() {
    let db = setup().await;
    let document = invoice("R0000042", date(2024, 3, 14));

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(&mut txn, &document).await.unwrap();

    REPO.record(
        &mut txn,
        &FinancialDocument {
            user_id: Some(BAR.user.id),
            customer_details: Some(vec!["Bar 42".into()]),
            coins: Some(1),
            net_total_cents: Some(1),
            vat_total_cents: Some(1),
            gross_total_cents: Some(1),
            ..document.clone()
        },
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap().unwrap(),
        document
    );
}

/// Values that have not been recorded yet are filled in, which is how the
/// documents that existed before this table was introduced obtain their
/// address block and amounts.
#[tokio::test]
async fn record_fills_in_missing_values() {
    let db = setup().await;
    let document = invoice("R0000042", date(2024, 3, 14));

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(
        &mut txn,
        &FinancialDocument {
            customer_details: None,
            coins: None,
            net_total_cents: None,
            vat_total_cents: None,
            gross_total_cents: None,
            ..document.clone()
        },
    )
    .await
    .unwrap();

    REPO.record(&mut txn, &document).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap().unwrap(),
        document
    );
}

/// Deleting an account must not delete its invoices and credit notes: the
/// record survives with the customer details replaced and without a reference
/// to the account.
#[tokio::test]
async fn pseudonymize_and_delete_the_user() {
    let db = setup().await;
    let document = invoice("R0000042", date(2024, 3, 14));
    let other = FinancialDocument {
        number: "R0000043".try_into().unwrap(),
        user_id: Some(BAR.user.id),
        ..invoice("R0000043", date(2024, 3, 15))
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(&mut txn, &document).await.unwrap();
    REPO.record(&mut txn, &other).await.unwrap();

    assert_eq!(
        REPO.pseudonymize(&mut txn, FOO.user.id, &[RETENTION_MARKER.into()])
            .await
            .unwrap(),
        1
    );
    assert!(
        PostgresUserRepository
            .delete(&mut txn, FOO.user.id)
            .await
            .unwrap()
    );
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap().unwrap(),
        FinancialDocument {
            user_id: None,
            customer_details: Some(vec![RETENTION_MARKER.into()]),
            ..document
        }
    );
    assert_eq!(
        REPO.get(&mut txn, &other.number).await.unwrap().unwrap(),
        other,
        "documents of other users are untouched"
    );
}

#[tokio::test]
async fn list_and_delete_issued_before() {
    let db = setup().await;
    let old = invoice("R0000001", date(2024, 12, 31));
    let new = FinancialDocument {
        number: "R0000002".try_into().unwrap(),
        ..invoice("R0000002", date(2025, 1, 1))
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(&mut txn, &old).await.unwrap();
    REPO.record(&mut txn, &new).await.unwrap();
    txn.commit().await.unwrap();

    // A document issued in 2024 may be deleted from the beginning of 2033.
    let cutoff = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_issued_before(&mut txn, cutoff).await.unwrap(),
        vec![old.clone()]
    );
    assert_eq!(
        REPO.delete_issued_before(&mut txn, cutoff).await.unwrap(),
        1
    );
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get(&mut txn, &old.number).await.unwrap(), None);
    assert_eq!(REPO.get(&mut txn, &new.number).await.unwrap().unwrap(), new);
    assert_eq!(
        REPO.list_issued_before(&mut txn, cutoff).await.unwrap(),
        Vec::new()
    );
}
