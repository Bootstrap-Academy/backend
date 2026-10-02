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
        .with_get_by_refresh_token_hash_for_update((*SHA256HASH1).into(), Some(FOO_1.clone()));

    let sut = AuthServiceImpl {
        auth_refresh_token,
        time,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_refresh_token(
            &mut academy_persistence_contracts::MockTransaction::new(),
            &"the refresh token".into(),
        )
        .await;

    // Assert
    assert_eq!(result.unwrap(), FOO_1.id);
}

#[tokio::test]
async fn authenticate_by_refresh_token_invalid() {
    // Arrange
    let auth_refresh_token = MockAuthRefreshTokenService::new()
        .with_hash("the refresh token".into(), (*SHA256HASH1).into());

    let session_repo = MockSessionRepository::new()
        .with_get_by_refresh_token_hash_for_update((*SHA256HASH1).into(), None);

    let sut = AuthServiceImpl {
        auth_refresh_token,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_refresh_token(
            &mut academy_persistence_contracts::MockTransaction::new(),
            &"the refresh token".into(),
        )
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

    let time = MockTimeService::new().with_now(FOO_1.updated_at + config.refresh_token_ttl);

    let session_repo = MockSessionRepository::new()
        .with_get_by_refresh_token_hash_for_update((*SHA256HASH1).into(), Some(FOO_1.clone()));

    let sut = AuthServiceImpl {
        auth_refresh_token,
        time,
        session_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .authenticate_by_refresh_token(
            &mut academy_persistence_contracts::MockTransaction::new(),
            &"the refresh token".into(),
        )
        .await;

    // Assert
    assert_matches!(result, Err(AuthenticateByRefreshTokenError::Expired(x)) if *x == FOO_1.id);
}

#[tokio::test]
async fn authenticate_by_refresh_token_expires_while_waiting() {
    use std::sync::{Arc, Mutex};

    let config = AuthServiceConfig::default();
    let expiry = FOO_1.updated_at + config.refresh_token_ttl;
    let clock = Arc::new(Mutex::new(expiry - Duration::from_secs(1)));
    let after_lock = Arc::clone(&clock);
    let mut session_repo = MockSessionRepository::new();
    session_repo
        .expect_get_by_refresh_token_hash_for_update()
        .once()
        .return_once(move |_, _| {
            Box::pin(async move {
                // The owner lock is acquired only after the refresh has expired.
                tokio::task::yield_now().await;
                *after_lock.lock().unwrap() = expiry;
                Ok(Some(FOO_1.clone()))
            })
        });
    let mut time = MockTimeService::new();
    time.expect_now()
        .once()
        .returning(move || *clock.lock().unwrap());
    let auth_refresh_token = MockAuthRefreshTokenService::new()
        .with_hash("the refresh token".into(), (*SHA256HASH1).into());
    let sut = AuthServiceImpl {
        config,
        time,
        session_repo,
        auth_refresh_token,
        ..Sut::default()
    };
    let result = sut
        .authenticate_by_refresh_token(
            &mut academy_persistence_contracts::MockTransaction::new(),
            &"the refresh token".into(),
        )
        .await;
    assert_matches!(result, Err(AuthenticateByRefreshTokenError::Expired(x)) if *x == FOO_1.id);
}
