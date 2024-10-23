use std::{sync::Arc, time::Duration};

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_oauth2_contracts::registration::OAuth2RegistrationService;
use academy_core_session_contracts::session::SessionService;
use academy_core_user_contracts::{
    email_confirmation::{
        UserEmailConfirmationResetPasswordError, UserEmailConfirmationService,
        UserEmailConfirmationSubscribeToNewsletterError, UserEmailConfirmationVerifyEmailError,
    },
    update::{
        UserUpdateEmailError, UserUpdateNameError, UserUpdateNameRateLimitPolicy, UserUpdateService,
    },
    user::{UserCreateCommand, UserListQuery, UserListResult, UserService},
    PasswordUpdate, UserCreateError, UserCreateRequest, UserDeleteError, UserFeatureService,
    UserGetError, UserListError, UserRequestPasswordResetError, UserRequestVerificationEmailError,
    UserResetPasswordError, UserUpdateError, UserUpdateRequest, UserUpdateUserRequest,
    UserVerifyEmailError, UserVerifyNewsletterSubscriptionError,
};
use academy_di::Build;
use academy_extern_contracts::vat::VatApiService;
use academy_models::{
    auth::{AccessToken, Login},
    email_address::EmailAddress,
    session::DeviceName,
    user::{UserComposite, UserIdOrSelf, UserInvoiceInfoPatch, UserPassword, UserPatchRef},
    RecaptchaResponse, VerificationCode,
};
use academy_persistence_contracts::{
    coin::CoinRepository, user::UserRepository, Database, Transaction,
};
use academy_shared_contracts::captcha::{CaptchaCheckError, CaptchaService};
use academy_utils::{
    patch::{Patch, PatchValue},
    trace_instrument,
};
use anyhow::{anyhow, Context};

pub mod email_confirmation;
pub mod update;
pub mod user;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default, Build)]
pub struct UserFeatureServiceImpl<
    Db,
    Auth,
    Captcha,
    VatApi,
    User,
    UserEmailConfirmation,
    UserUpdate,
    Session,
    OAuth2Registration,
    UserRepo,
    CoinRepo,
> {
    db: Db,
    auth: Auth,
    captcha: Captcha,
    vat_api: VatApi,
    user: User,
    user_email_confirmation: UserEmailConfirmation,
    user_update: UserUpdate,
    session: Session,
    oauth2_registration: OAuth2Registration,
    user_repo: UserRepo,
    coin_repo: CoinRepo,
}

#[derive(Debug, Clone)]
pub struct UserFeatureConfig {
    pub name_change_rate_limit: Duration,
    pub verification_redirect_url: Arc<String>,
    pub verification_verification_code_ttl: Duration,
    pub password_reset_redirect_url: Arc<String>,
    pub password_reset_verification_code_ttl: Duration,
    pub newsletter_subscription_redirect_url: Arc<String>,
    pub newsletter_subscription_verification_code_ttl: Duration,
}

impl<
        Db,
        Auth,
        Captcha,
        VatApi,
        UserS,
        UserEmailConfirmation,
        UserUpdate,
        Session,
        OAuth2RegistrationS,
        UserRepo,
        CoinRepo,
    > UserFeatureService
    for UserFeatureServiceImpl<
        Db,
        Auth,
        Captcha,
        VatApi,
        UserS,
        UserEmailConfirmation,
        UserUpdate,
        Session,
        OAuth2RegistrationS,
        UserRepo,
        CoinRepo,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Captcha: CaptchaService,
    VatApi: VatApiService,
    UserS: UserService<Db::Transaction>,
    UserEmailConfirmation: UserEmailConfirmationService<Db::Transaction>,
    UserUpdate: UserUpdateService<Db::Transaction>,
    Session: SessionService<Db::Transaction>,
    OAuth2RegistrationS: OAuth2RegistrationService,
    UserRepo: UserRepository<Db::Transaction>,
    CoinRepo: CoinRepository<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn list_users(
        &self,
        token: &AccessToken,
        query: UserListQuery,
    ) -> Result<UserListResult, UserListError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await.unwrap();

        self.user
            .list(&mut txn, query)
            .await
            .context("Failed to list users")
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self))]
    async fn get_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<UserComposite, UserGetError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await.unwrap();

        self.user_repo
            .get_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(UserGetError::NotFound)
    }

    #[trace_instrument(skip(self))]
    async fn create_user(
        &self,
        request: UserCreateRequest,
        device_name: Option<DeviceName>,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> Result<Login, UserCreateError> {
        if request.password.is_none() && request.oauth2_registration_token.is_none() {
            return Err(UserCreateError::NoLoginMethod);
        }

        self.captcha
            .check(recaptcha_response.as_deref().map(String::as_str))
            .await
            .map_err(|err| match err {
                CaptchaCheckError::Failed => UserCreateError::Recaptcha,
                CaptchaCheckError::Other(err) => err.context("Failed to check captcha").into(),
            })?;

        let oauth2_registration = match &request.oauth2_registration_token {
            Some(oauth2_registration_token) => Some(
                self.oauth2_registration
                    .get(oauth2_registration_token)
                    .await
                    .context("Failed to get OAuth2 registration")?
                    .ok_or(UserCreateError::InvalidOAuthRegistrationToken)?,
            ),
            None => None,
        };

        let mut txn = self.db.begin_transaction().await.unwrap();

        let cmd = UserCreateCommand {
            name: request.name,
            display_name: request.display_name,
            email: request.email,
            password: request.password,
            admin: false,
            enabled: true,
            email_verified: false,
            oauth2_registration,
        };

        let user = self.user.create(&mut txn, cmd).await.map_err(|err| {
            use academy_core_user_contracts::user::UserCreateError as E;
            match err {
                E::NameConflict => UserCreateError::NameConflict,
                E::EmailConflict => UserCreateError::EmailConflict,
                E::RemoteAlreadyLinked => UserCreateError::RemoteAlreadyLinked,
                E::Other(err) => err.context("Failed to create user").into(),
            }
        })?;

        let result = self
            .session
            .create(&mut txn, user, device_name, true)
            .await
            .context("Failed to create session")?;

        if let Some(oauth2_registration_token) = request.oauth2_registration_token {
            self.oauth2_registration
                .remove(&oauth2_registration_token)
                .await
                .context("Failed to remove OAuth2 registration")?;
        }

        txn.commit().await.unwrap();

        Ok(result)
    }

    #[trace_instrument(skip(self))]
    async fn update_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        UserUpdateRequest {
            user:
                UserUpdateUserRequest {
                    name,
                    email,
                    email_verified,
                    password,
                    enabled,
                    admin,
                    newsletter,
                },
            profile: profile_update,
            invoice_info: invoice_info_update,
        }: UserUpdateRequest,
    ) -> Result<UserComposite, UserUpdateError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        // Fetch current user
        let UserComposite {
            mut user,
            mut profile,
            mut details,
            mut invoice_info,
        } = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(UserUpdateError::NotFound)?;

        let mut commit = false;

        // Minimize patch
        let name = name.minimize(&user.name);
        let email = email.map(Some).minimize(&user.email);
        let email_verified =
            email_verified.minimize(&(user.email_verified && email.is_unchanged()));
        let enabled = enabled.minimize(&user.enabled);
        let admin = admin.minimize(&user.admin);
        let newsletter = newsletter.minimize(&user.newsletter);

        let profile_update = profile_update.minimize(&profile);

        let invoice_info_update = UserInvoiceInfoPatch {
            business: invoice_info_update.business.map(Some).into(),
            first_name: invoice_info_update.first_name.map(Some).into(),
            last_name: invoice_info_update.last_name.map(Some).into(),
            street: invoice_info_update.street.map(Some).into(),
            zip_code: invoice_info_update.zip_code.map(Some).into(),
            city: invoice_info_update.city.map(Some).into(),
            country: invoice_info_update.country.map(Some).into(),
            vat_id: invoice_info_update.vat_id.map(Some).into(),
        }
        .minimize(&invoice_info);

        // Validate patch
        if email_verified.is_update() || enabled.is_update() || admin.is_update() {
            auth.ensure_admin().map_auth_err()?;
        }

        if enabled == PatchValue::Update(false) && user_id == auth.user_id {
            return Err(UserUpdateError::CannotDisableSelf);
        }

        if admin.is_update() && user_id == auth.user_id {
            return Err(UserUpdateError::CannotDemoteSelf);
        }

        if let PatchValue::Update(Some(vat_id)) = &invoice_info_update.vat_id {
            if !self
                .vat_api
                .is_vat_id_valid(vat_id.as_str())
                .await
                .context("Failed to validate VAT id")?
            {
                return Err(UserUpdateError::InvalidVatId);
            }
        }

        // Apply patch
        if profile_update.is_update() {
            self.user_repo
                .update_profile(&mut txn, user_id, profile_update.as_ref())
                .await
                .context("Failed to update user profile")?;
            profile = profile.update(profile_update);
            commit = true;
        }

        if let PatchValue::Update(name) = name {
            let rate_limit_policy = if auth.admin {
                UserUpdateNameRateLimitPolicy::Bypass
            } else {
                UserUpdateNameRateLimitPolicy::Enforce
            };
            user = self
                .user_update
                .update_name(&mut txn, user, name, rate_limit_policy)
                .await
                .map_err(|err| match err {
                    UserUpdateNameError::Conflict => UserUpdateError::NameConflict,
                    UserUpdateNameError::RateLimit { until } => {
                        UserUpdateError::NameChangeRateLimit { until }
                    }
                    UserUpdateNameError::Other(err) => {
                        err.context("Failed to update user name").into()
                    }
                })?;
            commit = true;
        }

        if email.is_update() || email_verified.is_update() {
            user.email_verified =
                email_verified.update(user.email_verified && email.is_unchanged());
            user.email = email.update(user.email);
            self.user_update
                .update_email(&mut txn, user_id, &user.email, user.email_verified)
                .await
                .map_err(|err| match err {
                    UserUpdateEmailError::Conflict => UserUpdateError::EmailConflict,
                    UserUpdateEmailError::Other(err) => {
                        err.context("Failed to update user email").into()
                    }
                })?;
            commit = true;
        }

        if let PatchValue::Update(enabled) = enabled {
            self.user_update
                .update_enabled(&mut txn, user_id, enabled)
                .await
                .context("Failed to update enabled status")?;
            user.enabled = enabled;
            commit = true;
        }

        if let PatchValue::Update(admin) = admin {
            self.user_update
                .update_admin(&mut txn, user_id, admin)
                .await
                .context("Failed to update admin status")?;
            user.admin = admin;
            commit = true;
        }

        match password {
            PatchValue::Update(PasswordUpdate::Remove) => {
                if !details.oauth2_login {
                    return Err(UserUpdateError::CannotRemovePassword);
                }
                self.user_repo
                    .remove_password_hash(&mut txn, user.id)
                    .await
                    .context("Failed to remove password hash from database")?;
                details.password_login = false;
                commit = true;
            }
            PatchValue::Update(PasswordUpdate::Change(password)) => {
                self.user_update
                    .update_password(&mut txn, user_id, password)
                    .await
                    .context("Failed to update user password")?;
                details.password_login = true;
                commit = true;
            }
            PatchValue::Unchanged => (),
        }

        if let PatchValue::Update(newsletter) = newsletter {
            if newsletter && !auth.admin {
                let email = user.email.clone().ok_or(UserUpdateError::NoEmail)?;
                self.user_email_confirmation
                    .request_newsletter_subscription(
                        user_id,
                        email.with_name(profile.display_name.clone().into_inner()),
                    )
                    .await
                    .context("Failed to request newsletter subscription email")?;
            } else {
                user.newsletter = newsletter;
                self.user_repo
                    .update(
                        &mut txn,
                        user_id,
                        UserPatchRef::new().update_newsletter(&newsletter),
                    )
                    .await
                    .map_err(|err| {
                        anyhow!(err).context("Failed to update user newsletter status in database")
                    })?;
                commit = true;
            }
        }

        let invoice_info_updated = invoice_info_update.is_update();
        if invoice_info_updated {
            invoice_info = self
                .user_update
                .update_invoice_info(&mut txn, user.id, invoice_info, invoice_info_update)
                .await
                .context("Failed to update user invoice info")?;
            commit = true;
        }

        let user_composite = UserComposite {
            user,
            profile,
            details,
            invoice_info,
        };

        if invoice_info_updated && user_composite.can_receive_coins() {
            self.coin_repo
                .release_coins(&mut txn, user_id)
                .await
                .context("Failed to release coins")?;
        }

        if commit {
            txn.commit().await?;
        }

        Ok(user_composite)
    }

    #[trace_instrument(skip(self))]
    async fn delete_user(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<(), UserDeleteError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        self.auth
            .invalidate_access_tokens(&mut txn, user_id)
            .await
            .context("Failed to invalidate access tokens")?;

        if !self
            .user_repo
            .delete(&mut txn, user_id)
            .await
            .context("Failed to delete user from database")?
        {
            return Err(UserDeleteError::NotFound);
        }

        txn.commit().await?;

        Ok(())
    }

    #[trace_instrument(skip(self))]
    async fn request_verification_email(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<(), UserRequestVerificationEmailError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let user_composite = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(UserRequestVerificationEmailError::NotFound)?;

        if user_composite.user.email_verified {
            return Err(UserRequestVerificationEmailError::AlreadyVerified);
        }

        let email = user_composite
            .user
            .email
            .ok_or(UserRequestVerificationEmailError::NoEmail)?;

        self.user_email_confirmation
            .request_verification(email.with_name(user_composite.profile.display_name.into_inner()))
            .await
            .context("Failed to request verification email")?;

        Ok(())
    }

    #[trace_instrument(skip(self))]
    async fn verify_email(&self, code: VerificationCode) -> Result<(), UserVerifyEmailError> {
        let mut txn = self.db.begin_transaction().await?;

        match self
            .user_email_confirmation
            .verify_email(&mut txn, &code)
            .await
        {
            Ok(_) => {
                txn.commit().await?;
                Ok(())
            }
            Err(UserEmailConfirmationVerifyEmailError::AlreadyVerified) => Ok(()),
            Err(UserEmailConfirmationVerifyEmailError::InvalidCode) => {
                Err(UserVerifyEmailError::InvalidCode)
            }
            Err(UserEmailConfirmationVerifyEmailError::Other(err)) => {
                Err(err.context("Failed to verify email").into())
            }
        }
    }

    #[trace_instrument(skip(self))]
    async fn verify_newsletter_subscription(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
        code: VerificationCode,
    ) -> Result<UserComposite, UserVerifyNewsletterSubscriptionError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let mut user_composite = self
            .user_repo
            .get_composite(&mut txn, user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(UserVerifyNewsletterSubscriptionError::NotFound)?;

        if user_composite.user.newsletter {
            return Err(UserVerifyNewsletterSubscriptionError::AlreadySubscribed);
        }

        self.user_email_confirmation
            .subscribe_to_newsletter(&mut txn, user_id, code)
            .await
            .map_err(|err| match err {
                UserEmailConfirmationSubscribeToNewsletterError::InvalidCode => {
                    UserVerifyNewsletterSubscriptionError::InvalidCode
                }
                UserEmailConfirmationSubscribeToNewsletterError::Other(err) => err
                    .context("Failed to confirm newsletter subscription")
                    .into(),
            })?;

        user_composite.user.newsletter = true;

        txn.commit().await?;

        Ok(user_composite)
    }

    #[trace_instrument(skip(self))]
    async fn request_password_reset(
        &self,
        email: EmailAddress,
        recaptcha_response: Option<RecaptchaResponse>,
    ) -> Result<(), UserRequestPasswordResetError> {
        self.captcha
            .check(recaptcha_response.as_deref().map(String::as_str))
            .await
            .map_err(|err| match err {
                CaptchaCheckError::Failed => UserRequestPasswordResetError::Recaptcha,
                CaptchaCheckError::Other(err) => err.context("Failed to check captcha").into(),
            })?;

        let mut txn = self.db.begin_transaction().await?;

        if let Some(user_composite) = self
            .user_repo
            .get_composite_by_email(&mut txn, &email)
            .await
            .context("Failed to get user from database")?
        {
            let email = user_composite.user.email.ok_or_else(|| {
                anyhow!(
                    "User {} fetched by email {} has no email address",
                    user_composite.user.id.hyphenated(),
                    email.as_str()
                )
            })?;
            self.user_email_confirmation
                .request_password_reset(
                    user_composite.user.id,
                    email.with_name(user_composite.profile.display_name.into_inner()),
                )
                .await
                .context("Failed to request password reset email")?;
        }

        Ok(())
    }

    #[trace_instrument(skip(self))]
    async fn reset_password(
        &self,
        email: EmailAddress,
        code: VerificationCode,
        new_password: UserPassword,
    ) -> Result<UserComposite, UserResetPasswordError> {
        let mut txn = self.db.begin_transaction().await?;

        let user_composite = self
            .user_repo
            .get_composite_by_email(&mut txn, &email)
            .await
            .context("Failed to get user from database")?
            .ok_or(UserResetPasswordError::Failed)?;

        self.user_email_confirmation
            .reset_password(&mut txn, user_composite.user.id, code, new_password)
            .await
            .map_err(|err| match err {
                UserEmailConfirmationResetPasswordError::InvalidCode => {
                    UserResetPasswordError::Failed
                }
                UserEmailConfirmationResetPasswordError::Other(err) => {
                    err.context("Failed to reset password").into()
                }
            })?;

        txn.commit().await?;

        Ok(user_composite)
    }
}
