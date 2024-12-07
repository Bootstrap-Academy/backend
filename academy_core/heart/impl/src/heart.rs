use academy_core_heart_contracts::heart::{HeartAddError, HeartService};
use academy_di::Build;
use academy_models::{heart::Hearts, user::UserId};
use academy_persistence_contracts::heart::HeartRepository;
use academy_shared_contracts::time::TimeService;
use academy_utils::trace_instrument;
use chrono::{DateTime, TimeDelta, Utc};

use crate::HeartFeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct HeartServiceImpl<Time, HeartRepo> {
    time: Time,
    heart_repo: HeartRepo,
    config: HeartFeatureConfig,
}

impl<Txn, Time, HeartRepo> HeartService<Txn> for HeartServiceImpl<Time, HeartRepo>
where
    Txn: Send + Sync + 'static,
    Time: TimeService,
    HeartRepo: HeartRepository<Txn>,
{
    #[trace_instrument(skip(self, txn))]
    async fn get(&self, txn: &mut Txn, user_id: UserId) -> anyhow::Result<Hearts> {
        Ok(self.apply_auto_refill(self.heart_repo.get(txn, user_id).await?))
    }

    #[trace_instrument(skip(self, txn))]
    async fn add(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        hearts: i64,
    ) -> Result<Hearts, HeartAddError> {
        let current = self.apply_auto_refill(self.heart_repo.get(txn, user_id).await?);

        let hearts = Hearts {
            hearts: u64::try_from(current.hearts as i64 + hearts)
                .map_err(|_| HeartAddError::NotEnoughHearts)?
                .min(self.config.hearts_max),
            ..current
        };

        self.heart_repo.set(txn, user_id, hearts).await?;

        Ok(hearts)
    }
}

impl<Time, HeartRepo> HeartServiceImpl<Time, HeartRepo>
where
    Time: TimeService,
{
    fn apply_auto_refill(&self, hearts: Option<Hearts>) -> Hearts {
        let last_auto_refill = self.last_auto_refill();
        match hearts {
            Some(hearts) if hearts.last_refill >= last_auto_refill => hearts,
            _ => Hearts {
                hearts: self.config.hearts_max,
                last_refill: last_auto_refill,
            },
        }
    }

    fn last_auto_refill(&self) -> DateTime<Utc> {
        let now = self.time.now();
        let today_with_time = now.with_time(self.config.auto_refill_time).unwrap();
        if today_with_time <= now {
            today_with_time
        } else {
            today_with_time - TimeDelta::days(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use academy_demo::user::FOO;
    use academy_persistence_contracts::heart::MockHeartRepository;
    use academy_shared_contracts::time::MockTimeService;
    use academy_utils::assert_matches;
    use chrono::{TimeZone, Utc};

    use super::*;

    type Sut = HeartServiceImpl<MockTimeService, MockHeartRepository<()>>;

    #[tokio::test]
    async fn get() {
        // Arrange
        let hearts = Hearts {
            hearts: 4,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(hearts.last_refill + Duration::from_secs(60));

        let heart_repo = MockHeartRepository::new().with_get(FOO.user.id, Some(hearts));

        let sut = HeartServiceImpl {
            time,
            heart_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.get(&mut (), FOO.user.id).await.unwrap();

        // Assert
        assert_eq!(result, hearts);
    }

    #[tokio::test]
    async fn set_updated() {
        // Arrange
        let hearts = Hearts {
            hearts: 4,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        };
        let expected = Hearts {
            hearts: 3,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(expected.last_refill + Duration::from_secs(60));

        let heart_repo = MockHeartRepository::new()
            .with_get(FOO.user.id, Some(hearts))
            .with_set(FOO.user.id, expected);

        let sut = HeartServiceImpl {
            time,
            heart_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.add(&mut (), FOO.user.id, -1).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn set_updated_max() {
        // Arrange
        let hearts = Hearts {
            hearts: 4,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        };
        let expected = Hearts {
            hearts: 6,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(expected.last_refill + Duration::from_secs(60));

        let heart_repo = MockHeartRepository::new()
            .with_get(FOO.user.id, Some(hearts))
            .with_set(FOO.user.id, expected);

        let sut = HeartServiceImpl {
            time,
            heart_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.add(&mut (), FOO.user.id, 7).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn set_auto_refill() {
        // Arrange
        let hearts = Hearts {
            hearts: 4,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        };
        let expected = Hearts {
            hearts: 1,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(expected.last_refill + Duration::from_secs(60));

        let heart_repo = MockHeartRepository::new()
            .with_get(FOO.user.id, Some(hearts))
            .with_set(FOO.user.id, expected);

        let sut = HeartServiceImpl {
            time,
            heart_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.add(&mut (), FOO.user.id, -5).await;

        // Assert
        assert_eq!(result.unwrap(), expected);
    }

    #[tokio::test]
    async fn set_not_enough_hearts() {
        // Arrange
        let hearts = Hearts {
            hearts: 4,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(hearts.last_refill + Duration::from_secs(60));

        let heart_repo = MockHeartRepository::new().with_get(FOO.user.id, Some(hearts));

        let sut = HeartServiceImpl {
            time,
            heart_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.add(&mut (), FOO.user.id, -5).await;

        // Assert
        assert_matches!(result, Err(HeartAddError::NotEnoughHearts));
    }

    #[test]
    fn apply_auto_refill_no_record() {
        // Arrange
        let expected = Hearts {
            hearts: 6,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(expected.last_refill + Duration::from_secs(900));

        let sut = HeartServiceImpl {
            time,
            ..Sut::default()
        };

        // Act
        let result = sut.apply_auto_refill(None);

        // Assert
        assert_eq!(result, expected);
    }

    #[test]
    fn apply_auto_refill_up_to_date() {
        // Arrange
        let expected = Hearts {
            hearts: 4,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(expected.last_refill + Duration::from_secs(900));

        let sut = HeartServiceImpl {
            time,
            ..Sut::default()
        };

        // Act
        let result = sut.apply_auto_refill(Some(expected));

        // Assert
        assert_eq!(result, expected);
    }

    #[test]
    fn apply_auto_refill_pending() {
        // Arrange
        let expected = Hearts {
            hearts: 6,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).unwrap(),
        };

        let time = MockTimeService::new().with_now(expected.last_refill + Duration::from_secs(900));

        let sut = HeartServiceImpl {
            time,
            ..Sut::default()
        };

        // Act
        let result = sut.apply_auto_refill(Some(Hearts {
            hearts: 4,
            last_refill: Utc.with_ymd_and_hms(2024, 1, 1, 1, 0, 0).unwrap(),
        }));

        // Assert
        assert_eq!(result, expected);
    }

    #[test]
    fn last_auto_refill_yesterday() {
        // Arrange
        let expected = Utc.with_ymd_and_hms(2024, 1, 2, 1, 0, 0).unwrap();
        let time =
            MockTimeService::new().with_now(Utc.with_ymd_and_hms(2024, 1, 3, 0, 45, 0).unwrap());

        let sut = HeartServiceImpl {
            time,
            ..Sut::default()
        };

        // Act
        let result = sut.last_auto_refill();

        // Act
        assert_eq!(result, expected);
    }

    #[test]
    fn last_auto_refill_today() {
        // Arrange
        let expected = Utc.with_ymd_and_hms(2024, 1, 3, 1, 0, 0).unwrap();
        let time =
            MockTimeService::new().with_now(Utc.with_ymd_and_hms(2024, 1, 3, 1, 15, 0).unwrap());

        let sut = HeartServiceImpl {
            time,
            ..Sut::default()
        };

        // Act
        let result = sut.last_auto_refill();

        // Act
        assert_eq!(result, expected);
    }
}
