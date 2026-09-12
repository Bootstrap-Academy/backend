use academy_auth_contracts::internal::MockAuthInternalService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_core_internal_contracts::{InternalAddCoinsError, InternalService};
use academy_demo::{UUID1, user::FOO};
use academy_models::coin::{Balance, CoinOperation, CoinOperationClaim};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, coin::MockCoinRepository, user::MockUserRepository,
};
use academy_utils::{Apply, assert_matches};

use super::Sut;

fn operation() -> CoinOperation {
    CoinOperation {
        id: UUID1.into(),
        user_id: FOO.user.id,
        coins: 17,
        description: Some("Historical challenge reward".try_into().unwrap()),
        include_in_credit_note: true,
    }
}

fn claimed(claim: CoinOperationClaim) -> MockCoinRepository<MockTransaction> {
    let mut repo = MockCoinRepository::new();
    repo.expect_claim_operation()
        .once()
        .withf(|_, value| *value == operation())
        .return_once(move |_, _| Box::pin(async move { Ok(claim) }));
    repo
}

#[tokio::test]
async fn unknown_positive_credit_does_not_mutate_balance_or_recipient() {
    let sut = Sut {
        db: MockDatabase::build(false),
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
        coin_repo: claimed(CoinOperationClaim::CreditNotAuthorized),
        ..Sut::default()
    };
    assert_matches!(
        sut.apply_coin_operation(&"internal token".into(), operation())
            .await,
        Err(InternalAddCoinsError::CreditNotAuthorized)
    );
}

#[tokio::test]
async fn completed_historical_credit_replays_without_current_recipient() {
    let expected = Balance {
        coins: 17,
        withheld_coins: 0,
    };
    let sut = Sut {
        db: MockDatabase::build(false),
        auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
        coin_repo: claimed(CoinOperationClaim::Completed(expected)),
        ..Sut::default()
    };
    assert_eq!(
        sut.apply_coin_operation(&"internal token".into(), operation())
            .await
            .unwrap(),
        expected
    );
}

#[tokio::test]
async fn reserved_historical_credit_preserves_withholding_and_credit_note() {
    for withhold in [false, true] {
        let expected = Balance {
            coins: if withhold { 0 } else { 17 },
            withheld_coins: if withhold { 17 } else { 0 },
        };
        let recipient = FOO
            .clone()
            .with(|user| user.invoice_info.country.take_if(|_| withhold));
        let mut coin_repo = claimed(CoinOperationClaim::New);
        coin_repo
            .expect_complete_operation()
            .once()
            .withf(move |_, id, result| *id == UUID1.into() && *result == expected)
            .return_once(|_, _, _| Box::pin(async { Ok(()) }));
        let sut = Sut {
            db: MockDatabase::build(true),
            auth_internal: MockAuthInternalService::new().with_authenticate("shop", true),
            user_repo: MockUserRepository::new()
                .with_get_purchase_composite(FOO.user.id, Some(recipient)),
            coin_repo,
            coin: MockCoinService::new().with_add_coins(
                FOO.user.id,
                17,
                withhold,
                operation().description,
                true,
                Ok(expected),
            ),
            ..Sut::default()
        };
        assert_eq!(
            sut.apply_coin_operation(&"internal token".into(), operation())
                .await
                .unwrap(),
            expected
        );
    }
}
