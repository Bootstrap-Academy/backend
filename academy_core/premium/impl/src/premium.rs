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
    /// Paid periods are always retained. Only a separately recorded monthly
    /// agreement with its durable confirmation sent can trigger a coin debit.
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

        // Existing paid access above survives restriction. A fresh renewal is
        // serialized with the current account state and cannot debit while
        // disabled, including internal entitlement reads and the renewal task.
        if !self.premium_repo.renewal_allowed(txn, user_id).await? {
            return Ok(None);
        }
        let Some(renewal) = self.premium_repo.get_renewal(txn, user_id).await? else {
            return Ok(None);
        };
        if !renewal.confirmation_sent {
            // Failed confirmation cannot create an invisible delayed charge
            // after the paid membership has expired.
            self.premium_repo
                .set_subscription(txn, user_id, None)
                .await?;
            return Ok(None);
        }

        match self
            .premium_purchase
            .renew(txn, user_id, renewal.monthly_price)
            .await
        {
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
    use super::*;
    use academy_core_premium_contracts::purchase::MockPremiumPurchaseService;
    use academy_demo::{UUID1, user::FOO};
    use academy_models::premium::PremiumRenewalStatus;
    use academy_persistence_contracts::premium::MockPremiumRepository;
    use academy_shared_contracts::time::MockTimeService;
    use chrono::{TimeZone, Utc};

    type Sut = PremiumServiceImpl<
        MockTimeService,
        MockPremiumPurchaseService<()>,
        MockPremiumRepository<()>,
    >;

    fn paid() -> Premium {
        Premium {
            id: UUID1.into(),
            user_id: FOO.user.id,
            since: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            until: Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[tokio::test]
    async fn paid_access_requires_neither_new_terms_nor_renewal_consent() {
        let expected = paid();
        let sut = Sut {
            time: MockTimeService::new().with_now(expected.since),
            premium_repo: MockPremiumRepository::new()
                .with_get_latest_by_user_id(FOO.user.id, Some(expected)),
            ..Sut::default()
        };
        assert_eq!(
            sut.get_active(&mut (), FOO.user.id).await.unwrap(),
            Some(expected)
        );
    }

    #[tokio::test]
    async fn restriction_pauses_new_debit_without_cancelling_agreement() {
        let mut repo =
            MockPremiumRepository::new().with_get_latest_by_user_id(FOO.user.id, Some(paid()));
        repo.expect_renewal_allowed()
            .once()
            .return_once(|_, _| Box::pin(async { Ok(false) }));
        let sut = Sut {
            time: MockTimeService::new().with_now(paid().until),
            premium_repo: repo,
            ..Sut::default()
        };
        assert_eq!(sut.get_active(&mut (), FOO.user.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn missing_agreement_never_debits_or_changes_legacy_plan() {
        let mut repo =
            MockPremiumRepository::new().with_get_latest_by_user_id(FOO.user.id, Some(paid()));
        repo.expect_renewal_allowed()
            .once()
            .return_once(|_, _| Box::pin(async { Ok(true) }));
        repo.expect_get_renewal()
            .once()
            .return_once(|_, _| Box::pin(async { Ok(None) }));
        let sut = Sut {
            time: MockTimeService::new().with_now(paid().until),
            premium_repo: repo,
            ..Sut::default()
        };
        assert_eq!(sut.get_active(&mut (), FOO.user.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn confirmation_failure_at_expiry_disables_late_debits() {
        let mut repo = MockPremiumRepository::new()
            .with_get_latest_by_user_id(FOO.user.id, Some(paid()))
            .with_set_subscription(FOO.user.id, None);
        repo.expect_renewal_allowed()
            .once()
            .return_once(|_, _| Box::pin(async { Ok(true) }));
        repo.expect_get_renewal().once().return_once(|_, _| {
            Box::pin(async {
                Ok(Some(PremiumRenewalStatus {
                    id: UUID1.into(),
                    monthly_price: 750,
                    confirmation_sent: false,
                }))
            })
        });
        let sut = Sut {
            time: MockTimeService::new().with_now(paid().until),
            premium_repo: repo,
            ..Sut::default()
        };
        assert_eq!(sut.get_active(&mut (), FOO.user.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn confirmed_renewal_uses_agreed_price_and_handles_insufficient_coins() {
        for enough in [true, false] {
            let mut repo =
                MockPremiumRepository::new().with_get_latest_by_user_id(FOO.user.id, Some(paid()));
            repo.expect_renewal_allowed()
                .once()
                .return_once(|_, _| Box::pin(async { Ok(true) }));
            repo.expect_get_renewal().once().return_once(|_, _| {
                Box::pin(async {
                    Ok(Some(PremiumRenewalStatus {
                        id: UUID1.into(),
                        monthly_price: 750,
                        confirmation_sent: true,
                    }))
                })
            });
            if !enough {
                repo = repo.with_set_subscription(FOO.user.id, None);
            }
            let mut purchase = MockPremiumPurchaseService::new();
            purchase
                .expect_renew()
                .once()
                .withf(|_, user_id, price| *user_id == FOO.user.id && *price == 750)
                .return_once(move |_, _, _| {
                    Box::pin(async move {
                        if enough {
                            Ok(paid())
                        } else {
                            Err(PremiumPurchaseError::NotEnoughCoins)
                        }
                    })
                });
            let sut = Sut {
                time: MockTimeService::new().with_now(paid().until),
                premium_repo: repo,
                premium_purchase: purchase,
            };
            assert_eq!(
                sut.get_active(&mut (), FOO.user.id).await.unwrap(),
                enough.then(paid)
            );
        }
    }
}
