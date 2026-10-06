use std::time::Duration;

use academy_auth_contracts::{AuthResultExt, AuthService, internal::AuthInternalService};
use academy_core_user_contracts::publication::{PublicationError, PublicationFeatureService};
use academy_di::Build;
use academy_models::{
    auth::{AccessToken, InternalToken},
    publication::{
        NOTICE, NOTICE_HASH, PublicationChoice, PublicationChoiceResult, PublicationConfig,
        PublicationEpoch, PublicationPreview, PublicationPreviewClaims, PublicationSettings,
        PublicationSnapshot, PublicationWithdrawal, PublishedIdentity, SCOPE_VERSION,
    },
    user::UserId,
};
use academy_persistence_contracts::{
    Database, Transaction,
    publication::{PublicationRepository, PublicationWriteError},
    session::SessionRepository,
    user::UserRepository,
};
use academy_shared_contracts::jwt::JwtService;

#[derive(Debug, Clone, Build)]
pub struct PublicationFeatureServiceImpl<Db, Auth, AuthInternal, Jwt, Repo, UserRepo, SessionRepo> {
    db: Db,
    auth: Auth,
    auth_internal: AuthInternal,
    jwt: Jwt,
    repo: Repo,
    user_repo: UserRepo,
    session_repo: SessionRepo,
    config: PublicationConfig,
}

fn publication_error(error: PublicationWriteError) -> PublicationError {
    match error {
        PublicationWriteError::Disabled => PublicationError::Disabled,
        PublicationWriteError::NotFound => PublicationError::NotFound,
        PublicationWriteError::Conflict => PublicationError::Conflict,
        PublicationWriteError::InvalidPreview => PublicationError::InvalidPreview,
        PublicationWriteError::Unverified => PublicationError::Unverified,
        PublicationWriteError::Other(error) => PublicationError::Other(error),
    }
}

impl<Db, Auth, AuthInternal, Jwt, Repo, UserRepo, SessionRepo> PublicationFeatureService
    for PublicationFeatureServiceImpl<Db, Auth, AuthInternal, Jwt, Repo, UserRepo, SessionRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    AuthInternal: AuthInternalService,
    Jwt: JwtService,
    Repo: PublicationRepository<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
    SessionRepo: SessionRepository<Db::Transaction>,
{
    async fn support_settings(
        &self,
        token: &AccessToken,
        user_id: UserId,
    ) -> Result<PublicationSettings, PublicationError> {
        let mut txn = self.db.begin_transaction().await?;
        let auth = self
            .auth
            .authenticate_in_transaction(&mut txn, token)
            .await
            .map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;
        if !self
            .repo
            .epoch(&mut txn, self.config.enabled)
            .await?
            .publishing_enabled
        {
            return Err(PublicationError::Disabled);
        }
        // Purpose-only retained subjects are outside the ordinary account support view.
        self.user_repo
            .get_composite(&mut txn, user_id)
            .await?
            .ok_or(PublicationError::NotFound)?;
        let settings = self
            .repo
            .settings(&mut txn, user_id)
            .await?
            .ok_or(PublicationError::NotFound)?;
        txn.commit().await?;
        Ok(settings)
    }

    async fn support_withdraw(
        &self,
        token: &AccessToken,
        user_id: UserId,
        withdrawal: PublicationWithdrawal,
    ) -> Result<PublicationChoiceResult, PublicationError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;
        let mut txn = self.db.begin_transaction().await?;
        // Stable owner order also handles two administrators supporting each other.
        let mut owners = vec![auth.user_id, user_id];
        owners.sort_unstable();
        owners.dedup();
        for owner in owners {
            if !self.user_repo.lock_account(&mut txn, owner).await? {
                return Err(PublicationError::NotFound);
            }
        }
        // A queued request must not retain removed admin/MFA/session authority.
        let current_auth = self
            .auth
            .authenticate_in_transaction(&mut txn, token)
            .await
            .map_auth_err()?;
        current_auth.ensure_admin().map_auth_err()?;
        academy_persistence_contracts::session::ensure_owner_sign_in(
            &self.session_repo,
            &mut txn,
            current_auth.session_id,
        )
        .await?;
        self.user_repo
            .get_composite(&mut txn, user_id)
            .await?
            .ok_or(PublicationError::NotFound)?;
        let result = self
            .repo
            .withdraw(&mut txn, user_id, &withdrawal, self.config.enabled)
            .await
            .map_err(publication_error)?;
        txn.commit().await?;
        Ok(result)
    }

    async fn settings(&self, token: &AccessToken) -> Result<PublicationSettings, PublicationError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let mut txn = self.db.begin_transaction().await?;
        if !self
            .repo
            .epoch(&mut txn, self.config.enabled)
            .await?
            .publishing_enabled
        {
            return Err(PublicationError::Disabled);
        }
        self.repo
            .settings(&mut txn, auth.user_id)
            .await?
            .ok_or(PublicationError::NotFound)
    }

    async fn preview(&self, token: &AccessToken) -> Result<PublicationPreview, PublicationError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let mut txn = self.db.begin_transaction().await?;
        if !self
            .repo
            .epoch(&mut txn, self.config.enabled)
            .await?
            .publishing_enabled
        {
            return Err(PublicationError::Disabled);
        }
        let publication = self
            .repo
            .settings(&mut txn, auth.user_id)
            .await?
            .ok_or(PublicationError::NotFound)?;
        let user = self
            .user_repo
            .get_composite(&mut txn, auth.user_id)
            .await?
            .ok_or(PublicationError::NotFound)?;
        let claims = PublicationPreviewClaims {
            purpose: "profile-publication-preview".into(),
            user_id: auth.user_id,
            visibility_revision: publication.visibility_revision,
            scope_version: SCOPE_VERSION.into(),
            notice_hash: NOTICE_HASH.into(),
        };
        let preview_token = self.jwt.sign(claims, Duration::from_secs(600))?;
        Ok(PublicationPreview {
            profile: PublishedIdentity {
                user_id: auth.user_id,
                display_name: user.profile.display_name,
                avatar_url: None,
                visibility_revision: publication.visibility_revision,
            },
            publication,
            scope_version: SCOPE_VERSION.into(),
            notice_hash: NOTICE_HASH.into(),
            notice: NOTICE.into(),
            preview_token,
        })
    }

    async fn choose(
        &self,
        token: &AccessToken,
        choice: PublicationChoice,
    ) -> Result<PublicationChoiceResult, PublicationError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let mut txn = self.db.begin_transaction().await?;
        if !self.user_repo.lock_account(&mut txn, auth.user_id).await? {
            return Err(PublicationError::NotFound);
        }
        // Reset may have revoked this session while the request waited for the
        // account lock. Recheck durable authority before any choice or replay.
        let auth = self
            .auth
            .authenticate_in_transaction(&mut txn, token)
            .await
            .map_auth_err()?;
        // Sharing, making private again and confirming the notice are the
        // owner's own decisions, recorded as such. A session someone else
        // opened in the account never makes them, however often it is
        // refreshed; support withdraws through its own route instead.
        let owner_sign_in = self
            .session_repo
            .get(&mut txn, auth.session_id)
            .await?
            .is_some_and(|session| session.is_owner_sign_in());
        if !owner_sign_in {
            return Err(PublicationError::NotOwnerSignIn);
        }
        let preview_valid = choice
            .preview_token
            .as_deref()
            .and_then(|token| self.jwt.verify::<PublicationPreviewClaims>(token).ok())
            .is_some_and(|claims| {
                claims.purpose == "profile-publication-preview"
                    && claims.user_id == auth.user_id
                    && claims.visibility_revision == choice.expected_revision
                    && claims.scope_version == SCOPE_VERSION
                    && claims.notice_hash == NOTICE_HASH
            });
        let result = self
            .repo
            .choose(
                &mut txn,
                auth.user_id,
                &choice,
                self.config.enabled,
                preview_valid,
            )
            .await
            .map_err(publication_error)?;
        txn.commit().await?;
        Ok(result)
    }

    async fn epoch(&self, token: &InternalToken) -> Result<PublicationEpoch, PublicationError> {
        self.auth_internal
            .authenticate(token, "auth")
            .map_err(|_| PublicationError::InternalAuth)?;
        let mut txn = self.db.begin_transaction().await?;
        let epoch = self.repo.epoch(&mut txn, self.config.enabled).await?;
        txn.commit().await?;
        Ok(epoch)
    }

    async fn snapshot(
        &self,
        token: &InternalToken,
    ) -> Result<PublicationSnapshot, PublicationError> {
        self.auth_internal
            .authenticate(token, "auth")
            .map_err(|_| PublicationError::InternalAuth)?;
        let mut txn = self.db.begin_transaction().await?;
        let snapshot = self.repo.snapshot(&mut txn, self.config.enabled).await?;
        txn.commit().await?;
        Ok(snapshot)
    }
}
