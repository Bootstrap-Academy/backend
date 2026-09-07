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
        contract_designation: Some("Premium-Abo, monatlich".try_into().unwrap()),
        cancellation_type: Some(ContractCancellationType::Extraordinary),
        details: "Zu teuer".try_into().unwrap(),
        requested_end: Some(Utc.with_ymd_and_hms(2026, 12, 31, 23, 0, 0).unwrap()),
        effective_end: Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()),
        processed_at: Some(Utc.with_ymd_and_hms(2026, 9, 4, 8, 0, 0).unwrap()),
        processing_note: Some("Kündigung akzeptiert, Ende bestätigt".try_into().unwrap()),
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
        contract_designation: None,
        cancellation_type: None,
        details: Default::default(),
        requested_end: None,
        effective_end: None,
        processed_at: None,
        processing_note: None,
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
        contract_designation: Some("Vertrag Nr. 4711".try_into().unwrap()),
        cancellation_type: Some(ContractCancellationType::Ordinary),
        details: "Kein Interesse mehr".try_into().unwrap(),
        requested_end: None,
        effective_end: None,
        processed_at: None,
        processing_note: None,
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

/// Two declarations that arrived in the same moment need a stable order, or a
/// page boundary between them would return one of them twice and skip the
/// other.
#[tokio::test]
async fn pagination_with_equal_timestamps() {
    let db = setup().await;

    let same_moment = Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap();
    let declarations = [
        cancellation(),
        ContractDeclaration {
            id: uuid!("3f5d59fa-8f3b-4e46-9d51-4f3c6b0d0a7e").into(),
            received_at: same_moment,
            ..withdrawal()
        },
        ContractDeclaration {
            id: uuid!("b3a2eb0e-7a35-4c2e-9ee6-6cf4a7a3f5b1").into(),
            received_at: same_moment,
            ..other_cancellation()
        },
    ];

    let mut txn = db.begin_transaction().await.unwrap();
    for declaration in &declarations {
        REPO.create(&mut txn, declaration.clone()).await.unwrap();
    }
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let whole = REPO.list(&mut txn, None, make_slice(100, 0)).await.unwrap();
    assert_eq!(whole.len(), declarations.len());

    // Reading the same list one entry at a time returns exactly the same
    // entries in the same order.
    let mut paged = Vec::new();
    for offset in 0..declarations.len() as u64 {
        paged.extend(
            REPO.list(&mut txn, None, make_slice(1, offset))
                .await
                .unwrap(),
        );
    }
    assert_eq!(paged, whole);
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

/// The declarations are pruned once a claim out of the declared contract is
/// time-barred; `academy task prune-database` passes the cutoff.
#[tokio::test]
async fn delete_by_received_at() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, cancellation()).await.unwrap();
    REPO.create(&mut txn, withdrawal()).await.unwrap();
    REPO.create(&mut txn, other_cancellation()).await.unwrap();
    txn.commit().await.unwrap();

    // Nothing is old enough yet.
    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.delete_by_received_at(
            &mut txn,
            Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap()
        )
        .await
        .unwrap(),
        0
    );

    // The cutoff is exclusive, so a declaration received exactly at it is
    // kept.
    assert_eq!(
        REPO.delete_by_received_at(
            &mut txn,
            Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap()
        )
        .await
        .unwrap(),
        2
    );
    assert_eq!(
        REPO.list(&mut txn, None, make_slice(100, 0)).await.unwrap(),
        [cancellation()]
    );

    assert_eq!(
        REPO.delete_by_received_at(&mut txn, Utc.with_ymd_and_hms(2029, 1, 1, 0, 0, 0).unwrap())
            .await
            .unwrap(),
        1
    );
    assert_eq!(REPO.count(&mut txn, None).await.unwrap(), 0);
}

#[tokio::test]
async fn get() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.get(&mut txn, cancellation().id).await.unwrap(), None);

    REPO.create(&mut txn, cancellation()).await.unwrap();
    REPO.create(&mut txn, withdrawal()).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, cancellation().id).await.unwrap(),
        Some(cancellation())
    );
    assert_eq!(
        REPO.get(&mut txn, withdrawal().id).await.unwrap(),
        Some(withdrawal())
    );
}

/// Recording that a declaration has been processed writes the time, the end of
/// the contract and the note, and leaves everything else alone.
#[tokio::test]
async fn set_processed() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, withdrawal()).await.unwrap();
    REPO.create(&mut txn, other_cancellation()).await.unwrap();
    txn.commit().await.unwrap();

    let processed_at = Utc.with_ymd_and_hms(2026, 9, 8, 9, 30, 0).unwrap();
    let effective_end = Utc.with_ymd_and_hms(2026, 9, 30, 21, 59, 59).unwrap();
    let note = "Außerordentliche Kündigung anerkannt".try_into().unwrap();

    let expected = ContractDeclaration {
        processed_at: Some(processed_at),
        effective_end: Some(effective_end),
        processing_note: Some("Außerordentliche Kündigung anerkannt".try_into().unwrap()),
        ..withdrawal()
    };

    let mut txn = db.begin_transaction().await.unwrap();
    // what comes back is the stored row, not what the caller passed in
    assert_eq!(
        REPO.set_processed(
            &mut txn,
            withdrawal().id,
            processed_at,
            Some(effective_end),
            Some(note),
        )
        .await
        .unwrap(),
        Some(expected.clone())
    );
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.get(&mut txn, withdrawal().id).await.unwrap(),
        Some(expected)
    );
    // the other declaration is untouched
    assert_eq!(
        REPO.get(&mut txn, other_cancellation().id).await.unwrap(),
        Some(other_cancellation())
    );
}

/// A declaration that does not exist is reported rather than silently created.
#[tokio::test]
async fn set_processed_not_found() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.set_processed(
            &mut txn,
            cancellation().id,
            Utc.with_ymd_and_hms(2026, 9, 8, 9, 30, 0).unwrap(),
            None,
            None,
        )
        .await
        .unwrap(),
        None
    );
}
