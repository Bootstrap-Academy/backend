use academy_auth_contracts::AuthService;
use academy_core_session_contracts::session::SessionService;
use academy_core_user_contracts::update::{
    UserUpdateEmailError, UserUpdateNameError, UserUpdateNameRateLimitPolicy, UserUpdateService,
};
use academy_di::Build;
use academy_models::{
    email_address::EmailAddress,
    user::{
        User, UserId, UserInvoiceInfo, UserInvoiceInfoPatch, UserName, UserPassword, UserPatch,
        UserPatchRef,
    },
};
use academy_persistence_contracts::user::{UserRepoError, UserRepository};
use academy_shared_contracts::{password::PasswordService, time::TimeService};
use academy_utils::{
    patch::{Patch, PatchValue},
    trace_instrument,
};
use anyhow::{anyhow, Context};

use crate::UserFeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct UserUpdateServiceImpl<Auth, Time, Password, Session, UserRepo> {
    auth: Auth,
    time: Time,
    password: Password,
    session: Session,
    user_repo: UserRepo,
    config: UserFeatureConfig,
}

impl<Txn, Auth, Time, Password, Session, UserRepo> UserUpdateService<Txn>
    for UserUpdateServiceImpl<Auth, Time, Password, Session, UserRepo>
where
    Txn: Send + Sync + 'static,
    Auth: AuthService<Txn>,
    Time: TimeService,
    Password: PasswordService,
    Session: SessionService<Txn>,
    UserRepo: UserRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn update_name(
        &self,
        txn: &mut Txn,
        user: User,
        name: UserName,
        rate_limit_policy: UserUpdateNameRateLimitPolicy,
    ) -> Result<User, UserUpdateNameError> {
        let last_name_change = match rate_limit_policy {
            UserUpdateNameRateLimitPolicy::Enforce => {
                let now = self.time.now();
                if let Some(last_name_change) = user.last_name_change {
                    let rate_limit_until = last_name_change + self.config.name_change_rate_limit;
                    if now < rate_limit_until {
                        return Err(UserUpdateNameError::RateLimit {
                            until: rate_limit_until,
                        });
                    }
                }
                PatchValue::Update(Some(now))
            }
            UserUpdateNameRateLimitPolicy::Bypass => PatchValue::Unchanged,
        };

        let patch = UserPatch {
            name: name.into(),
            last_name_change,
            ..Default::default()
        };

        self.user_repo
            .update(txn, user.id, patch.as_ref())
            .await
            .map(|_| user.update(patch))
            .map_err(|err| match err {
                UserRepoError::NameConflict => UserUpdateNameError::Conflict,
                err => anyhow!(err)
                    .context("Failed to update user in database")
                    .into(),
            })
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_email(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        email: &Option<EmailAddress>,
        email_verified: bool,
    ) -> Result<bool, UserUpdateEmailError> {
        let result = self
            .user_repo
            .update(
                txn,
                user_id,
                UserPatchRef::new()
                    .update_email(email)
                    .update_email_verified(&email_verified),
            )
            .await
            .map_err(|err| match err {
                UserRepoError::EmailConflict => UserUpdateEmailError::Conflict,
                err => anyhow!(err)
                    .context("Failed to update user in database")
                    .into(),
            })?;

        if result {
            // access tokens contain the `email_verified` field, so we need to invalidate
            // them when changing this value
            self.auth
                .invalidate_access_tokens(txn, user_id)
                .await
                .context("Failed to invalidate access tokens")?;
        }

        Ok(result)
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_password(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        password: UserPassword,
    ) -> anyhow::Result<()> {
        let hash = self
            .password
            .hash(password.into_inner().into())
            .await
            .context("Failed to hash password")?;

        self.user_repo
            .save_password_hash(txn, user_id, hash)
            .await
            .context("Failed to save password hash in database")?;

        Ok(())
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_enabled(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        enabled: bool,
    ) -> anyhow::Result<bool> {
        if !enabled {
            self.session
                .delete_by_user(txn, user_id)
                .await
                .context("Failed to log out user")?;
        }

        self.user_repo
            .update(txn, user_id, UserPatchRef::new().update_enabled(&enabled))
            .await
            .context("Failed to update user in database")
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_admin(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        admin: bool,
    ) -> anyhow::Result<bool> {
        // access tokens contain the `admin` field, so we need to invalidate
        // them when changing this value
        self.auth
            .invalidate_access_tokens(txn, user_id)
            .await
            .context("Failed to invalidate access tokens")?;

        self.user_repo
            .update(txn, user_id, UserPatchRef::new().update_admin(&admin))
            .await
            .context("Failed to update user in database")
    }

    #[trace_instrument(skip(self, txn))]
    async fn update_invoice_info(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        invoice_info: UserInvoiceInfo,
        mut patch: UserInvoiceInfoPatch,
    ) -> anyhow::Result<UserInvoiceInfo> {
        if patch.business.update_or(invoice_info.business) == Some(false) {
            patch.vat_id = PatchValue::Update(None).minimize(&invoice_info.vat_id);
        }

        self.user_repo
            .update_invoice_info(txn, user_id, patch.as_ref())
            .await
            .context("Failed to update user in database")?;

        Ok(invoice_info.update(patch))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use academy_auth_contracts::MockAuthService;
    use academy_core_session_contracts::session::MockSessionService;
    use academy_demo::user::{ADMIN, BAR, FOO};
    use academy_models::user::UserPatch;
    use academy_persistence_contracts::user::MockUserRepository;
    use academy_shared_contracts::{password::MockPasswordService, time::MockTimeService};
    use academy_utils::{assert_matches, patch::Patch, Apply};

    use super::*;

    type Sut = UserUpdateServiceImpl<
        MockAuthService<()>,
        MockTimeService,
        MockPasswordService,
        MockSessionService<()>,
        MockUserRepository<()>,
    >;

    #[tokio::test]
    async fn update_name_ok_rate_limit() {
        // Arrange
        let config = UserFeatureConfig::default();

        let now = FOO.user.last_name_change.unwrap()
            + config.name_change_rate_limit
            + Duration::from_secs(2);

        let expected = User {
            name: BAR.user.name.clone(),
            last_name_change: Some(now),
            ..FOO.user.clone()
        };

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new()
                .update_name(BAR.user.name.clone())
                .update_last_name_change(Some(now)),
            Ok(true),
        );

        let sut = UserUpdateServiceImpl {
            time,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .update_name(
                &mut (),
                FOO.user.clone(),
                BAR.user.name.clone(),
                UserUpdateNameRateLimitPolicy::Enforce,
            )
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn update_name_ok_bypass_rate_limit() {
        // Arrange
        let expected = User {
            name: BAR.user.name.clone(),
            ..FOO.user.clone()
        };

        let time = MockTimeService::new();

        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new().update_name(BAR.user.name.clone()),
            Ok(true),
        );

        let sut = UserUpdateServiceImpl {
            time,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .update_name(
                &mut (),
                FOO.user.clone(),
                BAR.user.name.clone(),
                UserUpdateNameRateLimitPolicy::Bypass,
            )
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn update_name_rate_limit() {
        // Arrange
        let config = UserFeatureConfig::default();

        let time = MockTimeService::new().with_now(
            FOO.user.last_name_change.unwrap() + config.name_change_rate_limit
                - Duration::from_secs(2),
        );

        let user_repo = MockUserRepository::new();

        let sut = UserUpdateServiceImpl {
            time,
            user_repo,
            ..Sut::default()
        };

        let expected = FOO.user.last_name_change.unwrap() + config.name_change_rate_limit;

        // Act
        let result = sut
            .update_name(
                &mut (),
                FOO.user.clone(),
                BAR.user.name.clone(),
                UserUpdateNameRateLimitPolicy::Enforce,
            )
            .await;

        // Assert
        assert_matches!(result, Err(UserUpdateNameError::RateLimit { until }) if *until == expected);
    }

    #[tokio::test]
    async fn update_name_conflict() {
        // Arrange
        let time = MockTimeService::new();

        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new().update_name(BAR.user.name.clone()),
            Err(UserRepoError::NameConflict),
        );

        let sut = UserUpdateServiceImpl {
            time,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .update_name(
                &mut (),
                FOO.user.clone(),
                BAR.user.name.clone(),
                UserUpdateNameRateLimitPolicy::Bypass,
            )
            .await;

        // Assert
        assert_matches!(result, Err(UserUpdateNameError::Conflict));
    }

    #[tokio::test]
    async fn update_email_ok() {
        for verified in [true, false] {
            // Arrange
            let auth = MockAuthService::new().with_invalidate_access_tokens(FOO.user.id);

            let user_repo = MockUserRepository::new().with_update(
                FOO.user.id,
                UserPatch::new()
                    .update_email(Some(ADMIN.user.email.clone().unwrap()))
                    .update_email_verified(verified),
                Ok(true),
            );

            let sut = UserUpdateServiceImpl {
                auth,
                user_repo,
                ..Sut::default()
            };

            // Act
            let result = sut
                .update_email(
                    &mut (),
                    FOO.user.id,
                    &ADMIN.user.email.clone().unwrap().into(),
                    verified,
                )
                .await;

            // Assert
            assert!(result.unwrap());
        }
    }

    #[tokio::test]
    async fn update_email_conflict() {
        // Arrange
        let auth = MockAuthService::new();

        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new()
                .update_email(Some(ADMIN.user.email.clone().unwrap()))
                .update_email_verified(false),
            Err(UserRepoError::EmailConflict),
        );

        let sut = UserUpdateServiceImpl {
            auth,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .update_email(
                &mut (),
                FOO.user.id,
                &ADMIN.user.email.clone().unwrap().into(),
                false,
            )
            .await;

        // Assert
        assert_matches!(result, Err(UserUpdateEmailError::Conflict));
    }

    #[tokio::test]
    async fn update_password() {
        // Arrange
        let password =
            MockPasswordService::new().with_hash("new password".into(), "the hash".into());

        let user_repo =
            MockUserRepository::new().with_save_password_hash(FOO.user.id, "the hash".into());

        let sut = UserUpdateServiceImpl {
            password,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .update_password(&mut (), FOO.user.id, "new password".try_into().unwrap())
            .await;

        // Assert
        result.unwrap();
    }

    #[tokio::test]
    async fn update_enabled_enable() {
        // Arrange
        let user_repo = MockUserRepository::new().with_update(
            BAR.user.id,
            UserPatch::new().update_enabled(true),
            Ok(true),
        );

        let sut = UserUpdateServiceImpl {
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.update_enabled(&mut (), BAR.user.id, true).await;

        // Act
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn update_enabled_disable() {
        // Arrange
        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new().update_enabled(false),
            Ok(true),
        );

        let session = MockSessionService::new().with_delete_by_user(FOO.user.id);

        let sut = UserUpdateServiceImpl {
            user_repo,
            session,
            ..Sut::default()
        };

        // Act
        let result = sut.update_enabled(&mut (), FOO.user.id, false).await;

        // Act
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn update_admin_promote() {
        // Arrange
        let auth = MockAuthService::new().with_invalidate_access_tokens(FOO.user.id);

        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new().update_admin(true),
            Ok(true),
        );

        let sut = UserUpdateServiceImpl {
            auth,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.update_admin(&mut (), FOO.user.id, true).await;

        // Assert
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn update_admin_demote() {
        // Arrange
        let auth = MockAuthService::new().with_invalidate_access_tokens(FOO.user.id);

        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new().update_admin(false),
            Ok(true),
        );

        let sut = UserUpdateServiceImpl {
            auth,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.update_admin(&mut (), FOO.user.id, false).await;

        // Assert
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn update_invoice_info_ok() {
        // Arrange
        let expected = FOO.invoice_info.clone();
        let patch = FOO.invoice_info.clone().into_patch();

        let user_repo =
            MockUserRepository::new().with_update_invoice_info(BAR.user.id, patch.clone(), true);

        let sut = UserUpdateServiceImpl {
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .update_invoice_info(&mut (), BAR.user.id, BAR.invoice_info.clone(), patch)
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn update_invoice_info_reset_vat_id_if_not_business() {
        // Arrange
        let expected = FOO.invoice_info.clone().with(|u| {
            u.business = Some(false);
            u.vat_id = None;
        });
        let patch = UserInvoiceInfoPatch::new().update_business(Some(false));

        let user_repo = MockUserRepository::new().with_update_invoice_info(
            FOO.user.id,
            patch.clone().update_vat_id(None),
            true,
        );

        let sut = UserUpdateServiceImpl {
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .update_invoice_info(&mut (), FOO.user.id, FOO.invoice_info.clone(), patch)
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}
