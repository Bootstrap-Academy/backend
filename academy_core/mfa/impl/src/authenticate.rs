use academy_core_mfa_contracts::{
    authenticate::{MfaAuthenticateError, MfaAuthenticateResult, MfaAuthenticateService},
    disable::MfaDisableService,
};
use academy_di::Build;
use academy_models::{mfa::MfaAuthentication, user::UserId};
use academy_persistence_contracts::mfa::MfaRepository;
use academy_shared_contracts::{
    hash::HashService,
    totp::{TotpCheckError, TotpService},
};
use academy_utils::trace_instrument;
use anyhow::Context;
use tracing::trace;

#[derive(Debug, Clone, Build, Default)]
pub struct MfaAuthenticateServiceImpl<Hash, Totp, MfaDisable, MfaRepo> {
    hash: Hash,
    totp: Totp,
    mfa_disable: MfaDisable,
    mfa_repo: MfaRepo,
}

impl<Txn, Hash, Totp, MfaDisable, MfaRepo> MfaAuthenticateService<Txn>
    for MfaAuthenticateServiceImpl<Hash, Totp, MfaDisable, MfaRepo>
where
    Txn: Send + Sync + 'static,
    Hash: HashService,
    Totp: TotpService,
    MfaDisable: MfaDisableService<Txn>,
    MfaRepo: MfaRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn authenticate(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        cmd: MfaAuthentication,
    ) -> Result<MfaAuthenticateResult, MfaAuthenticateError> {
        trace!("list totp secrets");
        let totp_secrets = self
            .mfa_repo
            .list_enabled_totp_device_secrets_by_user(txn, user_id)
            .await
            .context("Failed to get totp secrets from database")?;

        if totp_secrets.is_empty() {
            trace!("no totp secrets");
            return Ok(MfaAuthenticateResult::Disabled);
        }

        if let Some(recovery_code) = cmd.recovery_code {
            trace!("try recovery code");

            if let Some(hash) = self
                .mfa_repo
                .get_mfa_recovery_code_hash(txn, user_id)
                .await
                .context("Failed to get recovery code hash from database")?
            {
                if self.hash.sha256(&recovery_code) == *hash {
                    trace!("recovery code matches");
                    self.mfa_disable
                        .disable(txn, user_id)
                        .await
                        .context("Failed to disable MFA")?;
                    return Ok(MfaAuthenticateResult::Reset);
                }
            }
        }

        if let Some(code) = cmd.totp_code {
            trace!("try totp code");

            for secret in totp_secrets {
                match self.totp.check(&code, secret).await {
                    Ok(()) => {
                        trace!("totp code matches");
                        return Ok(MfaAuthenticateResult::Ok);
                    }
                    Err(TotpCheckError::InvalidCode | TotpCheckError::RecentlyUsed) => (),
                    Err(TotpCheckError::Other(err)) => {
                        return Err(err.context("Failed to check totp code").into());
                    }
                }
            }
        }

        trace!("all mfa options failed");

        Err(MfaAuthenticateError::Failed)
    }
}

#[cfg(test)]
mod tests {
    use academy_core_mfa_contracts::disable::MockMfaDisableService;
    use academy_demo::{SHA256HASH1, SHA256HASH2, user::FOO};
    use academy_models::mfa::TotpSecret;
    use academy_persistence_contracts::mfa::MockMfaRepository;
    use academy_shared_contracts::{
        hash::MockHashService,
        totp::{MockTotpService, TotpCheckError},
    };
    use academy_utils::assert_matches;

    use super::*;

    type Sut = MfaAuthenticateServiceImpl<
        MockHashService,
        MockTotpService,
        MockMfaDisableService<()>,
        MockMfaRepository<()>,
    >;

    #[tokio::test]
    async fn ok_mfa_disabled() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: None,
            recovery_code: None,
        };

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![]);

        let sut = MfaAuthenticateServiceImpl {
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_eq!(result.unwrap(), MfaAuthenticateResult::Disabled);
    }

    #[tokio::test]
    async fn ok_recovery_code() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: None,
            recovery_code: Some("PJVURV-QRK3YJ-O3U7T6-D50KAC".try_into().unwrap()),
        };

        let secret =
            TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

        let hash =
            MockHashService::new().with_sha256(cmd.recovery_code.clone().unwrap(), *SHA256HASH1);

        let mfa_disable = MockMfaDisableService::new().with_disable(FOO.user.id);

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![secret])
            .with_get_mfa_recovery_code_hash(FOO.user.id, Some((*SHA256HASH1).into()));

        let sut = MfaAuthenticateServiceImpl {
            hash,
            mfa_disable,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_eq!(result.unwrap(), MfaAuthenticateResult::Reset);
    }

    #[tokio::test]
    async fn ok_totp() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: Some("123456".try_into().unwrap()),
            recovery_code: None,
        };

        let secret =
            TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

        let totp = MockTotpService::new().with_check(
            cmd.totp_code.clone().unwrap(),
            secret.clone(),
            Ok(()),
        );

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![secret]);

        let sut = MfaAuthenticateServiceImpl {
            totp,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_eq!(result.unwrap(), MfaAuthenticateResult::Ok);
    }

    #[tokio::test]
    async fn failed_no_authentication() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: None,
            recovery_code: None,
        };

        let secret =
            TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![secret]);

        let sut = MfaAuthenticateServiceImpl {
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_matches!(result, Err(MfaAuthenticateError::Failed));
    }

    #[tokio::test]
    async fn failed_no_recovery_code() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: None,
            recovery_code: Some("PJVURV-QRK3YJ-O3U7T6-D50KAC".try_into().unwrap()),
        };

        let secret =
            TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![secret])
            .with_get_mfa_recovery_code_hash(FOO.user.id, None);

        let sut = MfaAuthenticateServiceImpl {
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_matches!(result, Err(MfaAuthenticateError::Failed));
    }

    #[tokio::test]
    async fn failed_invalid_recovery_code() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: None,
            recovery_code: Some("PJVURV-QRK3YJ-O3U7T6-D50KAC".try_into().unwrap()),
        };

        let secret =
            TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

        let hash =
            MockHashService::new().with_sha256(cmd.recovery_code.clone().unwrap(), *SHA256HASH1);

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![secret])
            .with_get_mfa_recovery_code_hash(FOO.user.id, Some((*SHA256HASH2).into()));

        let sut = MfaAuthenticateServiceImpl {
            hash,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_matches!(result, Err(MfaAuthenticateError::Failed));
    }

    #[tokio::test]
    async fn failed_invalid_totp_code() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: Some("123456".try_into().unwrap()),
            recovery_code: None,
        };

        let secret =
            TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

        let totp = MockTotpService::new().with_check(
            cmd.totp_code.clone().unwrap(),
            secret.clone(),
            Err(TotpCheckError::InvalidCode),
        );

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![secret]);

        let sut = MfaAuthenticateServiceImpl {
            totp,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_matches!(result, Err(MfaAuthenticateError::Failed));
    }

    #[tokio::test]
    async fn failed_totp_code_recently_used() {
        // Arrange
        let cmd = MfaAuthentication {
            totp_code: Some("123456".try_into().unwrap()),
            recovery_code: None,
        };

        let secret =
            TotpSecret::try_new("IZ6GJPVVwQWfRhQTuxwrdBfn".to_owned().into_bytes()).unwrap();

        let totp = MockTotpService::new().with_check(
            cmd.totp_code.clone().unwrap(),
            secret.clone(),
            Err(TotpCheckError::RecentlyUsed),
        );

        let mfa_repo = MockMfaRepository::new()
            .with_list_enabled_totp_device_secrets_by_user(FOO.user.id, vec![secret]);

        let sut = MfaAuthenticateServiceImpl {
            totp,
            mfa_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.authenticate(&mut (), FOO.user.id, cmd).await;

        // Assert
        assert_matches!(result, Err(MfaAuthenticateError::Failed));
    }
}
