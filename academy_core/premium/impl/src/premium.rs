use academy_core_premium_contracts::{
    premium::PremiumService,
    purchase::{PremiumPurchaseError, PremiumPurchaseService},
};
use academy_di::Build;
use academy_models::{premium::Premium, user::UserId};
use academy_persistence_contracts::premium::PremiumRepository;
use academy_shared_contracts::time::TimeService;
use academy_utils::trace_instrument;

#[derive(Debug, Clone, Build, Default)]
pub struct PremiumServiceImpl<Time, PremiumPurchase, PremiumRepo> {
    time: Time,
    premium_purchase: PremiumPurchase,
    premium_repo: PremiumRepo,
}

impl<Txn, Time, PremiumPurchase, PremiumRepo> PremiumService<Txn>
    for PremiumServiceImpl<Time, PremiumPurchase, PremiumRepo>
where
    Txn: Send + Sync + 'static,
    Time: TimeService,
    PremiumPurchase: PremiumPurchaseService<Txn>,
    PremiumRepo: PremiumRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn get_active(&self, txn: &mut Txn, user_id: UserId) -> anyhow::Result<Option<Premium>> {
        let now = self.time.now();

        if let Some(active) = self
            .premium_repo
            .get_latest_by_user_id(txn, user_id)
            .await?
            .filter(|premium| now < premium.until)
        {
            return Ok(Some(active));
        }

        let Some(plan) = self.premium_repo.get_subscription(txn, user_id).await? else {
            return Ok(None);
        };

        match self.premium_purchase.purchase(txn, user_id, plan).await {
            Ok(premium) => Ok(Some(premium)),
            Err(PremiumPurchaseError::NotEnoughCoins) => {
                self.premium_repo
                    .set_subscription(txn, user_id, None)
                    .await?;
                Ok(None)
            }
            Err(PremiumPurchaseError::Other(err)) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use academy_core_premium_contracts::purchase::MockPremiumPurchaseService;
    use academy_demo::{user::FOO, UUID1};
    use academy_models::premium::PremiumPlan;
    use academy_persistence_contracts::premium::MockPremiumRepository;
    use academy_shared_contracts::time::MockTimeService;
    use chrono::{TimeZone, Utc};

    use super::*;

    type Sut = PremiumServiceImpl<
        MockTimeService,
        MockPremiumPurchaseService<()>,
        MockPremiumRepository<()>,
    >;

    #[tokio::test]
    async fn ok_active() {
        // Arrange
        let expected = Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            until: Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
        };

        let now = Utc.with_ymd_and_hms(2025, 1, 15, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let premium_repo =
            MockPremiumRepository::new().with_get_latest_by_user_id(FOO.user.id, Some(expected));

        let sut = PremiumServiceImpl {
            time,
            premium_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.get_active(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), Some(expected));
    }

    #[tokio::test]
    async fn ok_subscription_purchase() {
        // Arrange
        let expected = Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            until: Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
        };

        let now = Utc.with_ymd_and_hms(2025, 2, 15, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let premium_repo = MockPremiumRepository::new()
            .with_get_latest_by_user_id(FOO.user.id, None)
            .with_get_subscription(FOO.user.id, Some(PremiumPlan::Monthly));

        let premium_purchase = MockPremiumPurchaseService::new().with_purchase(
            FOO.user.id,
            PremiumPlan::Monthly,
            Ok(expected),
        );

        let sut = PremiumServiceImpl {
            time,
            premium_repo,
            premium_purchase,
        };

        // Act
        let result = sut.get_active(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), Some(expected));
    }

    #[tokio::test]
    async fn expired_no_subscription() {
        // Arrange
        let expected = Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
            until: Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap(),
        };

        let now = Utc.with_ymd_and_hms(2025, 2, 15, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let premium_repo = MockPremiumRepository::new()
            .with_get_latest_by_user_id(FOO.user.id, Some(expected))
            .with_get_subscription(FOO.user.id, None);

        let sut = PremiumServiceImpl {
            time,
            premium_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.get_active(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn subscription_not_enough_coins() {
        // Arrange
        let now = Utc.with_ymd_and_hms(2025, 2, 15, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let premium_repo = MockPremiumRepository::new()
            .with_get_latest_by_user_id(FOO.user.id, None)
            .with_get_subscription(FOO.user.id, Some(PremiumPlan::Monthly))
            .with_set_subscription(FOO.user.id, None);

        let premium_purchase = MockPremiumPurchaseService::new().with_purchase(
            FOO.user.id,
            PremiumPlan::Monthly,
            Err(PremiumPurchaseError::NotEnoughCoins),
        );

        let sut = PremiumServiceImpl {
            time,
            premium_repo,
            premium_purchase,
        };

        // Act
        let result = sut.get_active(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }
}
