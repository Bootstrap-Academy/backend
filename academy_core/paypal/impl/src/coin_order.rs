use academy_core_paypal_contracts::coin_order::PaypalCoinOrderService;
use academy_di::Build;
use academy_models::{
    coin::Balance,
    paypal::{PaypalCoinOrder, PaypalOrderId},
    user::UserId,
};
use academy_persistence_contracts::{coin::CoinRepository, paypal::PaypalRepository};
use academy_shared_contracts::time::TimeService;
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build, Default)]
pub struct PaypalCoinOrderServiceImpl<Time, PaypalRepo, CoinRepo> {
    time: Time,
    paypal_repo: PaypalRepo,
    coin_repo: CoinRepo,
}

impl<Txn, Time, PaypalRepo, CoinRepo> PaypalCoinOrderService<Txn>
    for PaypalCoinOrderServiceImpl<Time, PaypalRepo, CoinRepo>
where
    Txn: Send + Sync + 'static,
    Time: TimeService,
    PaypalRepo: PaypalRepository<Txn>,
    CoinRepo: CoinRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut Txn,
        id: PaypalOrderId,
        user_id: UserId,
        coins: u64,
    ) -> anyhow::Result<PaypalCoinOrder> {
        let now = self.time.now();
        let invoice_number = self.paypal_repo.get_next_invoice_number(txn).await?;

        let coin_order = PaypalCoinOrder {
            id,
            user_id,
            created_at: now,
            captured_at: None,
            coins,
            invoice_number,
        };

        self.paypal_repo.create_coin_order(txn, &coin_order).await?;

        Ok(coin_order)
    }

    #[trace_instrument(skip(self, txn))]
    async fn capture(&self, txn: &mut Txn, order: PaypalCoinOrder) -> anyhow::Result<Balance> {
        let now = self.time.now();

        self.paypal_repo
            .capture_coin_order(txn, &order.id, now)
            .await?;

        let new_balance = self
            .coin_repo
            .add_coins(txn, order.user_id, order.coins.try_into()?, false)
            .await?;

        Ok(new_balance)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use academy_demo::user::FOO;
    use academy_persistence_contracts::{coin::MockCoinRepository, paypal::MockPaypalRepository};
    use academy_shared_contracts::time::MockTimeService;

    use super::*;

    type Sut = PaypalCoinOrderServiceImpl<
        MockTimeService,
        MockPaypalRepository<()>,
        MockCoinRepository<()>,
    >;

    #[tokio::test]
    async fn create() {
        // Arrange
        let expected = PaypalCoinOrder {
            id: "asdf1234".try_into().unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
        };

        let time = MockTimeService::new().with_now(expected.created_at);

        let paypal_repo = MockPaypalRepository::new()
            .with_get_next_invoice_number(expected.invoice_number)
            .with_create_coin_order(expected.clone());

        let sut = PaypalCoinOrderServiceImpl {
            time,
            paypal_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .create(
                &mut (),
                expected.id.clone(),
                expected.user_id,
                expected.coins,
            )
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn capture() {
        // Arrange
        let order = PaypalCoinOrder {
            id: "asdf1234".try_into().unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
        };
        let now = order.created_at + Duration::from_secs(300);

        let expected = Balance {
            coins: 12345,
            withheld_coins: 17,
        };

        let time = MockTimeService::new().with_now(now);

        let paypal_repo =
            MockPaypalRepository::new().with_capture_coin_order(order.id.clone(), now);

        let coin_repo = MockCoinRepository::new().with_add_coins(
            order.user_id,
            order.coins as _,
            false,
            Ok(expected),
        );

        let sut = PaypalCoinOrderServiceImpl {
            time,
            paypal_repo,
            coin_repo,
        };

        // Act
        let result = sut.capture(&mut (), order).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}
