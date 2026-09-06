use academy_auth_contracts::AuthService;
use academy_core_mfa_contracts::disable::MfaDisableService;
use academy_di::Build;
use academy_models::user::UserId;
use academy_persistence_contracts::{mfa::MfaRepository, session::SessionRepository};
use academy_utils::trace_instrument;
use anyhow::Context;
use tracing::trace;

#[derive(Debug, Clone, Build)]
pub struct MfaDisableServiceImpl<Auth, MfaRepo, SessionRepo> {
    auth: Auth,
    mfa_repo: MfaRepo,
    session_repo: SessionRepo,
}

impl<Txn, Auth, MfaRepo, SessionRepo> MfaDisableService<Txn>
    for MfaDisableServiceImpl<Auth, MfaRepo, SessionRepo>
where
    Txn: Send + Sync + 'static,
    Auth: AuthService<Txn>,
    MfaRepo: MfaRepository<Txn>,
    SessionRepo: SessionRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn disable(&self, txn: &mut Txn, user_id: UserId) -> anyhow::Result<()> {
        trace!("delete totp devices");
        self.mfa_repo
            .delete_totp_devices_by_user(txn, user_id)
            .await
            .context("Failed to delete totp devices from database")?;

        trace!("delete recovery code");
        self.mfa_repo
            .delete_mfa_recovery_code_hash(txn, user_id)
            .await
            .context("Failed to delete MFA recovery code hash from database")?;

        // Administrative authority is granted to a session that was
        // authenticated with the second factor, so it has to end with the
        // second factor. Without this a session stayed `mfa_verified` for the
        // whole `session.refresh_token_ttl`, and removing an administrator's
        // authenticator did not reduce that.
        trace!("clear mfa_verified on the sessions of the user");
        self.session_repo
            .clear_mfa_verified_by_user(txn, user_id)
            .await
            .context("Failed to clear mfa_verified in database")?;

        // The access token carries `mfa_verified` as well, so it has to be
        // reissued; the next refresh reads the cleared value from the session.
        self.auth
            .invalidate_access_tokens(txn, user_id)
            .await
            .context("Failed to invalidate access tokens")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use academy_auth_contracts::MockAuthService;
    use academy_demo::user::FOO;
    use academy_persistence_contracts::{mfa::MockMfaRepository, session::MockSessionRepository};

    use super::*;

    #[tokio::test]
    async fn ok() {
        // Arrange
        let mfa_repo = MockMfaRepository::new()
            .with_delete_totp_devices_by_user(FOO.user.id)
            .with_delete_mfa_recovery_code_hash(FOO.user.id);

        // The second factor is gone, so no session of this account keeps the
        // authority it granted.
        let session_repo =
            MockSessionRepository::new().with_clear_mfa_verified_by_user(FOO.user.id);

        let auth = MockAuthService::new().with_invalidate_access_tokens(FOO.user.id);

        let sut = MfaDisableServiceImpl {
            auth,
            mfa_repo,
            session_repo,
        };

        // Act
        let result = sut.disable(&mut (), FOO.user.id).await;

        // Assert
        result.unwrap();
    }
}
