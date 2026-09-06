use academy_demo::{
    UUID1, UUID2,
    user::{ADMIN, FOO},
};
use academy_models::admin_audit::{AdminAuditLogEntry, AdminAuditLogFilter};
use academy_persistence_contracts::{Database, Transaction, admin_audit::AdminAuditRepository};
use academy_persistence_postgres::admin_audit::PostgresAdminAuditRepository;
use chrono::{DateTime, TimeZone, Utc};
use uuid::uuid;

use crate::{
    common::setup,
    repos::{make_slice, sliced},
};

const REPO: PostgresAdminAuditRepository = PostgresAdminAuditRepository;

/// An entry with all optional columns set.
fn update_user() -> AdminAuditLogEntry {
    AdminAuditLogEntry {
        id: UUID1.into(),
        at: Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap(),
        admin_user_id: ADMIN.user.id,
        method: "PATCH".try_into().unwrap(),
        path: format!("/auth/users/{}", *FOO.user.id).try_into().unwrap(),
        target_user_id: Some(FOO.user.id),
        status: 200,
        request_id: "9EmXjWNfd0GcVXyIkbXQ2g".try_into().unwrap(),
    }
}

/// An entry with all optional columns unset.
fn purchase_premium() -> AdminAuditLogEntry {
    AdminAuditLogEntry {
        id: UUID2.into(),
        at: Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap(),
        admin_user_id: ADMIN.user.id,
        method: "POST".try_into().unwrap(),
        path: "/shop/premium".try_into().unwrap(),
        target_user_id: None,
        status: 403,
        request_id: "Y5DTPpUDQJ2rB7RQNSjWLQ".try_into().unwrap(),
    }
}

fn delete_user() -> AdminAuditLogEntry {
    AdminAuditLogEntry {
        id: uuid!("f0e7c0f1-7e33-4c19-9a3b-2b2c5d4b8d21").into(),
        at: Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap(),
        admin_user_id: FOO.user.id,
        method: "DELETE".try_into().unwrap(),
        path: format!("/auth/users/{}", *ADMIN.user.id)
            .try_into()
            .unwrap(),
        target_user_id: Some(ADMIN.user.id),
        status: 200,
        request_id: "2Q9SEXOoTLWJgAeaTuyfaQ".try_into().unwrap(),
    }
}

async fn setup_with_entries() -> academy_persistence_postgres::PostgresDatabase {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create(&mut txn, &update_user()).await.unwrap();
    REPO.create(&mut txn, &purchase_premium()).await.unwrap();
    REPO.create(&mut txn, &delete_user()).await.unwrap();
    txn.commit().await.unwrap();

    db
}

#[tokio::test]
async fn create_list_count() {
    let db = setup().await;
    let filter = AdminAuditLogFilter::default();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.count(&mut txn, filter).await.unwrap(), 0);
    assert_eq!(
        REPO.list(&mut txn, filter, make_slice(100, 0))
            .await
            .unwrap(),
        []
    );

    REPO.create(&mut txn, &update_user()).await.unwrap();
    REPO.create(&mut txn, &purchase_premium()).await.unwrap();
    REPO.create(&mut txn, &delete_user()).await.unwrap();
    txn.commit().await.unwrap();

    // most recent first, all optional columns round-trip
    let expected = [update_user(), purchase_premium(), delete_user()];

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.count(&mut txn, filter).await.unwrap(), 3);
    assert_eq!(
        REPO.list(&mut txn, filter, make_slice(100, 0))
            .await
            .unwrap(),
        expected
    );
}

#[tokio::test]
async fn filter_by_user() {
    let db = setup_with_entries().await;

    let mut txn = db.begin_transaction().await.unwrap();

    let filter = AdminAuditLogFilter {
        admin_user_id: Some(ADMIN.user.id),
        target_user_id: None,
    };
    assert_eq!(REPO.count(&mut txn, filter).await.unwrap(), 2);
    assert_eq!(
        REPO.list(&mut txn, filter, make_slice(100, 0))
            .await
            .unwrap(),
        [update_user(), purchase_premium()]
    );

    let filter = AdminAuditLogFilter {
        admin_user_id: None,
        target_user_id: Some(ADMIN.user.id),
    };
    assert_eq!(REPO.count(&mut txn, filter).await.unwrap(), 1);
    assert_eq!(
        REPO.list(&mut txn, filter, make_slice(100, 0))
            .await
            .unwrap(),
        [delete_user()]
    );

    let filter = AdminAuditLogFilter {
        admin_user_id: Some(ADMIN.user.id),
        target_user_id: Some(ADMIN.user.id),
    };
    assert_eq!(REPO.count(&mut txn, filter).await.unwrap(), 0);
    assert_eq!(
        REPO.list(&mut txn, filter, make_slice(100, 0))
            .await
            .unwrap(),
        []
    );
}

#[tokio::test]
async fn pagination() {
    let db = setup_with_entries().await;
    let filter = AdminAuditLogFilter::default();

    let expected = &[update_user(), purchase_premium(), delete_user()];

    let mut txn = db.begin_transaction().await.unwrap();
    for slice in [
        make_slice(100, 0),
        make_slice(2, 0),
        make_slice(2, 1),
        make_slice(100, 1),
        make_slice(1, 2),
        make_slice(100, 17),
    ] {
        let result = REPO.list(&mut txn, filter, slice).await.unwrap();
        assert_eq!(result, sliced(expected, slice));
    }
}

/// The retention sweep of `academy task prune-database` removes entries older
/// than the cutoff and keeps the rest.
#[tokio::test]
async fn delete_by_at() {
    let db = setup_with_entries().await;
    let filter = AdminAuditLogFilter::default();

    let cutoff: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.delete_by_at(&mut txn, cutoff).await.unwrap(), 1);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list(&mut txn, filter, make_slice(100, 0))
            .await
            .unwrap(),
        [update_user(), purchase_premium()]
    );
}

/// The audit log must stay complete and attributable even after the acting or
/// the affected account is deleted.
#[tokio::test]
async fn survives_user_deletion() {
    let db = setup_with_entries().await;
    let filter = AdminAuditLogFilter::default();

    db.execute(&format!("delete from users where id='{}';", *FOO.user.id))
        .await
        .unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(REPO.count(&mut txn, filter).await.unwrap(), 3);
    assert_eq!(
        REPO.list(&mut txn, filter, make_slice(100, 0))
            .await
            .unwrap(),
        [update_user(), purchase_premium(), delete_user()]
    );
}
