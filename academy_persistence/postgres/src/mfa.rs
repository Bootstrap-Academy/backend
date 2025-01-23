use academy_di::Build;
use academy_models::{
    mfa::{MfaRecoveryCodeHash, TotpDevice, TotpDeviceId, TotpDevicePatchRef, TotpSecret},
    user::UserId,
};
use academy_persistence_contracts::mfa::MfaRepository;
use academy_utils::trace_instrument;
use clorinde::{
    client::Params,
    queries::{
        self,
        mfa::{CreateTotpDeviceParams, UpdateTotpDeviceParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::{decode_sha256hash, PostgresTransaction};

#[derive(Debug, Clone, Build)]
pub struct PostgresMfaRepository;

impl MfaRepository<PostgresTransaction> for PostgresMfaRepository {
    #[trace_instrument(skip(self, txn))]
    async fn list_totp_devices_by_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<TotpDevice>> {
        queries::mfa::list_totp_devices_by_user()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_totp_device))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn create_totp_device(
        &self,
        txn: &mut PostgresTransaction,
        totp_device: &TotpDevice,
        secret: &TotpSecret,
    ) -> anyhow::Result<()> {
        let params = CreateTotpDeviceParams {
            id: *totp_device.id,
            user_id: *totp_device.user_id,
            enabled: totp_device.enabled,
            created_at: totp_device.created_at.into(),
        };

        queries::mfa::create_totp_device()
            .params(txn.txn(), &params)
            .await?;

        queries::mfa::set_totp_device_secret()
            .bind(txn.txn(), &totp_device.id, &**secret)
            .await?;

        Ok(())
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_totp_device<'a>(
        &self,
        txn: &mut PostgresTransaction,
        totp_device_id: TotpDeviceId,
        TotpDevicePatchRef { enabled }: TotpDevicePatchRef<'a>,
    ) -> anyhow::Result<bool> {
        let params = UpdateTotpDeviceParams {
            id: *totp_device_id,
            enabled: enabled.update().copied(),
        };

        queries::mfa::update_totp_device()
            .params(txn.txn(), &params)
            .await
            .map(|n| n != 0)
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_totp_devices_by_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<()> {
        queries::mfa::delete_totp_devices_by_user()
            .bind(txn.txn(), &user_id)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list_enabled_totp_device_secrets_by_user(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Vec<TotpSecret>> {
        queries::mfa::list_enabled_totp_device_secrets_by_user()
            .bind(txn.txn(), &user_id)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_totp_device_secret))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_totp_device_secret(
        &self,
        txn: &mut PostgresTransaction,
        totp_device_id: TotpDeviceId,
    ) -> anyhow::Result<TotpSecret> {
        queries::mfa::get_totp_device_secret()
            .bind(txn.txn(), &totp_device_id)
            .one()
            .await
            .map_err(Into::into)
            .and_then(decode_totp_device_secret)
    }

    #[trace_instrument(skip(self, txn))]
    async fn save_totp_device_secret(
        &self,
        txn: &mut PostgresTransaction,
        totp_device_id: TotpDeviceId,
        secret: &TotpSecret,
    ) -> anyhow::Result<()> {
        queries::mfa::set_totp_device_secret()
            .bind(txn.txn(), &totp_device_id, &**secret)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn get_mfa_recovery_code_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<Option<MfaRecoveryCodeHash>> {
        queries::mfa::get_recovery_code_hash()
            .bind(txn.txn(), &user_id)
            .opt()
            .await
            .map_err(Into::into)
            .and_then(|row| {
                row.map(|row| decode_sha256hash(row).map(Into::into))
                    .transpose()
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn save_mfa_recovery_code_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
        recovery_code_hash: MfaRecoveryCodeHash,
    ) -> anyhow::Result<()> {
        queries::mfa::set_recovery_code_hash()
            .bind(txn.txn(), &user_id, &recovery_code_hash.as_slice())
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_mfa_recovery_code_hash(
        &self,
        txn: &mut PostgresTransaction,
        user_id: UserId,
    ) -> anyhow::Result<()> {
        queries::mfa::delete_recovery_code_hash()
            .bind(txn.txn(), &user_id)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

fn decode_totp_device(value: queries::mfa::TotpDevice) -> anyhow::Result<TotpDevice> {
    Ok(TotpDevice {
        id: value.id.into(),
        user_id: value.user_id.into(),
        enabled: value.enabled,
        created_at: value.created_at.into(),
    })
}

fn decode_totp_device_secret(value: Vec<u8>) -> anyhow::Result<TotpSecret> {
    value.try_into().map_err(Into::into)
}
