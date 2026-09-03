use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::coin::{CoinAddCoinsError, MockCoinService};
use academy_core_heart_contracts::{
    HeartFeatureService, HeartRefillError, heart::MockHeartService,
};
use academy_core_withdrawal_contracts::consent::MockWithdrawalConsentService;
use academy_demo::{UUID1, session::FOO_1, user::FOO};
use academy_models::{
    auth::{AuthError, AuthenticateError},
    coin::Balance,
    heart::Hearts,
    withdrawal::{WithdrawalConsent, WithdrawalConsentDeclaration, WithdrawalSubject},
};
use academy_persistence_contracts::MockDatabase;
use academy_utils::{Apply, assert_matches};

use crate::{HeartFeatureServiceImpl, tests::Sut};

fn declaration() -> WithdrawalConsentDeclaration {
    WithdrawalConsentDeclaration {
        given: true,
        text_version: Some("2026-09".try_into().unwrap()),
    }
}

#[tokio::test]
async fn ok() {
    // Arrange
    let expected = Hearts {
        hearts: 6,
        last_refill: FOO.user.created_at,
    };

    let db = MockDatabase::build(true);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let heart = MockHeartService::new()
        .with_get(FOO.user.id, expected.with(|h| h.hearts = 4))
        .with_add(FOO.user.id, 6, Ok(expected));

    let coin = MockCoinService::new().with_add_coins(
        FOO.user.id,
        -50,
        false,
        Some("Hearts".try_into().unwrap()),
        false,
        Ok(Balance::default()),
    );

    let withdrawal_consent = MockWithdrawalConsentService::new().with_record(WithdrawalConsent {
        id: UUID1.into(),
        user_id: FOO.user.id,
        subject: WithdrawalSubject::Hearts,
        reference: None,
        text_version: "2026-09".try_into().unwrap(),
        consented_at: FOO.user.created_at,
    });

    let sut = HeartFeatureServiceImpl {
        db,
        auth,
        heart,
        coin,
        withdrawal_consent,
        ..Sut::default()
    };

    // Act
    let result = sut.refill(&"token".into(), declaration()).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn ok_already_full() {
    // Arrange
    let expected = Hearts {
        hearts: 6,
        last_refill: FOO.user.created_at,
    };

    let db = MockDatabase::build(false);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let heart = MockHeartService::new().with_get(FOO.user.id, expected);

    let sut = HeartFeatureServiceImpl {
        db,
        auth,
        heart,
        ..Sut::default()
    };

    // Act
    let result = sut.refill(&"token".into(), declaration()).await;

    // Assert
    assert_eq!(result.unwrap(), expected);
}

#[tokio::test]
async fn unauthenticated() {
    // Arrange
    let auth = MockAuthService::new().with_authenticate(None);

    let sut = HeartFeatureServiceImpl {
        auth,
        ..Sut::default()
    };

    // Act
    let result = sut.refill(&"token".into(), declaration()).await;

    // Assert
    assert_matches!(
        result,
        Err(HeartRefillError::Auth(AuthError::Authenticate(
            AuthenticateError::InvalidToken
        )))
    );
}

#[tokio::test]
async fn not_enough_coins() {
    // Arrange
    let expected = Hearts {
        hearts: 4,
        last_refill: FOO.user.created_at,
    };

    let db = MockDatabase::build(false);

    let auth = MockAuthService::new().with_authenticate(Some((FOO.user.clone(), FOO_1.clone())));

    let heart = MockHeartService::new().with_get(FOO.user.id, expected);

    let coin = MockCoinService::new().with_add_coins(
        FOO.user.id,
        -50,
        false,
        Some("Hearts".try_into().unwrap()),
        false,
        Err(CoinAddCoinsError::NotEnoughCoins),
    );

    let sut = HeartFeatureServiceImpl {
        db,
        auth,
        heart,
        coin,
        ..Sut::default()
    };

    // Act
    let result = sut.refill(&"token".into(), declaration()).await;

    // Assert
    assert_matches!(result, Err(HeartRefillError::NotEnoughCoins));
}

#[tokio::test]
async fn withdrawal_consent_missing() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut
        .refill(
            &"token".into(),
            WithdrawalConsentDeclaration {
                given: false,
                ..declaration()
            },
        )
        .await;

    // Assert
    assert_matches!(result, Err(HeartRefillError::WithdrawalConsentMissing));
}
