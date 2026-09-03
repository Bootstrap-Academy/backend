use std::{collections::HashMap, sync::Arc, time::Duration};

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_oauth2_contracts::{
    OAuth2BeginAuthorizationError, OAuth2CreateLinkError, OAuth2CreateSessionError,
    OAuth2CreateSessionResponse, OAuth2DeleteLinkError, OAuth2FeatureService, OAuth2ListLinksError,
    authorization::{OAuth2AuthorizationService, OAuth2AuthorizationServiceError},
    link::{OAuth2LinkService, OAuth2LinkServiceError},
    login::{OAuth2LoginService, OAuth2LoginServiceError},
    registration::OAuth2RegistrationService,
};
use academy_core_session_contracts::session::SessionService;
use academy_di::Build;
use academy_models::{
    auth::AccessToken,
    oauth2::{
        OAuth2AuthorizationUrl, OAuth2Callback, OAuth2Link, OAuth2LinkId, OAuth2Login,
        OAuth2Provider, OAuth2ProviderId, OAuth2ProviderSummary, OAuth2Registration,
        OAuth2UserInfo,
    },
    session::DeviceName,
    url::Url,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    Database, Transaction, oauth2::OAuth2Repository, user::UserRepository,
};
use academy_utils::trace_instrument;
use anyhow::Context;

pub mod authorization;
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
    UserRepo,
    OAuth2Repo,
    OAuth2Link,
    OAuth2Authorization,
    OAuth2Login,
    OAuth2Registration,
    Session,
> {
    db: Db,
    auth: Auth,
    user_repo: UserRepo,
    oauth2_repo: OAuth2Repo,
    oauth2_create_link: OAuth2Link,
    oauth2_authorization: OAuth2Authorization,
    oauth2_login: OAuth2Login,
    oauth2_registration: OAuth2Registration,
    session: Session,
    config: OAuth2FeatureConfig,
}

#[derive(Debug, Clone)]
pub struct OAuth2FeatureConfig {
    pub providers: Arc<HashMap<OAuth2ProviderId, OAuth2Provider>>,
    pub registration_token_ttl: Duration,
    /// How long a started authorization flow stays redeemable. Only has to
    /// cover the round trip through the provider's consent screen.
    pub authorization_ttl: Duration,
}

impl<
    Db,
    Auth,
    UserRepo,
    OAuth2Repo,
    OAuth2LinkS,
    OAuth2AuthorizationS,
    OAuth2LoginS,
    OAuth2RegistrationS,
    Session,
> OAuth2FeatureService
    for OAuth2FeatureServiceImpl<
        Db,
        Auth,
        UserRepo,
        OAuth2Repo,
        OAuth2LinkS,
        OAuth2AuthorizationS,
        OAuth2LoginS,
        OAuth2RegistrationS,
        Session,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
    OAuth2Repo: OAuth2Repository<Db::Transaction>,
    OAuth2LinkS: OAuth2LinkService<Db::Transaction>,
    OAuth2AuthorizationS: OAuth2AuthorizationService,
    OAuth2LoginS: OAuth2LoginService,
    OAuth2RegistrationS: OAuth2RegistrationService,
    Session: SessionService<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    fn list_providers(&self) -> Vec<OAuth2ProviderSummary> {
        self.config
            .providers
            .iter()
            .map(|(id, provider)| OAuth2ProviderSummary {
                id: id.clone(),
                name: provider.name.clone(),
            })
            .collect()
    }

    #[trace_instrument(skip(self))]
    async fn begin_authorization(
        &self,
        provider_id: OAuth2ProviderId,
        redirect_uri: Url,
    ) -> Result<OAuth2AuthorizationUrl, OAuth2BeginAuthorizationError> {
        self.oauth2_authorization
            .begin(provider_id, redirect_uri)
            .await
            .map_err(|err| match err {
                OAuth2AuthorizationServiceError::InvalidProvider => {
                    OAuth2BeginAuthorizationError::InvalidProvider
                }
                OAuth2AuthorizationServiceError::Other(err) => {
                    err.context("Failed to begin OAuth2 authorization").into()
                }
            })
    }

    #[trace_instrument(skip(self))]
    async fn list_links(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<Vec<OAuth2Link>, OAuth2ListLinksError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self
            .user_repo
            .exists(&mut txn, user_id)
            .await
            .context("Failed to check user existence")?
        {
            return Err(OAuth2ListLinksError::NotFound);
        }

        let mut links = self
            .oauth2_repo
            .list_links_by_user(&mut txn, user_id)
            .await
            .context("Failed to get OAuth2 links from database")?;

        // include only links with valid providers
        links.retain(|link| self.config.providers.contains_key(&link.provider_id));

        Ok(links)
    }

    #[trace_instrument(skip(self))]
    async fn create_link(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        callback: OAuth2Callback,
    ) -> Result<OAuth2Link, OAuth2CreateLinkError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self
            .user_repo
            .exists(&mut txn, user_id)
            .await
            .context("Failed to check user existence")?
        {
            return Err(OAuth2CreateLinkError::NotFound);
        }

        let (provider_id, user_info) =
            resolve_callback(&self.oauth2_authorization, &self.oauth2_login, callback)
                .await
                .map_err(|err| match err {
                    ResolveCallbackError::InvalidState => OAuth2CreateLinkError::InvalidState,
                    ResolveCallbackError::InvalidProvider => OAuth2CreateLinkError::InvalidProvider,
                    ResolveCallbackError::InvalidCode => OAuth2CreateLinkError::InvalidCode,
                    ResolveCallbackError::Other(err) => err.into(),
                })?;

        let link = self
            .oauth2_create_link
            .create(&mut txn, user_id, provider_id, user_info)
            .await
            .map_err(|err| match err {
                OAuth2LinkServiceError::RemoteAlreadyLinked => {
                    OAuth2CreateLinkError::RemoteAlreadyLinked
                }
                OAuth2LinkServiceError::Other(err) => {
                    err.context("Failed to create OAuth2 link").into()
                }
            })?;

        txn.commit().await?;

        Ok(link)
    }

    #[trace_instrument(skip(self))]
    async fn delete_link(
        &self,
        token: &AccessToken,
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
            .await
            .context("Failed to get OAuth2 link from database")?
            .filter(|link| link.user_id == user_id)
            .ok_or(OAuth2DeleteLinkError::NotFound)?;

        self.oauth2_repo
            .delete_link(&mut txn, link.id)
            .await
            .context("Failed to delete OAuth2 link from database")?;

        // ensure the user can still login
        let user_composite = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(OAuth2DeleteLinkError::NotFound)?;
        if !user_composite.details.password_login && !user_composite.details.oauth2_login {
            txn.rollback().await?;
            return Err(OAuth2DeleteLinkError::CannotRemoveLink);
        }

        txn.commit().await?;

        Ok(())
    }

    #[trace_instrument(skip(self))]
    async fn create_session(
        &self,
        callback: OAuth2Callback,
        device_name: Option<DeviceName>,
    ) -> Result<OAuth2CreateSessionResponse, OAuth2CreateSessionError> {
        let (provider_id, user_info) =
            resolve_callback(&self.oauth2_authorization, &self.oauth2_login, callback)
                .await
                .map_err(|err| match err {
                    ResolveCallbackError::InvalidState => OAuth2CreateSessionError::InvalidState,
                    ResolveCallbackError::InvalidProvider => {
                        OAuth2CreateSessionError::InvalidProvider
                    }
                    ResolveCallbackError::InvalidCode => OAuth2CreateSessionError::InvalidCode,
                    ResolveCallbackError::Other(err) => err.into(),
                })?;

        let mut txn = self.db.begin_transaction().await?;

        let Some(user_composite) = self
            .user_repo
            .get_composite_by_oauth2_provider_id_and_remote_user_id(
                &mut txn,
                &provider_id,
                &user_info.id,
            )
            .await
            .context("Failed to get user from database")?
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
                .await
                .context("Failed to save OAuth2 registration")?;

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
            .await
            .context("Failed to create session")?;

        txn.commit().await?;

        Ok(OAuth2CreateSessionResponse::Login(login.into()))
    }
}

/// Redeem the `state` of an OAuth2 callback and exchange the authorization code
/// it came with.
///
/// The `state` decides which provider and redirect URI the exchange uses, so a
/// callback can neither be replayed nor pointed at a different provider than
/// the flow it belongs to.
async fn resolve_callback(
    authorization: &impl OAuth2AuthorizationService,
    login: &impl OAuth2LoginService,
    callback: OAuth2Callback,
) -> Result<(OAuth2ProviderId, OAuth2UserInfo), ResolveCallbackError> {
    let pending = authorization
        .consume(&callback.state)
        .await
        .context("Failed to get OAuth2 authorization")?
        .ok_or(ResolveCallbackError::InvalidState)?;

    let provider_id = pending.provider_id.clone();

    let user_info = login
        .login(OAuth2Login {
            provider_id: pending.provider_id,
            code: callback.code,
            redirect_uri: pending.redirect_uri,
            code_verifier: pending.code_verifier,
        })
        .await
        .map_err(|err| match err {
            OAuth2LoginServiceError::InvalidProvider => ResolveCallbackError::InvalidProvider,
            OAuth2LoginServiceError::InvalidCode => ResolveCallbackError::InvalidCode,
            OAuth2LoginServiceError::Other(err) => {
                ResolveCallbackError::Other(err.context("Failed to perform OAuth2 login"))
            }
        })?;

    Ok((provider_id, user_info))
}

enum ResolveCallbackError {
    InvalidState,
    InvalidProvider,
    InvalidCode,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for ResolveCallbackError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}
