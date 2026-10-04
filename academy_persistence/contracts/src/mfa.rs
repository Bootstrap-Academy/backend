use std::future::Future;

use academy_models::{
    mfa::{MfaRecoveryCodeHash, TotpDevice, TotpDeviceId, TotpDevicePatchRef, TotpSecret},
    user::UserId,
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait MfaRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return all TOTP devices of the given user.
    fn list_totp_devices_by_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Vec<TotpDevice>>> + Send;

    /// Create a new TOTP device and set the associated secret.
    fn create_totp_device(
        &self,
        txn: &mut Txn,
        totp_device: &TotpDevice,
        secret: &TotpSecret,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Update an existing TOTP device.
    #[allow(clippy::needless_lifetimes, reason = "automock")]
    fn update_totp_device<'a>(
        &self,
        txn: &mut Txn,
        totp_device_id: TotpDeviceId,
        patch: TotpDevicePatchRef<'a>,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Delete all TOTP devices of the given user.
    fn delete_totp_devices_by_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the secrets of all enabled TOTP devices of the given user.
    fn list_enabled_totp_device_secrets_by_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Vec<TotpSecret>>> + Send;

    /// Atomically consume a verified step for the user's matching secret.
    /// A replay, stale secret or missing/disabled device returns false without
    /// changing replay state. Pending devices are allowed only at confirmation.
    fn consume_totp_step(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        secret: &TotpSecret,
        step: i64,
        allow_pending: bool,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Return the secret of the given TOTP device.
    fn get_totp_device_secret(
        &self,
        txn: &mut Txn,
        totp_device_id: TotpDeviceId,
    ) -> impl Future<Output = anyhow::Result<TotpSecret>> + Send;

    /// Update the secret of the given TOTP device.
    fn save_totp_device_secret(
        &self,
        txn: &mut Txn,
        totp_device_id: TotpDeviceId,
        secret: &TotpSecret,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the MFA recovery code hash of the given user.
    fn get_mfa_recovery_code_hash(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<MfaRecoveryCodeHash>>> + Send;

    /// Set the MFA recovery code hash of the given user.
    fn save_mfa_recovery_code_hash(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        recovery_code_hash: MfaRecoveryCodeHash,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Delete the MFA recovery code hash of the given user.
    fn delete_mfa_recovery_code_hash(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockMfaRepository<Txn> {
    pub fn with_consume_totp_step(
        mut self,
        user_id: UserId,
        secret: TotpSecret,
        step: i64,
        allow_pending: bool,
        result: bool,
    ) -> Self {
        self.expect_consume_totp_step()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(secret),
                mockall::predicate::eq(step),
                mockall::predicate::eq(allow_pending),
            )
            .return_once(move |_, _, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_list_totp_devices_by_user(
        mut self,
        user_id: UserId,
        result: Vec<TotpDevice>,
    ) -> Self {
        self.expect_list_totp_devices_by_user()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_create_totp_device(mut self, totp_device: TotpDevice, secret: TotpSecret) -> Self {
        self.expect_create_totp_device()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(totp_device),
                mockall::predicate::eq(secret),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_update_totp_device(
        mut self,
        totp_device_id: TotpDeviceId,
        patch: academy_models::mfa::TotpDevicePatch,
        result: bool,
    ) -> Self {
        self.expect_update_totp_device()
            .once()
            .withf(move |_, id, p| *id == totp_device_id && *p == patch.as_ref())
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_delete_totp_devices_by_user(mut self, user_id: UserId) -> Self {
        self.expect_delete_totp_devices_by_user()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_list_enabled_totp_device_secrets_by_user(
        mut self,
        user_id: UserId,
        secrets: Vec<TotpSecret>,
    ) -> Self {
        self.expect_list_enabled_totp_device_secrets_by_user()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(secrets))));
        self
    }

    pub fn with_get_totp_device_secret(
        mut self,
        totp_device_id: TotpDeviceId,
        secret: TotpSecret,
    ) -> Self {
        self.expect_get_totp_device_secret()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(totp_device_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(secret))));
        self
    }

    pub fn with_save_totp_device_secret(
        mut self,
        totp_device_id: TotpDeviceId,
        secret: TotpSecret,
    ) -> Self {
        self.expect_save_totp_device_secret()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(totp_device_id),
                mockall::predicate::eq(secret),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_get_mfa_recovery_code_hash(
        mut self,
        user_id: UserId,
        recovery_code: Option<MfaRecoveryCodeHash>,
    ) -> Self {
        self.expect_get_mfa_recovery_code_hash()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(recovery_code))));
        self
    }

    pub fn with_save_mfa_recovery_code_hash(
        mut self,
        user_id: UserId,
        recovery_code_hash: MfaRecoveryCodeHash,
    ) -> Self {
        self.expect_save_mfa_recovery_code_hash()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(recovery_code_hash),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_delete_mfa_recovery_code_hash(mut self, user_id: UserId) -> Self {
        self.expect_delete_mfa_recovery_code_hash()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
