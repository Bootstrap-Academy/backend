use academy_auth_contracts::{AuthService, access_token::AuthAccessTokenService};
use academy_core_session_contracts::session::{SessionRefreshError, SessionService};
use academy_di::Build;
use academy_models::{
    auth::Login,
    session::{DeviceName, Session, SessionId, SessionPatch},
    user::{UserComposite, UserId, UserPatch},
};
use academy_persistence_contracts::{session::SessionRepository, user::UserRepository};
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::{patch::Patch, trace_instrument};
use anyhow::Context;

#[derive(Debug, Clone, Build, Default)]
pub struct SessionServiceImpl<Id, Time, Auth, AuthAccessToken, SessionRepo, UserRepo> {
    id: Id,
    time: Time,
    auth: Auth,
    auth_access_token: AuthAccessToken,
    session_repo: SessionRepo,
    user_repo: UserRepo,
}

impl<Txn, Id, Time, Auth, AuthAccessToken, SessionRepo, UserRepo> SessionService<Txn>
    for SessionServiceImpl<Id, Time, Auth, AuthAccessToken, SessionRepo, UserRepo>
where
    Txn: Send + Sync + 'static,
    Id: IdService,
    Time: TimeService,
    Auth: AuthService<Txn>,
    AuthAccessToken: AuthAccessTokenService,
    SessionRepo: SessionRepository<Txn>,
    UserRepo: UserRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut Txn,
        mut user_composite: UserComposite,
        device_name: Option<DeviceName>,
        update_last_login: bool,
    ) -> anyhow::Result<Login> {
        let id = self.id.generate();
        let now = self.time.now();

        let session = Session {
            id,
            user_id: user_composite.user.id,
            device_name,
            created_at: now,
            updated_at: now,
        };

        let tokens = self
            .auth
            .issue_tokens(&user_composite.user, session.id)
            .context("Failed to issue tokens")?;

        self.session_repo
            .create(txn, &session)
            .await
            .context("Failed to create session in database")?;
        self.session_repo
            .save_refresh_token_hash(txn, session.id, tokens.refresh_token_hash)
            .await
            .context("Failed to save session refresh token hash in database")?;

        if update_last_login {
            let patch = UserPatch::new().update_last_login(Some(now));
            self.user_repo
                .update(txn, user_composite.user.id, patch.as_ref())
                .await
                .context("Failed to update user in database")?;
            user_composite.user = user_composite.user.update(patch);
        }

        Ok(Login {
            user_composite,
            session,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
        })
    }

    #[trace_instrument(skip(self, txn))]
    async fn refresh(
        &self,
        txn: &mut Txn,
        session_id: SessionId,
    ) -> Result<Login, SessionRefreshError> {
        // get session and user from database
        let refresh_token_hash = self
            .session_repo
            .get_refresh_token_hash(txn, session_id)
            .await
            .context("Failed to get session refresh token hash from database")?
            .ok_or(SessionRefreshError::NotFound)?;

        let session = self
            .session_repo
            .get(txn, session_id)
            .await
            .context("Failed to get session from database")?
            .ok_or(SessionRefreshError::NotFound)?;

        let user_composite = self
            .user_repo
            .get_composite(txn, session.user_id)
            .await
            .context("Failed to get user from database")?
            .ok_or(SessionRefreshError::NotFound)?;

        // invalidate old access token
        self.auth_access_token
            .invalidate(refresh_token_hash)
            .await
            .context("Failed to invalidate old access token")?;

        // issue new token pair
        let tokens = self
            .auth
            .issue_tokens(&user_composite.user, session_id)
            .context("Failed to issue tokens")?;

        // update session
        let patch = SessionPatch::new().update_updated_at(self.time.now());
        self.session_repo
            .update(txn, session.id, patch.as_ref())
            .await
            .context("Failed to update session in database")?;
        let session = session.update(patch);

        self.session_repo
            .save_refresh_token_hash(txn, session.id, tokens.refresh_token_hash)
            .await
            .context("Failed to update session refresh token hash in database")?;

        Ok(Login {
            user_composite,
            session,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
        })
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete(&self, txn: &mut Txn, session_id: SessionId) -> anyhow::Result<bool> {
        if let Some(refresh_token_hash) = self
            .session_repo
            .get_refresh_token_hash(txn, session_id)
            .await
            .context("Failed to get session fresh token hash from database")?
        {
            self.auth_access_token
                .invalidate(refresh_token_hash)
                .await
                .context("Failed to invalidate access token")?;
        }

        self.session_repo
            .delete(txn, session_id)
            .await
            .context("Failed to delete session from database")
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_by_user(&self, txn: &mut Txn, user_id: UserId) -> anyhow::Result<()> {
        self.auth
            .invalidate_access_tokens(txn, user_id)
            .await
            .context("Failed to invalidate access tokens")?;

        self.session_repo
            .delete_by_user(txn, user_id)
            .await
            .context("Failed to delete sessions from database")?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use academy_auth_contracts::{
        MockAuthService, Tokens, access_token::MockAuthAccessTokenService,
    };
    use academy_demo::{SHA256HASH1, SHA256HASH2, session::FOO_1, user::FOO};
    use academy_models::user::{User, UserPatch};
    use academy_persistence_contracts::{session::MockSessionRepository, user::MockUserRepository};
    use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
    use academy_utils::assert_matches;

    use super::*;

    type Sut = SessionServiceImpl<
        MockIdService,
        MockTimeService,
        MockAuthService<()>,
        MockAuthAccessTokenService,
        MockSessionRepository<()>,
        MockUserRepository<()>,
    >;

    #[tokio::test]
    async fn create_update_last_login() {
        // Arrange
        let tokens = Tokens {
            access_token: "the access token".into(),
            refresh_token: "the refresh token".into(),
            refresh_token_hash: (*SHA256HASH1).into(),
        };

        let expected = Login {
            user_composite: UserComposite {
                user: User {
                    last_login: Some(FOO_1.created_at),
                    ..FOO.user.clone()
                },
                ..FOO.clone()
            },
            session: Session {
                id: FOO_1.id,
                user_id: FOO.user.id,
                device_name: FOO_1.device_name.clone(),
                created_at: FOO_1.created_at,
                updated_at: FOO_1.created_at,
            },
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
        };

        let id = MockIdService::new().with_generate(FOO_1.id);
        let time = MockTimeService::new().with_now(FOO_1.created_at);
        let auth =
            MockAuthService::new().with_issue_tokens(FOO.user.clone(), FOO_1.id, tokens.clone());
        let session_repo = MockSessionRepository::new()
            .with_create(expected.session.clone())
            .with_save_refresh_token_hash(FOO_1.id, (*SHA256HASH1).into());

        let user_repo = MockUserRepository::new().with_update(
            FOO.user.id,
            UserPatch::new().update_last_login(Some(FOO_1.created_at)),
            Ok(true),
        );

        let sut = SessionServiceImpl {
            id,
            time,
            auth,
            session_repo,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .create(&mut (), FOO.clone(), FOO_1.device_name.clone(), true)
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn create_dont_update_last_login() {
        // Arrange
        let tokens = Tokens {
            access_token: "the access token".into(),
            refresh_token: "the refresh token".into(),
            refresh_token_hash: (*SHA256HASH1).into(),
        };

        let expected = Login {
            user_composite: FOO.clone(),
            session: Session {
                id: FOO_1.id,
                user_id: FOO.user.id,
                device_name: FOO_1.device_name.clone(),
                created_at: FOO_1.created_at,
                updated_at: FOO_1.created_at,
            },
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
        };

        let id = MockIdService::new().with_generate(FOO_1.id);
        let time = MockTimeService::new().with_now(FOO_1.created_at);
        let auth =
            MockAuthService::new().with_issue_tokens(FOO.user.clone(), FOO_1.id, tokens.clone());
        let session_repo = MockSessionRepository::new()
            .with_create(expected.session.clone())
            .with_save_refresh_token_hash(FOO_1.id, (*SHA256HASH1).into());

        let user_repo = MockUserRepository::new();

        let sut = SessionServiceImpl {
            id,
            time,
            auth,
            session_repo,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .create(&mut (), FOO.clone(), FOO_1.device_name.clone(), false)
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn refresh_ok() {
        // Arrange
        let tokens = Tokens {
            access_token: "the new access token".into(),
            refresh_token: "the new refresh token".into(),
            refresh_token_hash: (*SHA256HASH2).into(),
        };

        let expected = Login {
            user_composite: FOO.clone(),
            session: Session {
                updated_at: FOO_1.updated_at + Duration::from_secs(3600),
                ..FOO_1.clone()
            },
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
        };

        let auth = MockAuthService::new().with_issue_tokens(FOO.user.clone(), FOO_1.id, tokens);

        let auth_access_token =
            MockAuthAccessTokenService::new().with_invalidate((*SHA256HASH1).into());

        let time = MockTimeService::new().with_now(expected.session.updated_at);

        let user_repo =
            MockUserRepository::new().with_get_composite(FOO_1.user_id, Some(FOO.clone()));
        let session_repo = MockSessionRepository::new()
            .with_get_refresh_token_hash(FOO_1.id, Some((*SHA256HASH1).into()))
            .with_get(FOO_1.id, Some(FOO_1.clone()))
            .with_update(
                FOO_1.id,
                SessionPatch::new().update_updated_at(expected.session.updated_at),
                true,
            )
            .with_save_refresh_token_hash(FOO_1.id, (*SHA256HASH2).into());

        let sut = SessionServiceImpl {
            auth,
            auth_access_token,
            time,
            session_repo,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.refresh(&mut (), FOO_1.id).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn refresh_token_hash_not_found() {
        // Arrange
        let session_repo = MockSessionRepository::new().with_get_refresh_token_hash(FOO_1.id, None);

        let sut = SessionServiceImpl {
            session_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.refresh(&mut (), FOO_1.id).await;

        // Assert
        assert_matches!(result, Err(SessionRefreshError::NotFound));
    }

    #[tokio::test]
    async fn refresh_session_not_found() {
        // Arrange
        let session_repo = MockSessionRepository::new()
            .with_get_refresh_token_hash(FOO_1.id, Some((*SHA256HASH1).into()))
            .with_get(FOO_1.id, None);

        let sut = SessionServiceImpl {
            session_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.refresh(&mut (), FOO_1.id).await;

        // Assert
        assert_matches!(result, Err(SessionRefreshError::NotFound));
    }

    #[tokio::test]
    async fn refresh_user_not_found() {
        // Arrange
        let session_repo = MockSessionRepository::new()
            .with_get_refresh_token_hash(FOO_1.id, Some((*SHA256HASH1).into()))
            .with_get(FOO_1.id, Some(FOO_1.clone()));
        let user_repo = MockUserRepository::new().with_get_composite(FOO_1.user_id, None);

        let sut = SessionServiceImpl {
            session_repo,
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.refresh(&mut (), FOO_1.id).await;

        // Assert
        assert_matches!(result, Err(SessionRefreshError::NotFound));
    }

    #[tokio::test]
    async fn delete_ok() {
        // Arrange
        let session_repo = MockSessionRepository::new()
            .with_get_refresh_token_hash(FOO_1.id, Some((*SHA256HASH1).into()))
            .with_delete(FOO_1.id, true);

        let auth_access_token =
            MockAuthAccessTokenService::new().with_invalidate((*SHA256HASH1).into());

        let sut = SessionServiceImpl {
            auth_access_token,
            session_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.delete(&mut (), FOO_1.id).await;

        // Assert
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn delete_not_found() {
        // Arrange
        let session_repo = MockSessionRepository::new()
            .with_get_refresh_token_hash(FOO_1.id, None)
            .with_delete(FOO_1.id, false);

        let sut = SessionServiceImpl {
            session_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.delete(&mut (), FOO_1.id).await;

        // Assert
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn delete_by_user() {
        // Arrange
        let auth = MockAuthService::new().with_invalidate_access_tokens(FOO.user.id);

        let session_repo = MockSessionRepository::new().with_delete_by_user(FOO.user.id);

        let sut = SessionServiceImpl {
            auth,
            session_repo,
            ..Sut::default()
        };

        // Arrange
        let result = sut.delete_by_user(&mut (), FOO.user.id).await;

        // Assert
        result.unwrap();
    }
}
