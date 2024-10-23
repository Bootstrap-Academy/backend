use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::{CoinFeatureService, CoinGetBalanceError};
use academy_demo::{
    session::{ADMIN_1, BAR_1, FOO_1},
    user::{ADMIN, BAR, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    coin::Balance,
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    coin::MockCoinRepository, user::MockUserRepository, MockDatabase,
};
use academy_utils::assert_matches;

use crate::{tests::Sut, CoinFeatureServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Balance {
        coins: 42,
        withheld_coins: 1337,
    };

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let coin_repo = MockCoinRepository::new().with_get_balance(FOO.user.id, expected);

    let sut = CoinFeatureServiceImpl {
        auth,
        db,
        user_repo,
        coin_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get_balance(&"token".into(), UserIdOrSelf::Slf).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = CoinFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.get_balance(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(CoinGetBalanceError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((BAR.user.clone(), BAR_1.clone())));

    let sut = CoinFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.get_balance(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(
        result,
        Err(CoinGetBalanceError::Auth(AuthError::Authorize(
            AuthorizeError::Admin
        )))
    );
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, false);

    let sut = CoinFeatureServiceImpl {
        auth,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut.get_balance(&"token".into(), FOO.user.id.into()).await;

    // Assert
    assert_matches!(result, Err(CoinGetBalanceError::UserNotFound));
}
