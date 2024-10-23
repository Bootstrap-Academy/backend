use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::{coin::MockCoinService, CoinAddCoinsError, CoinFeatureService};
use academy_demo::{
    session::{ADMIN_1, FOO_1},
    user::{ADMIN, FOO},
};
use academy_models::{
    auth::{AuthError, AuthenticateError, AuthorizeError},
    coin::Balance,
};
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::assert_matches;

use crate::{tests::Sut, CoinFeatureServiceImpl};

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Balance {
        coins: 42,
        withheld_coins: 0,
    };

    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(true);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let coin =
        MockCoinService::new().with_add_coins(FOO.user.id, -42, false, None, false, Ok(expected));

    let sut = CoinFeatureServiceImpl {
        auth,
        db,
        user_repo,
        coin,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_coins(&"token".into(), FOO.user.id.into(), -42, None, false)
        .await;

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
    let result = sut
        .add_coins(&"token".into(), FOO.user.id.into(), 42, None, false)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(CoinAddCoinsError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn unauthorized() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let sut = CoinFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_coins(&"token".into(), FOO.user.id.into(), 42, None, false)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(CoinAddCoinsError::Auth(AuthError::Authorize(
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
    let result = sut
        .add_coins(&"token".into(), FOO.user.id.into(), 42, None, false)
        .await;

    // Assert
    assert_matches!(result, Err(CoinAddCoinsError::UserNotFound));
}

#[tokio::test]
async fn not_enough_coins() {
    // Arrange
    let auth =
        MockAuthService::new().with_authenticate(Some((ADMIN.user.clone(), ADMIN_1.clone())));

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_exists(FOO.user.id, true);

    let coin = MockCoinService::new().with_add_coins(
        FOO.user.id,
        -42,
        false,
        None,
        false,
        Err(academy_core_coin_contracts::coin::CoinAddCoinsError::NotEnoughCoins),
    );

    let sut = CoinFeatureServiceImpl {
        auth,
        db,
        user_repo,
        coin,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_coins(&"token".into(), FOO.user.id.into(), -42, None, false)
        .await;

    // Assert
    assert_matches!(result, Err(CoinAddCoinsError::NotEnoughCoins));
}
