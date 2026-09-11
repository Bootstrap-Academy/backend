use academy_core_coin_contracts::coin::{CoinAddCoinsError, CoinService};
use academy_core_premium_contracts::{
    plan::PremiumPlanService,
    purchase::{PremiumPurchaseError, PremiumPurchaseService},
};
use academy_di::Build;
use academy_models::{
    premium::{Premium, PremiumPlan, PremiumPlanDetails},
    user::UserId,
};
use academy_persistence_contracts::premium::PremiumRepository;
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;

use crate::period::add_months;

#[derive(Debug, Clone, Build, Default)]
pub struct PremiumPurchaseServiceImpl<Id, Time, Coin, PremiumPlanS, PremiumRepo> {
    id: Id,
    time: Time,
    coin: Coin,
    premium_plan: PremiumPlanS,
    premium_repo: PremiumRepo,
}

impl<Txn, Id, Time, Coin, PremiumPlanS, PremiumRepo> PremiumPurchaseService<Txn>
    for PremiumPurchaseServiceImpl<Id, Time, Coin, PremiumPlanS, PremiumRepo>
where
    Txn: Send + Sync + 'static,
    Id: IdService,
    Time: TimeService,
    Coin: CoinService<Txn>,
    PremiumPlanS: PremiumPlanService,
    PremiumRepo: PremiumRepository<Txn>,
{
    async fn renew(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        monthly_price: u64,
    ) -> Result<Premium, PremiumPurchaseError> {
        self.purchase_details(
            txn,
            user_id,
            PremiumPlanDetails {
                price: monthly_price,
                months: 1,
            },
        )
        .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn purchase(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        plan: PremiumPlan,
    ) -> Result<Premium, PremiumPurchaseError> {
        let details = self.premium_plan.get_details(plan);

        self.purchase_details(txn, user_id, details).await
    }
}

impl<Id, Time, Coin, PremiumPlanS, PremiumRepo>
    PremiumPurchaseServiceImpl<Id, Time, Coin, PremiumPlanS, PremiumRepo>
{
    async fn purchase_details<Txn>(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        details: PremiumPlanDetails,
    ) -> Result<Premium, PremiumPurchaseError>
    where
        Txn: Send + Sync + 'static,
        Id: IdService,
        Time: TimeService,
        Coin: CoinService<Txn>,
        PremiumRepo: PremiumRepository<Txn>,
    {
        // Lock/read before the balance debit, in the same order as cancellation
        // and automatic renewal, to avoid concurrent double extensions/deadlocks.
        let latest = self
            .premium_repo
            .get_latest_by_user_id(txn, user_id)
            .await?;
        self.coin
            .add_coins(
                txn,
                user_id,
                -(details.price as i64),
                false,
                Some("Premium".try_into().unwrap()),
                false,
            )
            .await
            .map_err(|err| match err {
                CoinAddCoinsError::NotEnoughCoins => PremiumPurchaseError::NotEnoughCoins,
                CoinAddCoinsError::Other(err) => err.into(),
            })?;

        let now = self.time.now();

        // The period sold is a calendar period, so it is added to the day the
        // membership currently runs to (or to today, if it does not run at
        // all), not as a fixed number of days.
        let months = details.months.try_into().unwrap_or(u32::MAX);

        if let Some(mut active) = latest.filter(|premium| now < premium.until) {
            active.until = add_months(active.until, months);
            self.premium_repo
                .extend(txn, active.id, active.until)
                .await?;
            Ok(active)
        } else {
            let premium = Premium {
                id: self.id.generate(),
                user_id,
                since: now,
                until: add_months(now, months),
            };
            self.premium_repo.create(txn, premium).await?;
            Ok(premium)
        }
    }
}

#[cfg(test)]
mod tests {
    use academy_core_coin_contracts::coin::MockCoinService;
    use academy_core_premium_contracts::plan::MockPremiumPlanService;
    use academy_demo::{UUID1, UUID2, user::FOO};
    use academy_models::{coin::Balance, premium::PremiumPlanDetails};
    use academy_persistence_contracts::premium::MockPremiumRepository;
    use academy_shared_contracts::{id::MockIdService, time::MockTimeService};
    use academy_utils::assert_matches;
    use chrono::{TimeZone, Utc};

    use super::*;

    type Sut = PremiumPurchaseServiceImpl<
        MockIdService,
        MockTimeService,
        MockCoinService<()>,
        MockPremiumPlanService,
        MockPremiumRepository<()>,
    >;

    /// An active membership is extended by a calendar month from the day it
    /// currently runs to.
    #[tokio::test]
    async fn ok_extend() {
        // Arrange
        let now = Utc.with_ymd_and_hms(2025, 1, 1, 13, 0, 0).unwrap();
        let old_until = Utc.with_ymd_and_hms(2025, 1, 1, 18, 0, 0).unwrap();
        let expected = Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: Utc.with_ymd_and_hms(2024, 12, 1, 18, 0, 0).unwrap(),
            until: Utc.with_ymd_and_hms(2025, 2, 1, 18, 0, 0).unwrap(),
        };

        let premium_plan = MockPremiumPlanService::new().with_get_details(
            PremiumPlan::Monthly,
            PremiumPlanDetails {
                price: 1000,
                months: 1,
            },
        );

        let coin = MockCoinService::new().with_add_coins(
            FOO.user.id,
            -1000,
            false,
            Some("Premium".try_into().unwrap()),
            false,
            Ok(Balance {
                coins: 0,
                withheld_coins: 0,
            }),
        );

        let time = MockTimeService::new().with_now(now);

        let premium_repo = MockPremiumRepository::new()
            .with_get_latest_by_user_id(
                FOO.user.id,
                Some(Premium {
                    until: old_until,
                    ..expected
                }),
            )
            .with_extend(expected.id, expected.until);

        let sut = PremiumPurchaseServiceImpl {
            premium_plan,
            coin,
            time,
            premium_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .purchase(&mut (), FOO.user.id, PremiumPlan::Monthly)
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    /// An expired membership starts a new period of a calendar month from
    /// today. `now` is 00:00 on 2 January in `Europe/Berlin`, which is the day
    /// the period is counted from.
    #[tokio::test]
    async fn ok_create() {
        // Arrange
        let now = Utc.with_ymd_and_hms(2025, 1, 1, 23, 0, 0).unwrap();
        let old_until = Utc.with_ymd_and_hms(2025, 1, 1, 18, 0, 0).unwrap();
        let expected = Premium {
            id: UUID2.into(),
            user_id: FOO.user.id,
            since: now,
            until: Utc.with_ymd_and_hms(2025, 2, 1, 23, 0, 0).unwrap(),
        };

        let premium_plan = MockPremiumPlanService::new().with_get_details(
            PremiumPlan::Monthly,
            PremiumPlanDetails {
                price: 1000,
                months: 1,
            },
        );

        let coin = MockCoinService::new().with_add_coins(
            FOO.user.id,
            -1000,
            false,
            Some("Premium".try_into().unwrap()),
            false,
            Ok(Balance {
                coins: 0,
                withheld_coins: 0,
            }),
        );

        let time = MockTimeService::new().with_now(now);

        let premium_repo = MockPremiumRepository::new()
            .with_get_latest_by_user_id(
                FOO.user.id,
                Some(Premium {
                    until: old_until,
                    ..expected
                }),
            )
            .with_create(expected);

        let id = MockIdService::new().with_generate(expected.id);

        let sut = PremiumPurchaseServiceImpl {
            premium_plan,
            coin,
            time,
            premium_repo,
            id,
        };

        // Act
        let result = sut
            .purchase(&mut (), FOO.user.id, PremiumPlan::Monthly)
            .await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn not_enough_coins() {
        // Arrange
        let premium_plan = MockPremiumPlanService::new().with_get_details(
            PremiumPlan::Monthly,
            PremiumPlanDetails {
                price: 1000,
                months: 1,
            },
        );

        let coin = MockCoinService::new().with_add_coins(
            FOO.user.id,
            -1000,
            false,
            Some("Premium".try_into().unwrap()),
            false,
            Err(CoinAddCoinsError::NotEnoughCoins),
        );

        let sut = PremiumPurchaseServiceImpl {
            premium_repo: MockPremiumRepository::new()
                .with_get_latest_by_user_id(FOO.user.id, None),
            premium_plan,
            coin,
            ..Sut::default()
        };

        // Act
        let result = sut
            .purchase(&mut (), FOO.user.id, PremiumPlan::Monthly)
            .await;

        // Assert
        assert_matches!(result, Err(PremiumPurchaseError::NotEnoughCoins));
    }
}
