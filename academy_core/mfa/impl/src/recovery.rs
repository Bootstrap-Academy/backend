use academy_core_mfa_contracts::recovery::MfaRecoveryService;
use academy_di::Build;
use academy_models::{mfa::MfaRecoveryCode, user::UserId};
use academy_persistence_contracts::mfa::MfaRepository;
use academy_shared_contracts::{hash::HashService, secret::SecretService};
use academy_utils::trace_instrument;
use anyhow::Context;

#[derive(Debug, Clone, Build)]
pub struct MfaRecoveryServiceImpl<Secret, Hash, MfaRepo> {
    secret: Secret,
    hash: Hash,
    mfa_repo: MfaRepo,
}

impl<Txn, Secret, Hash, MfaRepo> MfaRecoveryService<Txn>
    for MfaRecoveryServiceImpl<Secret, Hash, MfaRepo>
where
    Txn: Send + Sync + 'static,
    Secret: SecretService,
    Hash: HashService,
    MfaRepo: MfaRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn setup(&self, txn: &mut Txn, user_id: UserId) -> anyhow::Result<MfaRecoveryCode> {
        let recovery_code = self.secret.generate_mfa_recovery_code();

        let hash = self.hash.sha256(&recovery_code).into();
        self.mfa_repo
            .save_mfa_recovery_code_hash(txn, user_id, hash)
            .await
            .context("Failed to save MFA recovery code hash in database")?;

        Ok(recovery_code)
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::{SHA256HASH1, user::FOO};
    use academy_persistence_contracts::mfa::MockMfaRepository;
    use academy_shared_contracts::{hash::MockHashService, secret::MockSecretService};

    use super::*;

    #[tokio::test]
    async fn setup() {
        // Arrange
        let expected = MfaRecoveryCode::try_new("PJVURV-QRK3YJ-O3U7T6-D50KAC").unwrap();

        let secret = MockSecretService::new().with_generate_mfa_recovery_code(expected.clone());

        let hash = MockHashService::new().with_sha256(expected.clone(), *SHA256HASH1);

        let mfa_repo = MockMfaRepository::new()
            .with_save_mfa_recovery_code_hash(FOO.user.id, (*SHA256HASH1).into());

        let sut = MfaRecoveryServiceImpl {
            secret,
            hash,
            mfa_repo,
        };

        // Act
        let result = sut.setup(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}
