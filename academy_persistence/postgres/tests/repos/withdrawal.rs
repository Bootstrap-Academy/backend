use academy_demo::{UUID1, UUID2, user::FOO};
use academy_models::withdrawal::{WithdrawalConsent, WithdrawalSubject};
use academy_persistence_contracts::{Database, Transaction, withdrawal::WithdrawalRepository};
use academy_persistence_postgres::withdrawal::PostgresWithdrawalRepository;
use chrono::{TimeZone, Utc};

use crate::common::setup;

const REPO: PostgresWithdrawalRepository = PostgresWithdrawalRepository;

#[tokio::test]
async fn create_and_list() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_by_user_id(&mut txn, FOO.user.id).await.unwrap(),
        Vec::new()
    );

    let premium = WithdrawalConsent {
        id: UUID1.into(),
        user_id: FOO.user.id,
        subject: WithdrawalSubject::Premium,
        reference: None,
        text_version: "2026-09".try_into().unwrap(),
        consented_at: Utc.with_ymd_and_hms(2026, 9, 3, 12, 0, 0).unwrap(),
    };
    let course = WithdrawalConsent {
        id: UUID2.into(),
        user_id: FOO.user.id,
        subject: WithdrawalSubject::Course,
        reference: Some("html".try_into().unwrap()),
        text_version: "2026-09".try_into().unwrap(),
        consented_at: Utc.with_ymd_and_hms(2026, 9, 3, 13, 0, 0).unwrap(),
    };

    REPO.create(&mut txn, &premium).await.unwrap();
    REPO.create(&mut txn, &course).await.unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    assert_eq!(
        REPO.list_by_user_id(&mut txn, FOO.user.id).await.unwrap(),
        vec![premium, course]
    );
}
