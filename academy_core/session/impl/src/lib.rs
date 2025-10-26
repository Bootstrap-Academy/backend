use academy_auth_contracts::{
    AuthResultExt, AuthService, AuthenticateByPasswordError, AuthenticateByRefreshTokenError,
};
use academy_core_mfa_contracts::authenticate::{
    MfaAuthenticateError, MfaAuthenticateResult, MfaAuthenticateService,
};
use academy_core_session_contracts::{
    ActiveUsersGranularity, ActiveUsersRange, SessionActiveUsersError, SessionCreateCommand,
    SessionCreateError, SessionDeleteByUserError, SessionDeleteCurrentError, SessionDeleteError,
    SessionFeatureService, SessionGetCurrentError, SessionImpersonateError, SessionListByUserError,
    SessionRefreshError, failed_auth_count::SessionFailedAuthCountService, session::SessionService,
};
use academy_di::Build;
use academy_models::{
    RecaptchaResponse,
    auth::{AccessToken, Login, RefreshToken},
    session::{ActiveUsersBucket, Session, SessionId},
    user::{UserId, UserIdOrSelf, UserNameOrEmailAddress},
};
use academy_persistence_contracts::{
    Database, Transaction, session::SessionRepository, user::UserRepository,
};
use academy_shared_contracts::captcha::{CaptchaCheckError, CaptchaService};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow};

pub mod failed_auth_count;
pub mod session;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct SessionFeatureServiceImpl<
    Db,
    Auth,
    Captcha,
    Session,
    SessionFailedAuthCount,
    MfaAuthenticate,
    UserRepo,
    SessionRepo,
> {
    db: Db,
    auth: Auth,
    captcha: Captcha,
    session: Session,
    session_failed_auth_count: SessionFailedAuthCount,
    mfa_authenticate: MfaAuthenticate,
    user_repo: UserRepo,
    session_repo: SessionRepo,
    config: SessionFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct SessionFeatureConfig {
    pub login_fails_before_captcha: u64,
}

impl<Db, Auth, Captcha, SessionS, SessionFailedAuthCount, MfaAuthenticate, UserRepo, SessionRepo>
    SessionFeatureService
    for SessionFeatureServiceImpl<
        Db,
        Auth,
        Captcha,
        SessionS,
        SessionFailedAuthCount,
        MfaAuthenticate,
        UserRepo,
        SessionRepo,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Captcha: CaptchaService,
    SessionS: SessionService<Db::Transaction>,
    SessionFailedAuthCount: SessionFailedAuthCountService,
    MfaAuthenticate: MfaAuthenticateService<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
    SessionRepo: SessionRepository<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn get_current_session(
        &self,
        token: &AccessToken,
    ) -> Result<Session, SessionGetCurrentError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        self.session_repo
            .get(&mut txn, auth.session_id)
            .await?
            .ok_or_else(|| anyhow!("Failed to get authenticated session").into())
    }

    #[trace_instrument(skip(self))]
    async fn list_by_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<Vec<Session>, SessionListByUserError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        self.session_repo
            .list_by_user(&mut txn, user_id)
            .await
            .context("Failed to get sessions from database")
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self))]
    async fn create_session(
        &self,
        cmd: SessionCreateCommand,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> Result<Login, SessionCreateError> {
        let failed_login_attempts = self
            .session_failed_auth_count
            .get(&cmd.name_or_email)
            .await
            .context("Failed to get failed auth count")?;

        if failed_login_attempts >= self.config.login_fails_before_captcha {
            self.captcha
                .check(recaptcha_response.as_deref().map(String::as_str))
                .await
                .map_err(|err| match err {
                    CaptchaCheckError::Failed => SessionCreateError::Recaptcha,
                    CaptchaCheckError::Other(err) => err.context("Failed to check captcha").into(),
                })?;
        }

        let mut txn = self.db.begin_transaction().await?;

        let mut user_composite = match self
            .user_repo
            .get_composite_by_name_or_email(&mut txn, &cmd.name_or_email)
            .await
            .context("Failed to get user from database")?
        {
            Some(user_composite) => user_composite,
            None => {
                self.session_failed_auth_count
                    .increment(&cmd.name_or_email)
                    .await
                    .context("Failed to increment failed auth count")?;
                return Err(SessionCreateError::InvalidCredentials);
            }
        };

        let increment_failed_login_attempts = || async {
            self.session_failed_auth_count
                .increment(&UserNameOrEmailAddress::Name(
                    user_composite.user.name.clone(),
                ))
                .await
                .context("Failed to increment failed auth count for name")?;
            if let Some(email) = user_composite.user.email.clone() {
                self.session_failed_auth_count
                    .increment(&UserNameOrEmailAddress::Email(email))
                    .await
                    .context("Failed to increment failed auth count for email")?;
            }
            anyhow::Ok(())
        };

        match self
            .auth
            .authenticate_by_password(&mut txn, user_composite.user.id, cmd.password)
            .await
        {
            Ok(()) => {}
            Err(AuthenticateByPasswordError::InvalidCredentials) => {
                increment_failed_login_attempts().await?;
                return Err(SessionCreateError::InvalidCredentials);
            }
            Err(AuthenticateByPasswordError::Other(err)) => {
                return Err(err
                    .context("Failed to perform password authentication")
                    .into());
            }
        };

        if user_composite.details.mfa_enabled {
            match self
                .mfa_authenticate
                .authenticate(&mut txn, user_composite.user.id, cmd.mfa)
                .await
            {
                Ok(MfaAuthenticateResult::Ok | MfaAuthenticateResult::Disabled) => (),
                Ok(MfaAuthenticateResult::Reset) => user_composite.details.mfa_enabled = false,
                Err(MfaAuthenticateError::Failed) => {
                    increment_failed_login_attempts().await?;
                    return Err(SessionCreateError::MfaFailed);
                }
                Err(MfaAuthenticateError::Other(err)) => {
                    return Err(err.context("Failed to perform MFA").into());
                }
            }
        }

        self.session_failed_auth_count
            .reset(&UserNameOrEmailAddress::Name(
                user_composite.user.name.clone(),
            ))
            .await
            .context("Failed to reset failed auth count for name")?;
        if let Some(email) = user_composite.user.email.clone() {
            self.session_failed_auth_count
                .reset(&UserNameOrEmailAddress::Email(email))
                .await
                .context("Failed to reset failed auth count for email")?;
        }

        if !user_composite.user.enabled {
            return Err(SessionCreateError::UserDisabled);
        }

        let login = self
            .session
            .create(&mut txn, user_composite, cmd.device_name, true)
            .await
            .context("Failed to create session")?;

        txn.commit().await?;

        Ok(login)
    }

    #[trace_instrument(skip(self))]
    async fn impersonate(
        &self,
        token: &AccessToken,
        user_id: UserId,
    ) -> Result<Login, SessionImpersonateError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let user_composite = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(SessionImpersonateError::NotFound)?;

        let login = self
            .session
            .create(&mut txn, user_composite, None, false)
            .await
            .context("Failed to create session")?;

        txn.commit().await?;

        Ok(login)
    }

    #[trace_instrument(skip(self))]
    async fn refresh_session(
        &self,
        refresh_token: &RefreshToken,
    ) -> Result<Login, SessionRefreshError> {
        let mut txn = self.db.begin_transaction().await?;

        let session_id = match self
            .auth
            .authenticate_by_refresh_token(&mut txn, refresh_token)
            .await
        {
            Ok(session_id) => session_id,
            Err(AuthenticateByRefreshTokenError::Invalid) => {
                return Err(SessionRefreshError::InvalidRefreshToken);
            }
            Err(AuthenticateByRefreshTokenError::Expired(session_id)) => {
                self.session
                    .delete(&mut txn, session_id)
                    .await
                    .context("Failed to delete expired session")?;
                return Err(SessionRefreshError::InvalidRefreshToken);
            }
            Err(AuthenticateByRefreshTokenError::Other(err)) => {
                return Err(err
                    .context("Failed to authenticate by refresh token")
                    .into());
            }
        };

        let login = self
            .session
            .refresh(&mut txn, session_id)
            .await
            .map_err(|err| {
                use academy_core_session_contracts::session::SessionRefreshError as E;
                match err {
                    E::NotFound => SessionRefreshError::InvalidRefreshToken,
                    E::Other(err) => err.context("Failed to refresh session").into(),
                }
            })?;

        txn.commit().await?;

        Ok(login)
    }

    #[trace_instrument(skip(self))]
    async fn delete_session(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        session_id: SessionId,
    ) -> Result<(), SessionDeleteError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let session = self
            .session_repo
            .get(&mut txn, session_id)
            .await
            .context("Failed to get session from database")?
            .filter(|s| s.user_id == user_id)
            .ok_or(SessionDeleteError::NotFound)?;

        self.session
            .delete(&mut txn, session.id)
            .await
            .context("Failed to delete session")?;

        txn.commit().await?;

        Ok(())
    }

    #[trace_instrument(skip(self))]
    async fn delete_current_session(
        &self,
        token: &AccessToken,
    ) -> Result<(), SessionDeleteCurrentError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        self.session
            .delete(&mut txn, auth.session_id)
            .await
            .context("Failed to delete session")?;

        txn.commit().await?;

        Ok(())
    }

    #[trace_instrument(skip(self))]
    async fn delete_by_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<(), SessionDeleteByUserError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        self.session
            .delete_by_user(&mut txn, user_id)
            .await
            .context("Failed to delete sessions")?;

        txn.commit().await?;

        Ok(())
    }

    #[trace_instrument(skip(self))]
    async fn active_users(
        &self,
        token: &AccessToken,
        range: ActiveUsersRange,
        granularity: ActiveUsersGranularity,
    ) -> Result<Vec<ActiveUsersBucket>, SessionActiveUsersError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;
        let buckets = self
            .session
            .active_users(&mut txn, range, granularity)
            .await
            .context("Failed to load active user statistics")?;
        txn.commit().await?;

        Ok(buckets)
    }
}
