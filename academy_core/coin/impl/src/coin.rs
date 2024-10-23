use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_di::Build;
use academy_models::{
    coin::{Balance, TransactionDescription},
    user::UserId,
};
use academy_persistence_contracts::coin::{CoinRepoAddCoinsError, CoinRepository};
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build)]
pub struct CoinServiceImpl<CoinRepo> {
    coin_repo: CoinRepo,
}

impl<Txn, CoinRepo> CoinService<Txn> for CoinServiceImpl<CoinRepo>
where
    Txn: Send + Sync + 'static,
    CoinRepo: CoinRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn add_coins(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        coins: i64,
        withhold: bool,
        // TODO: save transactions
        _description: Option<TransactionDescription>,
        _include_in_credit_note: bool,
    ) -> Result<Balance, CoinAddCoinsError> {
        self.coin_repo
            .add_coins(txn, user_id, coins, withhold)
            .await
            .map_err(|err| match err {
                CoinRepoAddCoinsError::NotEnoughCoins => CoinAddCoinsError::NotEnoughCoins,
                CoinRepoAddCoinsError::Other(err) => err.into(),
            })
    }
}

#[cfg(test)]
mod tests {
    use academy_demo::user::FOO;
    use academy_persistence_contracts::coin::MockCoinRepository;
    use academy_utils::assert_matches;

    use super::*;

    #[tokio::test]
    async fn add_coins_ok() {
        // Arrange
        let expected = Balance {
            coins: 42,
            withheld_coins: 0,
        };

        let description = TransactionDescription::try_new("test123").unwrap();

        let coin_repo =
            MockCoinRepository::new().with_add_coins(FOO.user.id, -1337, false, Ok(expected));

        let sut = CoinServiceImpl { coin_repo };

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

        let sut = CoinServiceImpl { coin_repo };

        // Act
        let result = sut
            .add_coins(&mut (), FOO.user.id, -1337, false, Some(description), true)
            .await;

        // Assert
        assert_matches!(result, Err(CoinAddCoinsError::NotEnoughCoins));
    }
}
