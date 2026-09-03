use academy_core_oauth2_contracts::link::{OAuth2LinkService, OAuth2LinkServiceError};
use academy_core_user_contracts::user::{
    UserCreateCommand, UserCreateError, UserListQuery, UserListResult, UserService,
};
use academy_di::Build;
use academy_models::user::{User, UserComposite, UserDetails, UserInvoiceInfo, UserProfile};
use academy_persistence_contracts::user::{UserRepoError, UserRepository};
use academy_shared_contracts::{id::IdService, password::PasswordService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow};

#[derive(Debug, Clone, Copy, Build, Default)]
pub struct UserServiceImpl<Id, Time, Password, UserRepo, OAuth2CreateLink> {
    id: Id,
    time: Time,
    password: Password,
    user_repo: UserRepo,
    oauth2_create_link: OAuth2CreateLink,
}

impl<Txn, Id, Time, Password, UserRepo, OAuth2Link> UserService<Txn>
    for UserServiceImpl<Id, Time, Password, UserRepo, OAuth2Link>
where
    Txn: Send + Sync + 'static,
    Id: IdService,
    Time: TimeService,
    Password: PasswordService,
    UserRepo: UserRepository<Txn>,
    OAuth2Link: OAuth2LinkService<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn list(&self, txn: &mut Txn, query: UserListQuery) -> anyhow::Result<UserListResult> {
        let total = self
            .user_repo
            .count(txn, &query.filter)
            .await
            .context("Failed to get total number of users from database")?;

        let user_composites = self
            .user_repo
            .list_composites(txn, &query.filter, query.pagination)
            .await
            .context("Failed to get users from database")?;

        Ok(UserListResult {
            total,
            user_composites,
        })
    }

    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut Txn,
        UserCreateCommand {
            name,
            display_name,
            email,
            password,
            admin,
            enabled,
            email_verified,
            oauth2_registration,
            terms_version,
            age_confirmed,
        }: UserCreateCommand,
    ) -> Result<UserComposite, UserCreateError> {
        let password_hash = match password {
            Some(password) => Some(
                self.password
                    .hash(password.into_inner().into())
                    .await
                    .context("Failed to hash password")?,
            ),
            None => None,
        };

        let now = self.time.now();

        let user = User {
            id: self.id.generate(),
            name,
            email: Some(email),
            email_verified,
            created_at: now,
            last_login: None,
            last_name_change: None,
            enabled,
            admin,
            newsletter: false,
            terms_accepted_at: terms_version.is_some().then_some(now),
            terms_version,
            age_confirmed_at: age_confirmed.then_some(now),
        };

        let profile = UserProfile {
            display_name,
            bio: Default::default(),
            tags: Default::default(),
        };

        let details = UserDetails {
            mfa_enabled: false,
            password_login: password_hash.is_some(),
            oauth2_login: oauth2_registration.is_some(),
        };

        let invoice_info = UserInvoiceInfo::default();

        self.user_repo
            .create(txn, &user, &profile, &invoice_info)
            .await
            .map_err(|err| match err {
                UserRepoError::NameConflict => UserCreateError::NameConflict,
                UserRepoError::EmailConflict => UserCreateError::EmailConflict,
                UserRepoError::Other(err) => anyhow!(err)
                    .context("Failed to create user in database")
                    .into(),
            })?;

        if let Some(password_hash) = password_hash {
            self.user_repo
                .save_password_hash(txn, user.id, password_hash)
                .await
                .context("Failed to save password hash in database")?;
        }

        if let Some(oauth2_registration) = oauth2_registration {
            self.oauth2_create_link
                .create(
                    txn,
                    user.id,
                    oauth2_registration.provider_id,
                    oauth2_registration.remote_user,
                )
                .await
                .map_err(|err| match err {
                    OAuth2LinkServiceError::RemoteAlreadyLinked => {
                        UserCreateError::RemoteAlreadyLinked
                    }
                    OAuth2LinkServiceError::Other(err) => {
                        err.context("Failed to create OAuth2 link").into()
                    }
                })?;
        }

        let user_composite = UserComposite {
            user,
            profile,
            details,
            invoice_info,
        };

        Ok(user_composite)
    }
}

#[cfg(test)]
mod tests {
    use academy_core_oauth2_contracts::link::MockOAuth2LinkService;
    use academy_demo::{
        oauth2::{FOO_OAUTH2_LINK_1, TEST_OAUTH2_PROVIDER_ID},
        user::{ALL_USERS, FOO},
    };
    use academy_models::{
        oauth2::OAuth2Registration,
        pagination::PaginationSlice,
        user::{UserFilter, UserPassword},
    };
    use academy_persistence_contracts::user::MockUserRepository;
    use academy_shared_contracts::{
        id::MockIdService, password::MockPasswordService, time::MockTimeService,
    };
    use academy_utils::assert_matches;

    use super::*;

    type Sut = UserServiceImpl<
        MockIdService,
        MockTimeService,
        MockPasswordService,
        MockUserRepository<()>,
        MockOAuth2LinkService<()>,
    >;

    #[tokio::test]
    async fn list() {
        // Arrange
        let query = UserListQuery {
            pagination: PaginationSlice {
                limit: 42.try_into().unwrap(),
                offset: 7,
            },
            filter: UserFilter {
                name: Some("the name".try_into().unwrap()),
                email: Some("the email".try_into().unwrap()),
                enabled: Some(true),
                admin: Some(false),
                mfa_enabled: None,
                email_verified: Some(true),
                newsletter: Some(false),
            },
        };
        let expected = ALL_USERS.iter().copied().cloned().collect::<Vec<_>>();

        let user_repo = MockUserRepository::new()
            .with_count(query.filter.clone(), 17)
            .with_list_composites(query.filter.clone(), query.pagination, expected.clone());

        let sut = UserServiceImpl {
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.list(&mut (), query).await;

        // Assert
        let result = result.unwrap();
        assert_eq!(result.user_composites, expected);
    }

    #[tokio::test]
    async fn create_ok() {
        // Arrange
        let user_password = UserPassword::try_new("secure password").unwrap();
        let user_password_hash = "password_hash".to_owned();

        let expected = make_user_composite(true, false);

        let id = MockIdService::new().with_generate(FOO.user.id);
        let time = MockTimeService::new().with_now(FOO.user.created_at);
        let password = MockPasswordService::new().with_hash(
            user_password.clone().into_inner(),
            user_password_hash.clone(),
        );
        let user_repo = MockUserRepository::new()
            .with_create(
                expected.user.clone(),
                expected.profile.clone(),
                Default::default(),
                Ok(()),
            )
            .with_save_password_hash(FOO.user.id, user_password_hash);

        let sut = UserServiceImpl {
            id,
            time,
            password,
            user_repo,
            ..Sut::default()
        };

        let command = UserCreateCommand {
            name: FOO.user.name.clone(),
            display_name: FOO.profile.display_name.clone(),
            email: FOO.user.email.clone().unwrap(),
            password: Some(user_password),
            admin: false,
            enabled: true,
            email_verified: false,
            oauth2_registration: None,
            terms_version: FOO.user.terms_version.clone(),
            age_confirmed: true,
        };

        // Act
        let result = sut.create(&mut (), command).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn create_ok_without_consent() {
        // Arrange
        let user_password = UserPassword::try_new("secure password").unwrap();
        let user_password_hash = "password_hash".to_owned();

        let mut expected = make_user_composite(true, false);
        expected.user.terms_version = None;
        expected.user.terms_accepted_at = None;
        expected.user.age_confirmed_at = None;

        let id = MockIdService::new().with_generate(FOO.user.id);
        let time = MockTimeService::new().with_now(FOO.user.created_at);
        let password = MockPasswordService::new().with_hash(
            user_password.clone().into_inner(),
            user_password_hash.clone(),
        );
        let user_repo = MockUserRepository::new()
            .with_create(
                expected.user.clone(),
                expected.profile.clone(),
                Default::default(),
                Ok(()),
            )
            .with_save_password_hash(FOO.user.id, user_password_hash);

        let sut = UserServiceImpl {
            id,
            time,
            password,
            user_repo,
            ..Sut::default()
        };

        let command = UserCreateCommand {
            name: FOO.user.name.clone(),
            display_name: FOO.profile.display_name.clone(),
            email: FOO.user.email.clone().unwrap(),
            password: Some(user_password),
            admin: false,
            enabled: true,
            email_verified: false,
            oauth2_registration: None,
            terms_version: None,
            age_confirmed: false,
        };

        // Act
        let result = sut.create(&mut (), command).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn create_ok_oauth2() {
        // Arrange
        let expected = make_user_composite(false, true);

        let id = MockIdService::new().with_generate(FOO.user.id);
        let time = MockTimeService::new().with_now(FOO.user.created_at);
        let user_repo = MockUserRepository::new().with_create(
            expected.user.clone(),
            expected.profile.clone(),
            Default::default(),
            Ok(()),
        );

        let oauth2_create_link = MockOAuth2LinkService::new().with_create(
            FOO.user.id,
            TEST_OAUTH2_PROVIDER_ID.clone(),
            FOO_OAUTH2_LINK_1.remote_user.clone(),
            Ok(FOO_OAUTH2_LINK_1.clone()),
        );

        let sut = UserServiceImpl {
            id,
            time,
            user_repo,
            oauth2_create_link,
            ..Sut::default()
        };

        let command = UserCreateCommand {
            name: FOO.user.name.clone(),
            display_name: FOO.profile.display_name.clone(),
            email: FOO.user.email.clone().unwrap(),
            password: None,
            admin: false,
            enabled: true,
            email_verified: false,
            oauth2_registration: Some(OAuth2Registration {
                provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
                remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
            }),
            terms_version: FOO.user.terms_version.clone(),
            age_confirmed: true,
        };

        // Act
        let result = sut.create(&mut (), command).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn create_name_conflict() {
        // Arrange
        let user_password = UserPassword::try_new("secure password").unwrap();
        let user_password_hash = "password_hash".to_owned();

        let expected = make_user_composite(true, false);

        let id = MockIdService::new().with_generate(FOO.user.id);
        let time = MockTimeService::new().with_now(FOO.user.created_at);
        let password = MockPasswordService::new().with_hash(
            user_password.clone().into_inner(),
            user_password_hash.clone(),
        );
        let user_repo = MockUserRepository::new().with_create(
            expected.user.clone(),
            expected.profile.clone(),
            Default::default(),
            Err(UserRepoError::NameConflict),
        );

        let sut = UserServiceImpl {
            id,
            time,
            password,
            user_repo,
            ..Sut::default()
        };

        let command = UserCreateCommand {
            name: FOO.user.name.clone(),
            display_name: FOO.profile.display_name.clone(),
            email: FOO.user.email.clone().unwrap(),
            password: Some(user_password),
            admin: false,
            enabled: true,
            email_verified: false,
            oauth2_registration: None,
            terms_version: FOO.user.terms_version.clone(),
            age_confirmed: true,
        };

        // Act
        let result = sut.create(&mut (), command).await;

        // Assert
        assert_matches!(result, Err(UserCreateError::NameConflict));
    }

    #[tokio::test]
    async fn create_email_conflict() {
        // Arrange
        let user_password = UserPassword::try_new("secure password").unwrap();
        let user_password_hash = "password_hash".to_owned();

        let expected = make_user_composite(true, false);

        let id = MockIdService::new().with_generate(FOO.user.id);
        let time = MockTimeService::new().with_now(FOO.user.created_at);
        let password = MockPasswordService::new().with_hash(
            user_password.clone().into_inner(),
            user_password_hash.clone(),
        );
        let user_repo = MockUserRepository::new().with_create(
            expected.user.clone(),
            expected.profile.clone(),
            Default::default(),
            Err(UserRepoError::EmailConflict),
        );

        let sut = UserServiceImpl {
            id,
            time,
            password,
            user_repo,
            ..Sut::default()
        };

        let command = UserCreateCommand {
            name: FOO.user.name.clone(),
            display_name: FOO.profile.display_name.clone(),
            email: FOO.user.email.clone().unwrap(),
            password: Some(user_password),
            admin: false,
            enabled: true,
            email_verified: false,
            oauth2_registration: None,
            terms_version: FOO.user.terms_version.clone(),
            age_confirmed: true,
        };

        // Act
        let result = sut.create(&mut (), command).await;

        // Assert
        assert_matches!(result, Err(UserCreateError::EmailConflict));
    }

    #[tokio::test]
    async fn create_oauth2_remote_already_linked() {
        // Arrange
        let expected = make_user_composite(false, true);

        let id = MockIdService::new().with_generate(FOO.user.id);
        let time = MockTimeService::new().with_now(FOO.user.created_at);
        let user_repo = MockUserRepository::new().with_create(
            expected.user.clone(),
            expected.profile.clone(),
            Default::default(),
            Ok(()),
        );

        let oauth2_create_link = MockOAuth2LinkService::new().with_create(
            FOO.user.id,
            TEST_OAUTH2_PROVIDER_ID.clone(),
            FOO_OAUTH2_LINK_1.remote_user.clone(),
            Err(OAuth2LinkServiceError::RemoteAlreadyLinked),
        );

        let sut = UserServiceImpl {
            id,
            time,
            user_repo,
            oauth2_create_link,
            ..Sut::default()
        };

        let command = UserCreateCommand {
            name: FOO.user.name.clone(),
            display_name: FOO.profile.display_name.clone(),
            email: FOO.user.email.clone().unwrap(),
            password: None,
            admin: false,
            enabled: true,
            email_verified: false,
            oauth2_registration: Some(OAuth2Registration {
                provider_id: TEST_OAUTH2_PROVIDER_ID.clone(),
                remote_user: FOO_OAUTH2_LINK_1.remote_user.clone(),
            }),
            terms_version: FOO.user.terms_version.clone(),
            age_confirmed: true,
        };

        // Act
        let result = sut.create(&mut (), command).await;

        // Assert
        assert_matches!(result, Err(UserCreateError::RemoteAlreadyLinked));
    }

    fn make_user_composite(password_login: bool, oauth2_login: bool) -> UserComposite {
        UserComposite {
            user: User {
                id: FOO.user.id,
                name: FOO.user.name.clone(),
                email: FOO.user.email.clone(),
                email_verified: false,
                created_at: FOO.user.created_at,
                last_login: None,
                last_name_change: None,
                enabled: true,
                admin: false,
                newsletter: false,
                terms_version: FOO.user.terms_version.clone(),
                terms_accepted_at: Some(FOO.user.created_at),
                age_confirmed_at: Some(FOO.user.created_at),
            },
            profile: UserProfile {
                display_name: FOO.profile.display_name.clone(),
                bio: Default::default(),
                tags: Default::default(),
            },
            details: UserDetails {
                mfa_enabled: false,
                password_login,
                oauth2_login,
            },
            invoice_info: Default::default(),
        }
    }
}
