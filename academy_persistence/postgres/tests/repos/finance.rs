use academy_demo::user::{BAR, FOO};
use academy_models::{
    finance::{FinancialDocument, FinancialDocumentKind, RETENTION_MARKER},
    paypal::PaypalCoinOrder,
};
use academy_persistence_contracts::{
    Database, Transaction, finance::FinancialDocumentRepository, paypal::PaypalRepository,
    user::UserRepository,
};
use academy_persistence_postgres::{
    MIGRATIONS, finance::PostgresFinancialDocumentRepository, paypal::PostgresPaypalRepository,
    user::PostgresUserRepository,
};
use chrono::{DateTime, TimeZone, Utc};

use crate::{common::setup, repos::make_slice};

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
        settled_at: None,
        withdrawal_consent_at: None,
        withdrawal_text_version: None,
    }
}

fn final_statement(number: &str, issued_at: DateTime<Utc>) -> FinancialDocument {
    FinancialDocument {
        number: number.try_into().unwrap(),
        kind: FinancialDocumentKind::FinalStatement,
        user_id: Some(FOO.user.id),
        issued_at,
        customer_details: Some(vec!["Foo 42".into(), "foo@example.com".into()]),
        coins: Some(500),
        net_total_cents: None,
        vat_total_cents: None,
        gross_total_cents: Some(500),
        settled_at: None,
        withdrawal_consent_at: None,
        withdrawal_text_version: None,
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

/// The claim a final statement records is refunded by hand, so it is closed
/// out by hand: `settle` stamps the record and a repeated `record` does not
/// clear the stamp.
#[tokio::test]
async fn settle() {
    let db = setup().await;
    let document = final_statement("S42", date(2024, 3, 14));
    let settled_at = date(2026, 9, 6);

    let mut txn = db.begin_transaction().await.unwrap();
    assert!(
        !REPO
            .settle(&mut txn, &document.number, settled_at)
            .await
            .unwrap(),
        "a document that does not exist cannot be settled"
    );

    REPO.record(&mut txn, &document).await.unwrap();
    assert!(
        REPO.get(&mut txn, &document.number)
            .await
            .unwrap()
            .unwrap()
            .settled_at
            .is_none()
    );

    assert!(
        REPO.settle(&mut txn, &document.number, settled_at)
            .await
            .unwrap()
    );
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap().unwrap(),
        FinancialDocument {
            settled_at: Some(settled_at),
            ..document.clone()
        }
    );

    // Re-rendering the pdf records the document again, which must not reopen
    // a claim that has been paid out.
    REPO.record(&mut txn, &document).await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number)
            .await
            .unwrap()
            .unwrap()
            .settled_at,
        Some(settled_at)
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
            settled_at: None,
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
            settled_at: None,
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

/// The final statement is the one document that keeps its customer details
/// after the deletion, because a refund can only be offered to somebody it
/// still names.
#[tokio::test]
async fn pseudonymize_keeps_the_final_statement() {
    let db = setup().await;
    let document = invoice("R0000042", date(2024, 3, 14));
    let statement = final_statement("S7", date(2024, 4, 1));

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(&mut txn, &document).await.unwrap();
    REPO.record(&mut txn, &statement).await.unwrap();

    assert_eq!(
        REPO.pseudonymize(&mut txn, FOO.user.id, &[RETENTION_MARKER.into()])
            .await
            .unwrap(),
        1,
        "only the invoice is pseudonymized"
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
        REPO.get(&mut txn, &statement.number)
            .await
            .unwrap()
            .unwrap(),
        FinancialDocument {
            user_id: None,
            ..statement
        }
    );
}

/// The rows the migration backfilled carry no address block at all. They must
/// not make the search fail, and once the pdf has been rendered again the
/// address block it was printed with is searchable like any other.
#[tokio::test]
async fn search_tolerates_a_document_without_customer_details() {
    let db = setup().await;

    let backfilled = |number: &str, issued_at| FinancialDocument {
        customer_details: None,
        net_total_cents: None,
        vat_total_cents: None,
        ..invoice(number, issued_at)
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(&mut txn, &backfilled("R0000042", date(2024, 3, 14)))
        .await
        .unwrap();
    REPO.record(&mut txn, &backfilled("R0000043", date(2024, 4, 1)))
        .await
        .unwrap();

    // Nothing carries an address block yet, and the query still answers.
    assert_eq!(
        REPO.count(&mut txn, None, Some("foo@example.com".into()))
            .await
            .unwrap(),
        0
    );
    // Both are still found by their number.
    assert_eq!(
        REPO.count(&mut txn, None, Some("R00000".into()))
            .await
            .unwrap(),
        2
    );

    // Rendering the pdf again records the address block it is printed with,
    // and from then on the document is found by the email address on it.
    let rendered = invoice("R0000042", date(2024, 3, 14));
    REPO.record(&mut txn, &rendered).await.unwrap();
    assert_eq!(
        REPO.list(
            &mut txn,
            None,
            Some("FOO@example.COM".into()),
            make_slice(10, 0)
        )
        .await
        .unwrap(),
        vec![rendered]
    );
}

/// The search term is what an administrator typed, so the characters `like`
/// gives a special meaning to have to be matched literally.
#[tokio::test]
async fn search_matches_wildcards_literally() {
    let db = setup().await;

    let plain = invoice("R0000042", date(2024, 3, 14));
    let percent = FinancialDocument {
        customer_details: Some(vec!["100% Rabatt GmbH".into(), "bar@example.com".into()]),
        user_id: Some(BAR.user.id),
        ..invoice("R0000043", date(2024, 4, 1))
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(&mut txn, &plain).await.unwrap();
    REPO.record(&mut txn, &percent).await.unwrap();

    // A wildcard matches nothing instead of everything.
    for search in ["_", "R_000042", "\\", "%Rabatt%", "%%"] {
        assert_eq!(
            REPO.count(&mut txn, None, Some(search.into()))
                .await
                .unwrap(),
            0,
            "{search:?} was treated as a pattern"
        );
    }

    // A percent sign matches the one document that really contains one.
    assert_eq!(
        REPO.list(&mut txn, None, Some("%".into()), make_slice(10, 0))
            .await
            .unwrap(),
        vec![percent.clone()]
    );
    assert_eq!(
        REPO.list(&mut txn, None, Some("100%".into()), make_slice(10, 0))
            .await
            .unwrap(),
        vec![percent]
    );
}

/// The admin listing has to tolerate the documents of deleted accounts, which
/// have no `user_id`.
#[tokio::test]
async fn list_and_count() {
    let db = setup().await;
    let document = invoice("R0000042", date(2024, 3, 14));
    let statement = final_statement("S7", date(2024, 4, 1));
    let other = FinancialDocument {
        user_id: Some(BAR.user.id),
        customer_details: Some(vec!["Bar 42".into(), "bar@example.com".into()]),
        ..invoice("R0000043", date(2024, 5, 1))
    };

    let mut txn = db.begin_transaction().await.unwrap();
    for record in [&document, &statement, &other] {
        REPO.record(&mut txn, record).await.unwrap();
    }
    assert!(
        PostgresUserRepository
            .delete(&mut txn, FOO.user.id)
            .await
            .unwrap()
    );
    txn.commit().await.unwrap();

    let deleted_document = FinancialDocument {
        user_id: None,
        ..document
    };
    let deleted_statement = FinancialDocument {
        user_id: None,
        ..statement
    };

    let mut txn = db.begin_transaction().await.unwrap();

    // Newest first.
    assert_eq!(
        REPO.list(&mut txn, None, None, make_slice(10, 0))
            .await
            .unwrap(),
        vec![
            other.clone(),
            deleted_statement.clone(),
            deleted_document.clone()
        ]
    );
    assert_eq!(REPO.count(&mut txn, None, None).await.unwrap(), 3);

    assert_eq!(
        REPO.list(&mut txn, None, None, make_slice(1, 1))
            .await
            .unwrap(),
        vec![deleted_statement.clone()]
    );

    assert_eq!(
        REPO.list(
            &mut txn,
            Some(FinancialDocumentKind::FinalStatement),
            None,
            make_slice(10, 0)
        )
        .await
        .unwrap(),
        vec![deleted_statement.clone()]
    );
    assert_eq!(
        REPO.count(&mut txn, Some(FinancialDocumentKind::FinalStatement), None)
            .await
            .unwrap(),
        1
    );

    // The email address of a deleted account is still on its final statement,
    // so a later refund request can be matched to it.
    assert_eq!(
        REPO.list(
            &mut txn,
            None,
            Some("FOO@example.COM".into()),
            make_slice(10, 0)
        )
        .await
        .unwrap(),
        vec![deleted_statement, deleted_document.clone()]
    );
    assert_eq!(
        REPO.count(&mut txn, None, Some("foo@example.com".into()))
            .await
            .unwrap(),
        2
    );

    // The document number is searchable too.
    assert_eq!(
        REPO.list(&mut txn, None, Some("R0000042".into()), make_slice(10, 0))
            .await
            .unwrap(),
        vec![deleted_document]
    );

    assert_eq!(
        REPO.list_numbers(&mut txn).await.unwrap(),
        ["R0000042", "R0000043", "S7"]
            .into_iter()
            .map(|number| number.try_into().unwrap())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn list_by_user_id() {
    let db = setup().await;
    let document = invoice("R0000042", date(2024, 3, 14));
    let statement = final_statement("S7", date(2024, 4, 1));
    let other = FinancialDocument {
        user_id: Some(BAR.user.id),
        ..invoice("R0000043", date(2024, 5, 1))
    };

    let mut txn = db.begin_transaction().await.unwrap();
    for record in [&document, &statement, &other] {
        REPO.record(&mut txn, record).await.unwrap();
    }
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_by_user_id(&mut txn, FOO.user.id).await.unwrap(),
        vec![document, statement]
    );
    assert_eq!(
        REPO.list_by_user_id(&mut txn, BAR.user.id).await.unwrap(),
        vec![other]
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

/// The migration records one invoice per coin order that was actually paid.
/// An order that was never captured was never invoiced, so it gets no record.
#[tokio::test]
async fn migration_backfills_the_captured_coin_orders() {
    let db = crate::common::setup_through(Some(
        "2026-09-07-100000_add_withdrawal_consent_to_financial_documents",
    ))
    .await;

    let captured = PaypalCoinOrder {
        id: "captured".try_into().unwrap(),
        user_id: FOO.user.id,
        created_at: date(2024, 3, 14),
        captured_at: Some(date(2024, 3, 15)),
        coins: 1337,
        invoice_number: 42,
        withdrawal_consent_at: Some(date(2024, 3, 14)),
        withdrawal_text_version: Some("2026-09".try_into().unwrap()),
    };
    let open = PaypalCoinOrder {
        id: "open".try_into().unwrap(),
        captured_at: None,
        invoice_number: 43,
        ..captured.clone()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    PostgresPaypalRepository
        .create_coin_order(&mut txn, &captured)
        .await
        .unwrap();
    PostgresPaypalRepository
        .create_coin_order(&mut txn, &open)
        .await
        .unwrap();
    PostgresPaypalRepository
        .capture_coin_order(&mut txn, &captured.id, captured.captured_at.unwrap())
        .await
        .unwrap();
    txn.commit().await.unwrap();

    // Revert down to and including the migration that creates the table, then
    // apply it again so that the backfill runs against the coin orders.
    let index = MIGRATIONS
        .iter()
        .position(|migration| migration.name.ends_with("_create_financial_documents"))
        .unwrap();
    db.revert_migrations(Some(
        crate::repos::revert_through(&db, MIGRATIONS[index].name).await,
    ))
    .await
    .unwrap();
    crate::common::apply_through(
        &db,
        Some("2026-09-07-100000_add_withdrawal_consent_to_financial_documents"),
    )
    .await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_by_user_id(&mut txn, FOO.user.id).await.unwrap(),
        vec![FinancialDocument {
            number: "R0000042".try_into().unwrap(),
            kind: FinancialDocumentKind::Invoice,
            user_id: Some(FOO.user.id),
            issued_at: captured.created_at,
            customer_details: None,
            coins: Some(1337),
            net_total_cents: None,
            vat_total_cents: None,
            gross_total_cents: Some(1337),
            settled_at: None,
            // The declarations of the order are copied onto the record, so
            // that they survive the account the order belongs to.
            withdrawal_consent_at: captured.withdrawal_consent_at,
            withdrawal_text_version: captured.withdrawal_text_version.clone(),
        }]
    );
}

/// The declarations under § 356 Abs. 6 Nr. 2 BGB are recorded with the invoice
/// and are kept as long as it is, while the consent on the order and in
/// `withdrawal_consents` goes with the account.
#[tokio::test]
async fn withdrawal_consent_outlives_the_account() {
    let db = setup().await;

    let document = FinancialDocument {
        withdrawal_consent_at: Some(date(2024, 3, 14)),
        withdrawal_text_version: Some("2026-09".try_into().unwrap()),
        ..invoice("R0000042", date(2024, 3, 14))
    };

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.record(&mut txn, &document).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap(),
        Some(document.clone())
    );

    // Recording the same document again keeps what was recorded, exactly like
    // the other columns of an issued document.
    REPO.record(
        &mut txn,
        &FinancialDocument {
            withdrawal_consent_at: Some(date(2025, 1, 1)),
            withdrawal_text_version: Some("2027-01".try_into().unwrap()),
            ..document.clone()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap(),
        Some(document.clone())
    );
    txn.commit().await.unwrap();

    // Deleting the account drops the reference and the customer details, but
    // the declarations stay with the document.
    let mut txn = db.begin_transaction().await.unwrap();
    REPO.pseudonymize(&mut txn, FOO.user.id, &[RETENTION_MARKER.into()])
        .await
        .unwrap();
    PostgresUserRepository
        .delete(&mut txn, FOO.user.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, &document.number).await.unwrap(),
        Some(FinancialDocument {
            user_id: None,
            customer_details: Some(vec![RETENTION_MARKER.into()]),
            ..document
        })
    );
}
