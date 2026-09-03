use academy_core_coin_contracts::coin::CoinService;
use academy_core_paypal_contracts::coin_order::PaypalCoinOrderService;
use academy_di::Build;
use academy_models::{
    coin::Balance,
    paypal::{PaypalCoinOrder, PaypalOrderId},
    user::UserId,
    withdrawal::WithdrawalTextVersion,
};
use academy_persistence_contracts::paypal::PaypalRepository;
use academy_shared_contracts::time::TimeService;
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build, Default)]
pub struct PaypalCoinOrderServiceImpl<Time, PaypalRepo, Coin> {
    time: Time,
    paypal_repo: PaypalRepo,
    coin: Coin,
}

impl<Txn, Time, PaypalRepo, Coin> PaypalCoinOrderService<Txn>
    for PaypalCoinOrderServiceImpl<Time, PaypalRepo, Coin>
where
    Txn: Send + Sync + 'static,
    Time: TimeService,
    PaypalRepo: PaypalRepository<Txn>,
    Coin: CoinService<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut Txn,
        id: PaypalOrderId,
        user_id: UserId,
        coins: u64,
        withdrawal_text_version: WithdrawalTextVersion,
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
            withdrawal_consent_at: Some(now),
            withdrawal_text_version: Some(withdrawal_text_version),
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
            .coin
            .add_coins(
                txn,
                order.user_id,
                order.coins.try_into()?,
                false,
                Some("PayPal".try_into()?),
                false,
            )
            .await?;

        Ok(new_balance)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use academy_core_coin_contracts::coin::MockCoinService;
    use academy_demo::user::FOO;
    use academy_persistence_contracts::paypal::MockPaypalRepository;
    use academy_shared_contracts::time::MockTimeService;

    use super::*;

    type Sut =
        PaypalCoinOrderServiceImpl<MockTimeService, MockPaypalRepository<()>, MockCoinService<()>>;

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
            withdrawal_consent_at: Some(FOO.user.created_at),
            withdrawal_text_version: Some("2026-09".try_into().unwrap()),
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
                "2026-09".try_into().unwrap(),
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
            withdrawal_consent_at: Some(FOO.user.created_at),
            withdrawal_text_version: Some("2026-09".try_into().unwrap()),
        };
        let now = order.created_at + Duration::from_secs(300);

        let expected = Balance {
            coins: 12345,
            withheld_coins: 17,
        };

        let time = MockTimeService::new().with_now(now);

        let paypal_repo =
            MockPaypalRepository::new().with_capture_coin_order(order.id.clone(), now);

        let coin = MockCoinService::new().with_add_coins(
            order.user_id,
            order.coins as _,
            false,
            Some("PayPal".try_into().unwrap()),
            false,
            Ok(expected),
        );

        let sut = PaypalCoinOrderServiceImpl {
            time,
            paypal_repo,
            coin,
        };

        // Act
        let result = sut.capture(&mut (), order).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }
}
