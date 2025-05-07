use academy_auth_contracts::MockAuthService;
use academy_core_coin_contracts::coin::MockCoinService;
use academy_core_heart_contracts::heart::MockHeartService;
use academy_persistence_contracts::{MockDatabase, MockTransaction, user::MockUserRepository};
use chrono::NaiveTime;

use crate::{HeartFeatureConfig, HeartFeatureServiceImpl};

mod get;
mod get_config;
mod refill;

type Sut = HeartFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockHeartService<MockTransaction>,
    MockCoinService<MockTransaction>,
>;

impl Default for HeartFeatureConfig {
    fn default() -> Self {
        Self {
            hearts_max: 6,
            hearts_refill_price: 50,
            auto_refill_time: NaiveTime::from_hms_opt(1, 0, 0).unwrap(),
        }
    }
}
