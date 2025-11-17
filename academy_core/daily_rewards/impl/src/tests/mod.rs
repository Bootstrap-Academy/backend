use crate::{DailyRewardCoinsConfig, DailyRewardFeatureConfig};

pub mod activity;
pub mod claim;
pub mod ready;
pub mod snapshot;

impl Default for DailyRewardFeatureConfig {
    fn default() -> Self {
        Self {
            enable: true,
            coins: DailyRewardCoinsConfig {
                arrival: 20,
                lecture: 20,
                practice: 10,
                lab: 30,
            },
            cache_ttl: None,
        }
    }
}
