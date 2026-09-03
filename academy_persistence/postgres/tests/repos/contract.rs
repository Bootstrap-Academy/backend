use academy_demo::{
    UUID1, UUID2,
    user::{BAR, FOO},
};
use academy_models::contract::{
    ContractCancellationType, ContractDeclaration, ContractDeclarationKind, ContractKind,
};
use academy_persistence_contracts::{Database, Transaction, contract::ContractRepository};
use academy_persistence_postgres::contract::PostgresContractRepository;
use chrono::{TimeZone, Utc};
use uuid::uuid;

use crate::{
    common::setup,
    repos::{make_slice, sliced},
};

const REPO: PostgresContractRepository = PostgresContractRepository;

/// A declaration with all optional columns set.
fn cancellation() -> ContractDeclaration {
    ContractDeclaration {
        id: UUID1.into(),
        kind: ContractDeclarationKind::Cancellation,
        received_at: Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap(),
        name: "Max Mustermann".try_into().unwrap(),
        email: "max.mustermann@example.de".parse().unwrap(),
        user_id: Some(FOO.user.id),
        contract: ContractKind::Premium,
        cancellation_type: Some(ContractCancellationType::Extraordinary),
        details: "Zu teuer".try_into().unwrap(),
        requested_end: Some(Utc.with_ymd_and_hms(2026, 12, 31, 23, 0, 0).unwrap()),
        effective_end: Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()),
        processed_at: Some(Utc.with_ymd_and_hms(2026, 9, 4, 8, 0, 0).unwrap()),
    }
}

/// A declaration with all optional columns unset.
fn withdrawal() -> ContractDeclaration {
    ContractDeclaration {
        id: UUID2.into(),
        kind: ContractDeclarationKind::Withdrawal,
        received_at: Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap(),
        name: "Erika Mustermann".try_into().unwrap(),
        email: "erika@example.de".parse().unwrap(),
        user_id: None,
        contract: ContractKind::Coins,
        cancellation_type: None,
        details: Default::default(),
        requested_end: None,
        effective_end: None,
        processed_at: None,
    }
}

fn other_cancellation() -> ContractDeclaration {
    ContractDeclaration {
        id: uuid!("b3a2eb0e-7a35-4c2e-9ee6-6cf4a7a3f5b1").into(),
        kind: ContractDeclarationKind::Cancellation,
        received_at: Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap(),
        name: "John Doe".try_into().unwrap(),
        email: "john@example.de".parse().unwrap(),
        user_id: None,
        contract: ContractKind::Other,
        cancellation_type: Some(ContractCancellationType::Ordinary),
        details: "Kein Interesse mehr".try_into().unwrap(),
        requested_end: None,
        effective_end: None,
        processed_at: None,
    }
}

#[tokio::test]
async fn create_list_count() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.count(&mut txn, None).await.unwrap(), 0);
    assert_eq!(
        REPO.list(&mut txn, None, make_slice(100, 0)).await.unwrap(),
        []
    );

    REPO.create(&mut txn, cancellation()).await.unwrap();
    REPO.create(&mut txn, withdrawal()).await.unwrap();
    REPO.create(&mut txn, other_cancellation()).await.unwrap();
    txn.commit().await.unwrap();

    // most recent first, all optional columns round-trip
    let expected = [cancellation(), withdrawal(), other_cancellation()];

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.count(&mut txn, None).await.unwrap(), 3);
    assert_eq!(
        REPO.list(&mut txn, None, make_slice(100, 0)).await.unwrap(),
        expected
    );
}

#[tokio::test]
async fn filter_by_kind() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, cancellation()).await.unwrap();
    REPO.create(&mut txn, withdrawal()).await.unwrap();
    REPO.create(&mut txn, other_cancellation()).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();

    let kind = Some(ContractDeclarationKind::Cancellation);
    assert_eq!(REPO.count(&mut txn, kind).await.unwrap(), 2);
    assert_eq!(
        REPO.list(&mut txn, kind, make_slice(100, 0)).await.unwrap(),
        [cancellation(), other_cancellation()]
    );

    let kind = Some(ContractDeclarationKind::Withdrawal);
    assert_eq!(REPO.count(&mut txn, kind).await.unwrap(), 1);
    assert_eq!(
        REPO.list(&mut txn, kind, make_slice(100, 0)).await.unwrap(),
        [withdrawal()]
    );
}

#[tokio::test]
async fn pagination() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, cancellation()).await.unwrap();
    REPO.create(&mut txn, withdrawal()).await.unwrap();
    REPO.create(&mut txn, other_cancellation()).await.unwrap();
    txn.commit().await.unwrap();

    let expected = &[cancellation(), withdrawal(), other_cancellation()];

    let mut txn = db.begin_transaction().await.unwrap();
    for slice in [
        make_slice(100, 0),
        make_slice(2, 0),
        make_slice(2, 1),
        make_slice(100, 1),
        make_slice(1, 2),
        make_slice(100, 17),
    ] {
        let result = REPO.list(&mut txn, None, slice).await.unwrap();
        assert_eq!(result, sliced(expected, slice));
    }
}

/// The export of a user contains the declarations of that user, oldest first.
#[tokio::test]
async fn list_by_user_id() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_by_user_id(&mut txn, FOO.user.id).await.unwrap(),
        []
    );

    let second = ContractDeclaration {
        id: uuid!("3f5d59fa-8f3b-4e46-9d51-4f3c6b0d0a7e").into(),
        received_at: Utc.with_ymd_and_hms(2026, 9, 5, 12, 0, 0).unwrap(),
        ..cancellation()
    };

    REPO.create(&mut txn, second.clone()).await.unwrap();
    REPO.create(&mut txn, cancellation()).await.unwrap();
    // belongs to no account
    REPO.create(&mut txn, withdrawal()).await.unwrap();
    // belongs to another account
    REPO.create(
        &mut txn,
        ContractDeclaration {
            user_id: Some(BAR.user.id),
            ..other_cancellation()
        },
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_by_user_id(&mut txn, FOO.user.id).await.unwrap(),
        [cancellation(), second]
    );
    assert_eq!(
        REPO.list_by_user_id(&mut txn, BAR.user.id).await.unwrap(),
        [ContractDeclaration {
            user_id: Some(BAR.user.id),
            ..other_cancellation()
        }]
    );
}

/// The declaration is evidence of a legal declaration and must survive the
/// deletion of the associated account.
#[tokio::test]
async fn survives_user_deletion() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, cancellation()).await.unwrap();
    txn.commit().await.unwrap();

    db.execute(&format!("delete from users where id='{}';", *FOO.user.id))
        .await
        .unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.count(&mut txn, None).await.unwrap(), 1);
    assert_eq!(
        REPO.list(&mut txn, None, make_slice(100, 0)).await.unwrap(),
        [ContractDeclaration {
            user_id: None,
            ..cancellation()
        }]
    );
}
