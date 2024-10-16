use std::{collections::HashMap, sync::Arc, time::Duration};

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_oauth2_contracts::{
    link::{OAuth2LinkService, OAuth2LinkServiceError},
    login::{OAuth2LoginService, OAuth2LoginServiceError},
    registration::OAuth2RegistrationService,
    OAuth2CreateLinkError, OAuth2CreateSessionError, OAuth2CreateSessionResponse,
    OAuth2DeleteLinkError, OAuth2FeatureService, OAuth2ListLinksError,
};
use academy_core_session_contracts::session::SessionService;
use academy_di::Build;
use academy_extern_contracts::oauth2::OAuth2ApiService;
use academy_models::{
    oauth2::{
        OAuth2Link, OAuth2LinkId, OAuth2Login, OAuth2Provider, OAuth2ProviderId,
        OAuth2ProviderSummary, OAuth2Registration,
    },
    session::DeviceName,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    oauth2::OAuth2Repository, user::UserRepository, Database, Transaction,
};

pub mod link;
pub mod login;
pub mod registration;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct OAuth2FeatureServiceImpl<
    Db,
    Auth,
    OAuth2Api,
    UserRepo,
    OAuth2Repo,
    OAuth2Link,
    OAuth2Login,
    OAuth2Registration,
    Session,
> {
    db: Db,
    auth: Auth,
    oauth2_api: OAuth2Api,
    user_repo: UserRepo,
    oauth2_repo: OAuth2Repo,
    oauth2_create_link: OAuth2Link,
    oauth2_login: OAuth2Login,
    oauth2_registration: OAuth2Registration,
    session: Session,
    config: OAuth2FeatureConfig,
}

#[derive(Debug, Clone)]
pub struct OAuth2FeatureConfig {
    pub providers: Arc<HashMap<OAuth2ProviderId, OAuth2Provider>>,
    pub registration_token_ttl: Duration,
}

impl<
        Db,
        Auth,
        OAuth2Api,
        UserRepo,
        OAuth2Repo,
        OAuth2LinkS,
        OAuth2LoginS,
        OAuth2RegistrationS,
        Session,
    > OAuth2FeatureService
    for OAuth2FeatureServiceImpl<
        Db,
        Auth,
        OAuth2Api,
        UserRepo,
        OAuth2Repo,
        OAuth2LinkS,
        OAuth2LoginS,
        OAuth2RegistrationS,
        Session,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    OAuth2Api: OAuth2ApiService,
    UserRepo: UserRepository<Db::Transaction>,
    OAuth2Repo: OAuth2Repository<Db::Transaction>,
    OAuth2LinkS: OAuth2LinkService<Db::Transaction>,
    OAuth2LoginS: OAuth2LoginService,
    OAuth2RegistrationS: OAuth2RegistrationService,
    Session: SessionService<Db::Transaction>,
{
    fn list_providers(&self) -> Vec<OAuth2ProviderSummary> {
        self.config
            .providers
            .iter()
            .map(|(id, provider)| OAuth2ProviderSummary {
                id: id.clone(),
                name: provider.name.clone(),
                auth_url: self.oauth2_api.generate_auth_url(provider),
            })
            .collect()
    }

    async fn list_links(
        &self,
        token: &str,
        user_id: UserIdOrSelf,
    ) -> Result<Vec<OAuth2Link>, OAuth2ListLinksError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(OAuth2ListLinksError::NotFound);
        }

        let mut links = self
            .oauth2_repo
            .list_links_by_user(&mut txn, user_id)
            .await?;

        // include only links with valid providers
        links.retain(|link| self.config.providers.contains_key(&link.provider_id));

        Ok(links)
    }

    async fn create_link(
        &self,
        token: &str,
        user_id: UserIdOrSelf,
        login: OAuth2Login,
    ) -> Result<OAuth2Link, OAuth2CreateLinkError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(OAuth2CreateLinkError::NotFound);
        }

        let provider_id = login.provider_id.clone();

        let user_info = self
            .oauth2_login
            .login(login)
            .await
            .map_err(|err| match err {
                OAuth2LoginServiceError::InvalidProvider => OAuth2CreateLinkError::InvalidProvider,
                OAuth2LoginServiceError::InvalidCode => OAuth2CreateLinkError::InvalidCode,
                OAuth2LoginServiceError::Other(err) => err.into(),
            })?;

        let link = self
            .oauth2_create_link
            .create(&mut txn, user_id, provider_id, user_info)
            .await
            .map_err(|err| match err {
                OAuth2LinkServiceError::RemoteAlreadyLinked => {
                    OAuth2CreateLinkError::RemoteAlreadyLinked
                }
                OAuth2LinkServiceError::Other(err) => err.into(),
            })?;

        txn.commit().await?;

        Ok(link)
    }

    async fn delete_link(
        &self,
        token: &str,
        user_id: UserIdOrSelf,
        link_id: OAuth2LinkId,
    ) -> Result<(), OAuth2DeleteLinkError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let link = self
            .oauth2_repo
            .get_link(&mut txn, link_id)
            .await?
            .filter(|link| link.user_id == user_id)
            .ok_or(OAuth2DeleteLinkError::NotFound)?;

        self.oauth2_repo.delete_link(&mut txn, link.id).await?;

        // ensure the user can still login
        let user_composite = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await?
            .ok_or(OAuth2DeleteLinkError::NotFound)?;
        if !user_composite.details.password_login && !user_composite.details.oauth2_login {
            return Err(OAuth2DeleteLinkError::CannotRemoveLink);
        }

        txn.commit().await?;

        Ok(())
    }

    async fn create_session(
        &self,
        login: OAuth2Login,
        device_name: Option<DeviceName>,
    ) -> Result<OAuth2CreateSessionResponse, OAuth2CreateSessionError> {
        let provider_id = login.provider_id.clone();
        let user_info = self
            .oauth2_login
            .login(login)
            .await
            .map_err(|err| match err {
                OAuth2LoginServiceError::InvalidProvider => {
                    OAuth2CreateSessionError::InvalidProvider
                }
                OAuth2LoginServiceError::InvalidCode => OAuth2CreateSessionError::InvalidCode,
                OAuth2LoginServiceError::Other(err) => err.into(),
            })?;

        let mut txn = self.db.begin_transaction().await?;

        let Some(user_composite) = self
            .user_repo
            .get_composite_by_oauth2_provider_id_and_remote_user_id(
                &mut txn,
                &provider_id,
                &user_info.id,
            )
            .await?
        else {
            // there is no local user linked to this remote user, so we save the provider id
            // and remote user and return a registration token which can be used to create a
            // new user account which will be automatically linked to this remote user
            let registration_token = self
                .oauth2_registration
                .save(&OAuth2Registration {
                    provider_id,
                    remote_user: user_info,
                })
                .await?;

            return Ok(OAuth2CreateSessionResponse::RegistrationToken(
                registration_token,
            ));
        };

        if !user_composite.user.enabled {
            return Err(OAuth2CreateSessionError::UserDisabled);
        }

        let login = self
            .session
            .create(&mut txn, user_composite, device_name, true)
            .await?;

        txn.commit().await?;

        Ok(OAuth2CreateSessionResponse::Login(login.into()))
    }
}
