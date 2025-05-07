use academy_demo::{
    SHA256HASH1, UUID1,
    mfa::{ADMIN2_TOTP_1, FOO_TOTP_1},
    user::{ADMIN2, BAR, FOO},
};
use academy_models::mfa::{MfaRecoveryCodeHash, TotpDevice, TotpDevicePatchRef, TotpSecret};
use academy_persistence_contracts::{Database, Transaction, mfa::MfaRepository};
use academy_persistence_postgres::mfa::PostgresMfaRepository;
use academy_utils::Apply;

use crate::common::setup;

const REPO: PostgresMfaRepository = PostgresMfaRepository;

#[tokio::test]
async fn list_totp_devices_by_user() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO
        .list_totp_devices_by_user(&mut txn, ADMIN2.user.id)
        .await
        .unwrap();
    assert_eq!(result, [ADMIN2_TOTP_1.clone()]);

    let result = REPO
        .list_totp_devices_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, [FOO_TOTP_1.clone()]);

    let result = REPO
        .list_totp_devices_by_user(&mut txn, BAR.user.id)
        .await
        .unwrap();
    assert_eq!(result, []);
}

#[tokio::test]
async fn create_totp_device() {
    let expected = TotpDevice {
        id: UUID1.into(),
        user_id: BAR.user.id,
        enabled: true,
        created_at: BAR.user.created_at,
    };
    let secret = TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.create_totp_device(&mut txn, &expected, &secret)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let devices = REPO
        .list_totp_devices_by_user(&mut txn, BAR.user.id)
        .await
        .unwrap();
    assert_eq!(devices, [expected]);

    let result = REPO
        .get_totp_device_secret(&mut txn, UUID1.into())
        .await
        .unwrap();
    assert_eq!(result, secret);
}

#[tokio::test]
async fn update_totp_device() {
    let expected = FOO_TOTP_1.clone().with(|x| x.enabled = true);

    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .update_totp_device(
            &mut txn,
            expected.id,
            TotpDevicePatchRef::new().update_enabled(&true),
        )
        .await
        .unwrap();
    assert!(result);
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .list_totp_devices_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, [expected]);
}

#[tokio::test]
async fn delete_totp_devices() {
    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.delete_totp_devices_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();

    let result = REPO
        .list_totp_devices_by_user(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, []);
}

#[tokio::test]
async fn save_and_get_totp_device_secret() {
    let secret = TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.save_totp_device_secret(&mut txn, FOO_TOTP_1.id, &secret)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_totp_device_secret(&mut txn, FOO_TOTP_1.id)
        .await
        .unwrap();
    assert_eq!(result, secret);
}

#[tokio::test]
async fn list_enabled_totp_device_secrets() {
    let db = setup().await;
    let mut txn = db.begin_transaction().await.unwrap();

    let admin_secret = REPO
        .get_totp_device_secret(&mut txn, ADMIN2_TOTP_1.id)
        .await
        .unwrap();

    let result = REPO
        .list_enabled_totp_device_secrets_by_user(&mut txn, ADMIN2_TOTP_1.user_id)
        .await
        .unwrap();
    assert_eq!(result, [admin_secret]);

    let result = REPO
        .list_enabled_totp_device_secrets_by_user(&mut txn, FOO_TOTP_1.user_id)
        .await
        .unwrap();
    assert_eq!(result, []);

    let result = REPO
        .list_enabled_totp_device_secrets_by_user(&mut txn, BAR.user.id)
        .await
        .unwrap();
    assert_eq!(result, []);
}

#[tokio::test]
async fn save_get_and_delete_mfa_recovery_code_hash() {
    let hash = MfaRecoveryCodeHash::from(*SHA256HASH1);

    let db = setup().await;

    let mut txn = db.begin_transaction().await.unwrap();
    REPO.save_mfa_recovery_code_hash(&mut txn, FOO.user.id, hash)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_mfa_recovery_code_hash(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result.unwrap(), hash);

    REPO.delete_mfa_recovery_code_hash(&mut txn, FOO.user.id)
        .await
        .unwrap();
    txn.commit().await.unwrap();

    let mut txn = db.begin_transaction().await.unwrap();
    let result = REPO
        .get_mfa_recovery_code_hash(&mut txn, FOO.user.id)
        .await
        .unwrap();
    assert_eq!(result, None);
}
