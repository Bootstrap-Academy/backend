use academy_auth_contracts::{
    AuthService, Authentication, access_token::MockAuthAccessTokenService,
};
use academy_demo::{SHA256HASH1, UUID1, user::FOO};
use academy_models::auth::AuthenticateError;
use academy_utils::assert_matches;

use crate::{AuthServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Authentication {
        user_id: FOO.user.id,
        session_id: UUID1.into(),
        refresh_token_hash: (*SHA256HASH1).into(),
        admin: FOO.user.admin,
        email_verified: FOO.user.email_verified,
        mfa_verified: false,
    };

    let auth_access_token = MockAuthAccessTokenService::new()
        .with_verify("my auth token".into(), Some(expected))
        .with_is_invalidated(expected.refresh_token_hash, false);

    let sut = AuthServiceImpl {
        db: academy_persistence_contracts::MockDatabase::build(false),
        user_repo: academy_persistence_contracts::user::MockUserRepository::new()
            .with_get_composite(FOO.user.id, Some(FOO.clone())),
        session_repo: academy_persistence_contracts::session::MockSessionRepository::new()
            .with_get_by_refresh_token_hash(
                expected.refresh_token_hash,
                Some(academy_models::session::Session {
                    id: expected.session_id,
                    ..academy_demo::session::FOO_1.clone()
                }),
            ),
        auth_access_token,
        ..Sut::default()
    };

    // Act
    let result = sut.authenticate(&"my auth token".into()).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn invalid_token() {
    // Arrange
    let auth_access_token =
        MockAuthAccessTokenService::new().with_verify("my auth token".into(), None);

    let sut = AuthServiceImpl {
        auth_access_token,
        ..Sut::default()
    };

    // Act
    let result = sut.authenticate(&"my auth token".into()).await;

    // Assert
    assert_matches!(result, Err(AuthenticateError::InvalidToken));
}

#[tokio::test]
async fn access_token_invalidated() {
    // Arrange
    let expected = Authentication {
        user_id: FOO.user.id,
        session_id: UUID1.into(),
        refresh_token_hash: (*SHA256HASH1).into(),
        admin: FOO.user.admin,
        email_verified: FOO.user.email_verified,
        mfa_verified: false,
    };

    let auth_access_token = MockAuthAccessTokenService::new()
        .with_verify("my auth token".into(), Some(expected))
        .with_is_invalidated(expected.refresh_token_hash, true);

    let sut = AuthServiceImpl {
        auth_access_token,
        ..Sut::default()
    };

    // Act
    let result = sut.authenticate(&"my auth token".into()).await;

    // Assert
    assert_matches!(result, Err(AuthenticateError::InvalidToken));
}

#[tokio::test]
async fn deleted_account_rejected_with_unrevoked_cached_token() {
    // Arrange
    let expected = Authentication {
        user_id: FOO.user.id,
        session_id: UUID1.into(),
        refresh_token_hash: (*SHA256HASH1).into(),
        admin: FOO.user.admin,
        email_verified: FOO.user.email_verified,
        mfa_verified: false,
    };

    let auth_access_token = MockAuthAccessTokenService::new()
        .with_verify("my auth token".into(), Some(expected))
        .with_is_invalidated(expected.refresh_token_hash, false);

    let sut = AuthServiceImpl {
        db: academy_persistence_contracts::MockDatabase::build(false),
        user_repo: academy_persistence_contracts::user::MockUserRepository::new()
            .with_get_composite(FOO.user.id, None),
        auth_access_token,
        ..Sut::default()
    };

    // Act
    let result = sut.authenticate(&"my auth token".into()).await;

    // Assert
    assert_matches!(result, Err(AuthenticateError::InvalidToken));
}

#[tokio::test]
async fn disabled_or_missing_session_cannot_recover_authority_from_a_cache_miss() {
    for disabled in [true, false] {
        let expected = Authentication {
            user_id: FOO.user.id,
            session_id: UUID1.into(),
            refresh_token_hash: (*SHA256HASH1).into(),
            admin: true,
            email_verified: true,
            mfa_verified: true,
        };
        let mut user = FOO.clone();
        user.user.enabled = !disabled;
        let mut sessions = academy_persistence_contracts::session::MockSessionRepository::new();
        if !disabled {
            sessions = sessions.with_get_by_refresh_token_hash(expected.refresh_token_hash, None);
        }
        let sut = AuthServiceImpl {
            db: academy_persistence_contracts::MockDatabase::build(false),
            user_repo: academy_persistence_contracts::user::MockUserRepository::new()
                .with_get_composite(FOO.user.id, Some(user)),
            session_repo: sessions,
            auth_access_token: MockAuthAccessTokenService::new()
                .with_verify("my auth token".into(), Some(expected))
                .with_is_invalidated(expected.refresh_token_hash, false),
            ..Sut::default()
        };
        assert_matches!(
            sut.authenticate(&"my auth token".into()).await,
            Err(AuthenticateError::InvalidToken)
        );
    }
}
#[tokio::test]
async fn stale_admin_and_mfa_claims_do_not_grant_current_privilege() {
    let expected = Authentication {
        user_id: FOO.user.id,
        session_id: UUID1.into(),
        refresh_token_hash: (*SHA256HASH1).into(),
        admin: true,
        email_verified: true,
        mfa_verified: true,
    };
    let sut = AuthServiceImpl {
        db: academy_persistence_contracts::MockDatabase::build(false),
        user_repo: academy_persistence_contracts::user::MockUserRepository::new()
            .with_get_composite(FOO.user.id, Some(FOO.clone())),
        session_repo: academy_persistence_contracts::session::MockSessionRepository::new()
            .with_get_by_refresh_token_hash(
                expected.refresh_token_hash,
                Some(academy_models::session::Session {
                    id: expected.session_id,
                    ..academy_demo::session::FOO_1.clone()
                }),
            ),
        auth_access_token: MockAuthAccessTokenService::new()
            .with_verify("my auth token".into(), Some(expected))
            .with_is_invalidated(expected.refresh_token_hash, false),
        ..Sut::default()
    };
    let actual = sut.authenticate(&"my auth token".into()).await.unwrap();
    assert!(!actual.admin);
    assert!(!actual.mfa_verified);
    assert!(actual.ensure_admin().is_err());
}
