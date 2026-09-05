use academy_auth_contracts::{
    AuthService, Tokens, access_token::MockAuthAccessTokenService,
    refresh_token::MockAuthRefreshTokenService,
};
use academy_demo::{SHA256HASH1, UUID1, user::FOO};

use crate::{AuthServiceConfig, AuthServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let config = AuthServiceConfig::default();

    let expected = Tokens {
        access_token: "the access token jwt".into(),
        refresh_token: "some refresh token".into(),
        refresh_token_hash: (*SHA256HASH1).into(),
    };

    let auth_access_token = MockAuthAccessTokenService::new().with_issue(
        FOO.user.clone(),
        UUID1.into(),
        (*SHA256HASH1).into(),
        true,
        expected.access_token.clone(),
    );

    let auth_refresh_token = MockAuthRefreshTokenService::new()
        .with_issue(expected.refresh_token.clone())
        .with_hash(expected.refresh_token.clone(), expected.refresh_token_hash);

    let sut = AuthServiceImpl {
        config,
        auth_access_token,
        auth_refresh_token,
        ..Sut::default()
    };

    // Act
    let result = sut.issue_tokens(&FOO.user, UUID1.into(), true);

    // Assert
    assert_eq!(result.unwrap(), expected);
}
