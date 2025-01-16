use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_di::Build;
use academy_models::{
    coin::{Balance, Transaction, TransactionDescription},
    user::UserId,
};
use academy_persistence_contracts::coin::{CoinRepoAddCoinsError, CoinRepository};
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build, Default)]
pub struct CoinServiceImpl<Id, Time, CoinRepo> {
    id: Id,
    time: Time,
    coin_repo: CoinRepo,
}

impl<Txn, Id, Time, CoinRepo> CoinService<Txn> for CoinServiceImpl<Id, Time, CoinRepo>
where
    Txn: Send + Sync + 'static,
    Id: IdService,
    Time: TimeService,
    CoinRepo: CoinRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn add_coins(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        coins: i64,
        withhold: bool,
        description: Option<TransactionDescription>,
        include_in_credit_note: bool,
    ) -> Result<Balance, CoinAddCoinsError> {
        let new_balance = self
            .coin_repo
            .add_coins(txn, user_id, coins, withhold)
            .await
            .map_err(|err| match err {
                CoinRepoAddCoinsError::NotEnoughCoins => CoinAddCoinsError::NotEnoughCoins,
                CoinRepoAddCoinsError::Other(err) => err.into(),
            })?;

        let transaction = Transaction {
            id: self.id.generate(),
            user_id,
            coins,
            description,
            created_at: self.time.now(),
            include_in_credit_note,
        };
        self.coin_repo.create_transaction(txn, &transaction).await?;

        Ok(new_balance)
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::{user::FOO, UUID1};
    use academy_models::coin::TransactionId;
    use academy_persistence_contracts::coin::MockCoinRepository;
    use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
    use academy_utils::assert_matches;

    use super::*;

    type Sut = CoinServiceImpl<MockIdService, MockTimeService, MockCoinRepository<()>>;

    #[tokio::test]
    async fn add_coins_ok() {
        // Arrange
        let expected = Balance {
            coins: 42,
            withheld_coins: 0,
        };

        let description = TransactionDescription::try_new("test123").unwrap();

        let id = MockIdService::new().with_generate(TransactionId::from(UUID1));

        let time = MockTimeService::new().with_now(FOO.user.last_login.unwrap());

        let coin_repo = MockCoinRepository::new()
            .with_add_coins(FOO.user.id, -1337, false, Ok(expected))
            .with_create_transaction(Transaction {
                id: UUID1.into(),
                user_id: FOO.user.id,
                coins: -1337,
                description: Some(description.clone()),
                created_at: FOO.user.last_login.unwrap(),
                include_in_credit_note: true,
            });

        let sut = CoinServiceImpl {
            id,
            time,
            coin_repo,
        };

        // Act
        let result = sut
            .add_coins(&mut (), FOO.user.id, -1337, false, Some(description), true)
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn add_coins_not_enough_coins() {
        // Arrange
        let description = TransactionDescription::try_new("test123").unwrap();

        let coin_repo = MockCoinRepository::new().with_add_coins(
            FOO.user.id,
            -1337,
            false,
            Err(CoinRepoAddCoinsError::NotEnoughCoins),
        );

        let sut = CoinServiceImpl {
            coin_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .add_coins(&mut (), FOO.user.id, -1337, false, Some(description), true)
            .await;

        // Assert
        assert_matches!(result, Err(CoinAddCoinsError::NotEnoughCoins));
    }
}
