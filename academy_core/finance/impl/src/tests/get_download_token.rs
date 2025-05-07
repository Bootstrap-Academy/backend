use academy_auth_contracts::MockAuthService;
use academy_core_finance_contracts::{FinanceFeatureService, FinanceGetDownloadTokenError};
use academy_demo::{session::FOO_1, user::FOO};
use academy_models::auth::{AuthError, AuthenticateError};
use academy_shared_contracts::jwt::MockJwtService;
use academy_utils::assert_matches;

use crate::{DownloadToken, FinanceFeatureConfig, FinanceFeatureServiceImpl, tests::Sut};

#[tokio::test]
async fn ok() {
    // Arrange
    let config = FinanceFeatureConfig::default();

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let jwt = MockJwtService::new().with_sign(
        DownloadToken {
            sub: FOO.user.id,
            aud: Default::default(),
        },
        config.download_token_ttl,
        Ok("the-jwt".into()),
    );

    let sut = FinanceFeatureServiceImpl {
        auth,
        jwt,
        ..Sut::default()
    };

    // Act
    let result = sut.get_download_token(&"token".into()).await;

    // Assert
    assert_eq!(result.unwrap(), "the-jwt");
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = FinanceFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.get_download_token(&"token".into()).await;

    // Assert
    assert_matches!(
        result,
        Err(FinanceGetDownloadTokenError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}
