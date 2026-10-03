use academy_cache_contracts::CacheService;
use academy_core_user_contracts::email_confirmation::{
    UserEmailConfirmationResetPasswordError, UserEmailConfirmationService,
    UserEmailConfirmationVerifyEmailError,
};
use academy_di::Build;
use academy_email_contracts::template::TemplateEmailService;
use academy_models::{
    VerificationCode,
    email_address::{EmailAddress, EmailAddressWithName},
    user::{UserComposite, UserId, UserPassword, UserPatchRef},
};
use academy_persistence_contracts::user::UserRepository;
use academy_shared_contracts::{password::PasswordService, secret::SecretService};
use academy_templates_contracts::{ResetPasswordTemplate, VerifyEmailTemplate};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow};

use crate::UserFeatureConfig;

// Versioned keys reject unbound codes issued by the previous implementation.
type VerificationTarget = (UserId, EmailAddress);
type ResetAuthorization = (EmailAddress, VerificationCode);

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct UserEmailConfirmationServiceImpl<Secret, TemplateEmail, Cache, Password, UserRepo> {
    secret: Secret,
    template_email: TemplateEmail,
    cache: Cache,
    password: Password,
    user_repo: UserRepo,
    config: UserFeatureConfig,
}

impl<Txn, Secret, TemplateEmail, Cache, Password, UserRepo> UserEmailConfirmationService<Txn>
    for UserEmailConfirmationServiceImpl<Secret, TemplateEmail, Cache, Password, UserRepo>
where
    Txn: Send + Sync + 'static,
    Secret: SecretService,
    TemplateEmail: TemplateEmailService,
    Cache: CacheService,
    Password: PasswordService,
    UserRepo: UserRepository<Txn>,
{
    #[trace_instrument(skip(self, email))]
    async fn request_verification(
        &self,
        user_id: UserId,
        email: EmailAddressWithName,
    ) -> anyhow::Result<()> {
        let code = self.secret.generate_verification_code();

        self.cache
            .set(
                &verification_cache_key(&code),
                &(user_id, email.clone().into_email_address()),
                Some(self.config.verification_verification_code_ttl),
            )
            .await
            .context("Failed to save code in cache")?;

        self.template_email
            .send_verification_email(
                email,
                &VerifyEmailTemplate {
                    code: code.into_inner(),
                    url: (*self.config.verification_redirect_url).clone(),
                },
            )
            .await
            .context("Failed to send email")?;

        Ok(())
    }

    #[trace_instrument(skip(self, txn, verification_code))]
    async fn verify_email(
        &self,
        txn: &mut Txn,
        verification_code: &VerificationCode,
    ) -> Result<UserComposite, UserEmailConfirmationVerifyEmailError> {
        let cache_key = verification_cache_key(verification_code);
        let (user_id, email): VerificationTarget = self
            .cache
            .pop(&cache_key)
            .await
            .context("Failed to consume verification code")?
            .ok_or(UserEmailConfirmationVerifyEmailError::InvalidCode)?;

        if !self.user_repo.lock_account(txn, user_id).await? {
            return Err(UserEmailConfirmationVerifyEmailError::InvalidCode);
        }
        let mut user_composite = self
            .user_repo
            .get_composite(txn, user_id)
            .await?
            .filter(|user| user.user.email.as_ref() == Some(&email))
            .ok_or(UserEmailConfirmationVerifyEmailError::InvalidCode)?;

        if user_composite.user.email_verified {
            return Err(UserEmailConfirmationVerifyEmailError::AlreadyVerified);
        }

        user_composite.user.email_verified = true;
        self.user_repo
            .update(
                txn,
                user_composite.user.id,
                UserPatchRef::new().update_email_verified(&true),
            )
            .await
            .map_err(|err| anyhow!(err).context("Failed to update user in database"))?;

        Ok(user_composite)
    }

    #[trace_instrument(skip(self, email))]
    async fn request_password_reset(
        &self,
        user_id: UserId,
        email: EmailAddressWithName,
    ) -> anyhow::Result<()> {
        let code = self.secret.generate_verification_code();

        self.cache
            .set(
                &reset_password_cache_key(user_id),
                &(email.clone().into_email_address(), code.clone()),
                Some(self.config.password_reset_verification_code_ttl),
            )
            .await
            .context("Failed to save code in cache")?;

        self.template_email
            .send_reset_password_email(
                email,
                &ResetPasswordTemplate {
                    code: code.into_inner(),
                    url: (*self.config.password_reset_redirect_url).clone(),
                },
            )
            .await
            .context("Failed to send email")?;

        Ok(())
    }

    #[trace_instrument(skip(self, txn, code, new_password))]
    async fn reset_password(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        code: VerificationCode,
        new_password: UserPassword,
    ) -> Result<(), UserEmailConfirmationResetPasswordError> {
        let cache_key = reset_password_cache_key(user_id);

        if !self.user_repo.lock_account(txn, user_id).await? {
            return Err(UserEmailConfirmationResetPasswordError::InvalidCode);
        }
        let user = self
            .user_repo
            .get_composite(txn, user_id)
            .await?
            .ok_or(UserEmailConfirmationResetPasswordError::InvalidCode)?;
        let authorization: ResetAuthorization = self
            .cache
            .get(&cache_key)
            .await?
            .filter(|(email, expected_code)| {
                user.user.email.as_ref() == Some(email) && expected_code == &code
            })
            .ok_or(UserEmailConfirmationResetPasswordError::InvalidCode)?;
        // GETDEL is atomic; expiry or replacement between checking and consuming
        // fails closed. An incorrect code does not consume a valid one.
        if self.cache.pop::<ResetAuthorization>(&cache_key).await? != Some(authorization) {
            return Err(UserEmailConfirmationResetPasswordError::InvalidCode);
        }

        let password_hash = self
            .password
            .hash(new_password.into_inner().into())
            .await
            .context("Failed to hash password")?;

        self.user_repo
            .save_password_hash(txn, user_id, password_hash)
            .await
            .context("Failed to save password hash in database")?;

        Ok(())
    }
}

fn verification_cache_key(verification_code: &VerificationCode) -> String {
    format!("verification:v2:{}", **verification_code)
}

pub(crate) fn reset_password_cache_key(user_id: UserId) -> String {
    format!("reset_password_code:v2:{}", user_id.hyphenated())
}

#[cfg(test)]
mod tests {
    use academy_cache_contracts::MockCacheService;
    use academy_demo::{
        VERIFICATION_CODE_1, VERIFICATION_CODE_2,
        user::{FOO, FOO_PASSWORD},
    };
    use academy_email_contracts::template::MockTemplateEmailService;
    use academy_models::user::UserPatch;
    use academy_persistence_contracts::user::MockUserRepository;
    use academy_shared_contracts::{password::MockPasswordService, secret::MockSecretService};
    use academy_utils::{Apply, assert_matches};

    use super::*;

    type Sut = UserEmailConfirmationServiceImpl<
        MockSecretService,
        MockTemplateEmailService,
        MockCacheService,
        MockPasswordService,
        MockUserRepository<()>,
    >;

    #[tokio::test]
    async fn request_verification() {
        // Arrange
        let config = UserFeatureConfig::default();

        let recipient = FOO
            .user
            .email
            .clone()
            .unwrap()
            .with_name(FOO.profile.display_name.clone().into_inner());

        let secret =
            MockSecretService::new().with_generate_verification_code(VERIFICATION_CODE_1.clone());

        let template_email = MockTemplateEmailService::new().with_send_verification_email(
            recipient.clone(),
            VerifyEmailTemplate {
                code: VERIFICATION_CODE_1.clone().into_inner(),
                url: (*config.verification_redirect_url).clone(),
            },
            true,
        );

        let cache = MockCacheService::new().with_set(
            format!("verification:v2:{}", **VERIFICATION_CODE_1),
            (FOO.user.id, FOO.user.email.clone().unwrap()),
            Some(config.verification_verification_code_ttl),
        );

        let sut = UserEmailConfirmationServiceImpl {
            secret,
            template_email,
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut.request_verification(FOO.user.id, recipient).await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn verify_email_ok() {
        // Arrange
        let cache_key = format!("verification:v2:{}", **VERIFICATION_CODE_1);
        let cache = MockCacheService::new().with_pop(
            cache_key.clone(),
            Some((FOO.user.id, FOO.user.email.clone().unwrap())),
        );

        let user_repo = MockUserRepository::new()
            .with_lock_account(FOO.user.id, true)
            .with_get_composite(
                FOO.user.id,
                Some(FOO.clone().with(|u| u.user.email_verified = false)),
            )
            .with_update(
                FOO.user.id,
                UserPatch::new().update_email_verified(true),
                Ok(true),
            );

        let sut = UserEmailConfirmationServiceImpl {
            cache,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.verify_email(&mut (), &VERIFICATION_CODE_1).await;

        // Assert
        assert_eq!(result.unwrap(), *FOO);
    }

    #[tokio::test]
    async fn verify_email_invalid_code() {
        // Arrange
        let cache = MockCacheService::new().with_pop(
            format!("verification:v2:{}", **VERIFICATION_CODE_1),
            None::<VerificationTarget>,
        );

        let user_repo = MockUserRepository::new();

        let sut = UserEmailConfirmationServiceImpl {
            cache,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.verify_email(&mut (), &VERIFICATION_CODE_1).await;

        // Assert
        assert_matches!(
            result,
            Err(UserEmailConfirmationVerifyEmailError::InvalidCode)
        );
    }

    #[tokio::test]
    async fn verify_email_user_not_found() {
        // Arrange
        let cache = MockCacheService::new().with_pop(
            format!("verification:v2:{}", **VERIFICATION_CODE_1),
            Some((FOO.user.id, FOO.user.email.clone().unwrap())),
        );

        let user_repo = MockUserRepository::new()
            .with_lock_account(FOO.user.id, true)
            .with_get_composite(FOO.user.id, None);

        let sut = UserEmailConfirmationServiceImpl {
            cache,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.verify_email(&mut (), &VERIFICATION_CODE_1).await;

        // Assert
        assert_matches!(
            result,
            Err(UserEmailConfirmationVerifyEmailError::InvalidCode)
        );
    }

    #[tokio::test]
    async fn verify_email_already_verified() {
        // Arrange
        let cache_key = format!("verification:v2:{}", **VERIFICATION_CODE_1);
        let cache = MockCacheService::new().with_pop(
            cache_key.clone(),
            Some((FOO.user.id, FOO.user.email.clone().unwrap())),
        );

        let user_repo = MockUserRepository::new()
            .with_lock_account(FOO.user.id, true)
            .with_get_composite(FOO.user.id, Some(FOO.clone()));

        let sut = UserEmailConfirmationServiceImpl {
            cache,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.verify_email(&mut (), &VERIFICATION_CODE_1).await;

        // Assert
        assert_matches!(
            result,
            Err(UserEmailConfirmationVerifyEmailError::AlreadyVerified)
        );
    }

    #[tokio::test]
    async fn request_password_reset() {
        // Arrange
        let config = UserFeatureConfig::default();

        let secret =
            MockSecretService::new().with_generate_verification_code(VERIFICATION_CODE_1.clone());

        let expected_email = ResetPasswordTemplate {
            code: VERIFICATION_CODE_1.clone().into_inner(),
            url: (*config.password_reset_redirect_url).clone(),
        };

        let template_email = MockTemplateEmailService::new().with_send_reset_password_email(
            FOO.user
                .email
                .clone()
                .unwrap()
                .with_name(FOO.profile.display_name.clone().into_inner()),
            expected_email,
            true,
        );

        let cache = MockCacheService::new().with_set(
            format!("reset_password_code:v2:{}", FOO.user.id.hyphenated()),
            (FOO.user.email.clone().unwrap(), VERIFICATION_CODE_1.clone()),
            Some(config.password_reset_verification_code_ttl),
        );

        let sut = UserEmailConfirmationServiceImpl {
            secret,
            template_email,
            cache,
            ..Sut::default()
        };

        // Act
        let result = sut
            .request_password_reset(
                FOO.user.id,
                FOO.user
                    .email
                    .clone()
                    .unwrap()
                    .with_name(FOO.profile.display_name.clone().into_inner()),
            )
            .await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn reset_password_ok() {
        // Arrange
        let cache_key = format!("reset_password_code:v2:{}", FOO.user.id.hyphenated());
        let cache = MockCacheService::new()
            .with_get(
                cache_key.clone(),
                Some((FOO.user.email.clone().unwrap(), VERIFICATION_CODE_1.clone())),
            )
            .with_pop(
                cache_key,
                Some((FOO.user.email.clone().unwrap(), VERIFICATION_CODE_1.clone())),
            );

        let password = MockPasswordService::new()
            .with_hash(FOO_PASSWORD.clone().into_inner(), "new pw hash".into());

        let user_repo = MockUserRepository::new()
            .with_lock_account(FOO.user.id, true)
            .with_get_composite(FOO.user.id, Some(FOO.clone()))
            .with_save_password_hash(FOO.user.id, "new pw hash".into());

        let sut = UserEmailConfirmationServiceImpl {
            cache,
            password,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .reset_password(
                &mut (),
                FOO.user.id,
                VERIFICATION_CODE_1.clone(),
                FOO_PASSWORD.clone(),
            )
            .await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn reset_password_no_code() {
        // Arrange
        let cache = MockCacheService::new().with_get(
            format!("reset_password_code:v2:{}", FOO.user.id.hyphenated()),
            None::<ResetAuthorization>,
        );

        let sut = UserEmailConfirmationServiceImpl {
            cache,
            user_repo: MockUserRepository::new()
                .with_lock_account(FOO.user.id, true)
                .with_get_composite(FOO.user.id, Some(FOO.clone())),
            ..Sut::default()
        };

        // Act
        let result = sut
            .reset_password(
                &mut (),
                FOO.user.id,
                VERIFICATION_CODE_1.clone(),
                FOO_PASSWORD.clone(),
            )
            .await;

        // Assert
        assert_matches!(
            result,
            Err(UserEmailConfirmationResetPasswordError::InvalidCode)
        );
    }

    #[tokio::test]
    async fn reset_password_invalid_code() {
        // Arrange
        let cache = MockCacheService::new().with_get(
            format!("reset_password_code:v2:{}", FOO.user.id.hyphenated()),
            Some((FOO.user.email.clone().unwrap(), VERIFICATION_CODE_2.clone())),
        );

        let sut = UserEmailConfirmationServiceImpl {
            cache,
            user_repo: MockUserRepository::new()
                .with_lock_account(FOO.user.id, true)
                .with_get_composite(FOO.user.id, Some(FOO.clone())),
            ..Sut::default()
        };

        // Act
        let result = sut
            .reset_password(
                &mut (),
                FOO.user.id,
                VERIFICATION_CODE_1.clone(),
                FOO_PASSWORD.clone(),
            )
            .await;

        // Assert
        assert_matches!(
            result,
            Err(UserEmailConfirmationResetPasswordError::InvalidCode)
        );
    }
}
