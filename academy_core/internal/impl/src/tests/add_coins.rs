use academy_auth_contracts::internal::{AuthInternalAuthenticateError, MockAuthInternalService};
use academy_core_coin_contracts::coin::{CoinAddCoinsError, MockCoinService};
use academy_core_internal_contracts::{InternalAddCoinsError, InternalService};
use academy_demo::user::FOO;
use academy_models::coin::{Balance, TransactionDescription};
use academy_persistence_contracts::{user::MockUserRepository, MockDatabase};
use academy_utils::{assert_matches, Apply};

use crate::{tests::Sut, InternalServiceImpl};

#[tokio::test]
async fn ok() {
    for (coins, can_receive_coins, withhold, include_in_credit_note) in [
        (42, true, false, false),
        (42, false, true, false),
        (-42, true, false, false),
        (-42, false, false, false),
        (42, true, false, true),
    ] {
        // Arrange
        let expected = Balance {
            coins: 1234,
            withheld_coins: 321,
        };

        let description = TransactionDescription::try_new("asdf1234").unwrap();

        let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

        let db = MockDatabase::build(true);

        let user_repo = MockUserRepository::new().with_get_composite(
            FOO.user.id,
            Some(
                FOO.clone()
                    .with(|u| u.invoice_info.country.take_if(|_| !can_receive_coins)),
            ),
        );

        let coin = MockCoinService::new().with_add_coins(
            FOO.user.id,
            coins,
            withhold,
            Some(description.clone()),
            include_in_credit_note,
            Ok(expected),
        );

        let sut = InternalServiceImpl {
            auth_internal,
            db,
            user_repo,
            coin,
        };

        // Act
        let result = sut
            .add_coins(
                &"internal token".into(),
                FOO.user.id,
                coins,
                Some(description),
                include_in_credit_note,
            )
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", false);

    let sut = InternalServiceImpl {
        auth_internal,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_coins(&"internal token".into(), FOO.user.id, 42, None, false)
        .await;

    // Assert
    assert_matches!(
        result,
        Err(InternalAddCoinsError::Auth(
            AuthInternalAuthenticateError::InvalidToken
        ))
    );
}

#[tokio::test]
async fn user_not_found() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, None);

    let sut = InternalServiceImpl {
        auth_internal,
        db,
        user_repo,
        ..Sut::default()
    };

    // Act
    let result = sut
        .add_coins(&"internal token".into(), FOO.user.id, 42, None, false)
        .await;

    // Assert
    assert_matches!(result, Err(InternalAddCoinsError::UserNotFound));
}

#[tokio::test]
async fn not_enough_coins() {
    // Arrange
    let auth_internal = MockAuthInternalService::new().with_authenticate("shop", true);

    let db = MockDatabase::build(false);

    let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

    let coin = MockCoinService::new().with_add_coins(
        FOO.user.id,
        -42,
        false,
        None,
        false,
        Err(CoinAddCoinsError::NotEnoughCoins),
    );

    let sut = InternalServiceImpl {
        auth_internal,
        db,
        user_repo,
        coin,
    };

    // Act
    let result = sut
        .add_coins(&"internal token".into(), FOO.user.id, -42, None, false)
        .await;

    // Assert
    assert_matches!(result, Err(InternalAddCoinsError::NotEnoughCoins));
}
