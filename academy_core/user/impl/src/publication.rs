use std::time::Duration;

use academy_auth_contracts::{AuthResultExt, AuthService, internal::AuthInternalService};
use academy_core_user_contracts::publication::{PublicationError, PublicationFeatureService};
use academy_di::Build;
use academy_models::{
    auth::{AccessToken, InternalToken},
    publication::{
        NOTICE, NOTICE_HASH, PublicationChoice, PublicationChoiceResult, PublicationConfig,
        PublicationEpoch, PublicationPreview, PublicationPreviewClaims, PublicationSettings,
        PublicationSnapshot, PublishedIdentity, SCOPE_VERSION,
    },
};
use academy_persistence_contracts::{
    Database, Transaction,
    publication::{PublicationRepository, PublicationWriteError},
    user::UserRepository,
};
use academy_shared_contracts::jwt::JwtService;

#[derive(Debug, Clone, Build)]
pub struct PublicationFeatureServiceImpl<Db, Auth, AuthInternal, Jwt, Repo, UserRepo> {
    db: Db,
    auth: Auth,
    auth_internal: AuthInternal,
    jwt: Jwt,
    repo: Repo,
    user_repo: UserRepo,
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

impl<Db, Auth, AuthInternal, Jwt, Repo, UserRepo> PublicationFeatureService
    for PublicationFeatureServiceImpl<Db, Auth, AuthInternal, Jwt, Repo, UserRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    AuthInternal: AuthInternalService,
    Jwt: JwtService,
    Repo: PublicationRepository<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
{
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
