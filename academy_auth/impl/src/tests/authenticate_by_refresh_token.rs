use std::time::Duration;

use academy_auth_contracts::{
    AuthService, AuthenticateByRefreshTokenError, refresh_token::MockAuthRefreshTokenService,
};
use academy_demo::{SHA256HASH1, session::FOO_1};
use academy_persistence_contracts::session::MockSessionRepository;
use academy_shared_contracts::time::MockTimeService;
use academy_utils::assert_matches;

use crate::{AuthServiceConfig, AuthServiceImpl, tests::Sut};

#[tokio::test]
async fn authenticate_by_refresh_token_ok() {
    // Arrange
    let config = AuthServiceConfig::default();

    let auth_refresh_token = MockAuthRefreshTokenService::new()
        .with_hash("the refresh token".into(), (*SHA256HASH1).into());

    let time = MockTimeService::new()
        .with_now(FOO_1.updated_at + config.refresh_token_ttl - Duration::from_secs(1));

    let session_repo = MockSessionRepository::new()
        .with_get_by_refresh_token_hash((*SHA256HASH1).into(), Some(FOO_1.clone()));

    let sut = AuthServiceImpl {
        auth_refresh_token,
        time,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_refresh_token(&mut (), &"the refresh token".into())
        .await;

    // Assert
    assert_eq!(result.unwrap(), FOO_1.id);
}

#[tokio::test]
async fn authenticate_by_refresh_token_invalid() {
    // Arrange
    let auth_refresh_token = MockAuthRefreshTokenService::new()
        .with_hash("the refresh token".into(), (*SHA256HASH1).into());

    let session_repo =
        MockSessionRepository::new().with_get_by_refresh_token_hash((*SHA256HASH1).into(), None);

    let sut = AuthServiceImpl {
        auth_refresh_token,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_refresh_token(&mut (), &"the refresh token".into())
        .await;

    // Assert
    assert_matches!(result, Err(AuthenticateByRefreshTokenError::Invalid));
}

#[tokio::test]
async fn authenticate_by_refresh_token_expired() {
    // Arrange
    let config = AuthServiceConfig::default();

    let auth_refresh_token = MockAuthRefreshTokenService::new()
        .with_hash("the refresh token".into(), (*SHA256HASH1).into());

    let time = MockTimeService::new()
        .with_now(FOO_1.updated_at + config.refresh_token_ttl + Duration::from_secs(2));

    let session_repo = MockSessionRepository::new()
        .with_get_by_refresh_token_hash((*SHA256HASH1).into(), Some(FOO_1.clone()));

    let sut = AuthServiceImpl {
        auth_refresh_token,
        time,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_refresh_token(&mut (), &"the refresh token".into())
        .await;

    // Assert
    assert_matches!(result, Err(AuthenticateByRefreshTokenError::Expired(x)) if *x == FOO_1.id);
}
