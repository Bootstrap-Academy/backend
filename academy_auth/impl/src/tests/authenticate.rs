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
