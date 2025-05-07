use academy_core_mfa_contracts::totp_device::{MfaTotpDeviceConfirmError, MfaTotpDeviceService};
use academy_di::Build;
use academy_models::{
    mfa::{TotpCode, TotpDevice, TotpDeviceId, TotpDevicePatch, TotpDevicePatchRef, TotpSetup},
    user::UserId,
};
use academy_persistence_contracts::mfa::MfaRepository;
use academy_shared_contracts::{
    id::IdService,
    time::TimeService,
    totp::{TotpCheckError, TotpService},
};
use academy_utils::{patch::Patch, trace_instrument};
use anyhow::Context;
use tracing::trace;

#[derive(Debug, Clone, Build, Default)]
pub struct MfaTotpDeviceServiceImpl<Id, Time, Totp, MfaRepo> {
    id: Id,
    time: Time,
    totp: Totp,
    mfa_repo: MfaRepo,
}

impl<Txn, Id, Time, Totp, MfaRepo> MfaTotpDeviceService<Txn>
    for MfaTotpDeviceServiceImpl<Id, Time, Totp, MfaRepo>
where
    Txn: Send + Sync + 'static,
    Id: IdService,
    Time: TimeService,
    Totp: TotpService,
    MfaRepo: MfaRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn create(&self, txn: &mut Txn, user_id: UserId) -> anyhow::Result<TotpSetup> {
        let (secret, setup) = self.totp.generate_secret();

        let totp_device = TotpDevice {
            id: self.id.generate(),
            user_id,
            enabled: false,
            created_at: self.time.now(),
        };

        self.mfa_repo
            .create_totp_device(txn, &totp_device, &secret)
            .await
            .context("Failed to save totp device in database")?;

        Ok(setup)
    }

    #[trace_instrument(skip(self, txn))]
    async fn confirm(
        &self,
        txn: &mut Txn,
        totp_device: TotpDevice,
        code: TotpCode,
    ) -> Result<TotpDevice, MfaTotpDeviceConfirmError> {
        trace!("get secret");
        let secret = self
            .mfa_repo
            .get_totp_device_secret(txn, totp_device.id)
            .await
            .context("Failed to get totp device secret from database")?;

        trace!("check code");
        self.totp
            .check(&code, secret)
            .await
            .map_err(|err| match err {
                TotpCheckError::InvalidCode | TotpCheckError::RecentlyUsed => {
                    MfaTotpDeviceConfirmError::InvalidCode
                }
                TotpCheckError::Other(err) => err.context("Failed to check totp code").into(),
            })?;

        trace!("update device");
        let patch = TotpDevicePatch::new().update_enabled(true);
        self.mfa_repo
            .update_totp_device(txn, totp_device.id, patch.as_ref())
            .await
            .context("Failed to update totp device in database")?;

        Ok(totp_device.update(patch))
    }

    #[trace_instrument(skip(self, txn))]
    async fn reset(
        &self,
        txn: &mut Txn,
        totp_device_id: TotpDeviceId,
    ) -> anyhow::Result<TotpSetup> {
        let (secret, setup) = self.totp.generate_secret();

        trace!("update device");
        self.mfa_repo
            .update_totp_device(
                txn,
                totp_device_id,
                TotpDevicePatchRef::new().update_enabled(&false),
            )
            .await
            .context("Failed to update totp device in database")?;

        trace!("update secret");
        self.mfa_repo
            .save_totp_device_secret(txn, totp_device_id, &secret)
            .await
            .context("Failed to update totp device secret in database")?;

        Ok(setup)
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::{mfa::FOO_TOTP_1, user::FOO};
    use academy_models::mfa::TotpSecret;
    use academy_persistence_contracts::mfa::MockMfaRepository;
    use academy_shared_contracts::{
        id::MockIdService, time::MockTimeService, totp::MockTotpService,
    };
    use academy_utils::{Apply, assert_matches};

    use super::*;

    type Sut = MfaTotpDeviceServiceImpl<
        MockIdService,
        MockTimeService,
        MockTotpService,
        MockMfaRepository<()>,
    >;

    #[tokio::test]
    async fn create() {
        // Arrange
        let secret = TotpSecret::try_new("the random totp secret".to_owned().into_bytes()).unwrap();
        let setup = TotpSetup {
            secret: "ORUGKIDSMFXGI33NEB2G65DQEBZWKY3SMV2A".into(),
        };

        let id = MockIdService::new().with_generate(FOO_TOTP_1.id);
        let time = MockTimeService::new().with_now(FOO_TOTP_1.created_at);

        let totp = MockTotpService::new().with_generate_secret(secret.clone(), setup.clone());

        let mfa_repo = MockMfaRepository::new().with_create_totp_device(FOO_TOTP_1.clone(), secret);

        let sut = MfaTotpDeviceServiceImpl {
            id,
            time,
            totp,
            mfa_repo,
        };

        // Act
        let result = sut.create(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), setup);
    }

    #[tokio::test]
    async fn confirm_ok() {
        // Arrange
        let expected = FOO_TOTP_1.clone().with(|x| x.enabled = true);
        let code = TotpCode::try_new("123456").unwrap();
        let secret = TotpSecret::try_new("the random totp secret".to_owned().into_bytes()).unwrap();

        let totp = MockTotpService::new().with_check(code.clone(), secret.clone(), Ok(()));

        let mfa_repo = MockMfaRepository::new()
            .with_get_totp_device_secret(FOO_TOTP_1.id, secret)
            .with_update_totp_device(
                FOO_TOTP_1.id,
                TotpDevicePatch::new().update_enabled(true),
                true,
            );

        let sut = MfaTotpDeviceServiceImpl {
            totp,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.confirm(&mut (), FOO_TOTP_1.clone(), code).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn confirm_invalid_code() {
        // Arrange
        let code = TotpCode::try_new("123456").unwrap();
        let secret = TotpSecret::try_new("the random totp secret".to_owned().into_bytes()).unwrap();

        let totp = MockTotpService::new().with_check(
            code.clone(),
            secret.clone(),
            Err(TotpCheckError::InvalidCode),
        );

        let mfa_repo = MockMfaRepository::new().with_get_totp_device_secret(FOO_TOTP_1.id, secret);

        let sut = MfaTotpDeviceServiceImpl {
            totp,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.confirm(&mut (), FOO_TOTP_1.clone(), code).await;

        // Assert
        assert_matches!(result, Err(MfaTotpDeviceConfirmError::InvalidCode));
    }

    #[tokio::test]
    async fn reset() {
        // Arrange
        let secret = TotpSecret::try_new("the random totp secret".to_owned().into_bytes()).unwrap();
        let setup = TotpSetup {
            secret: "ORUGKIDSMFXGI33NEB2G65DQEBZWKY3SMV2A".into(),
        };

        let totp = MockTotpService::new().with_generate_secret(secret.clone(), setup.clone());

        let mfa_repo = MockMfaRepository::new()
            .with_update_totp_device(
                FOO_TOTP_1.id,
                TotpDevicePatch::new().update_enabled(false),
                true,
            )
            .with_save_totp_device_secret(FOO_TOTP_1.id, secret);

        let sut = MfaTotpDeviceServiceImpl {
            totp,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.reset(&mut (), FOO_TOTP_1.id).await;

        // Assert
        assert_eq!(result.unwrap(), setup);
    }
}
