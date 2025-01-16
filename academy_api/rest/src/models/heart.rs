use academy_models::heart::{HeartConfig, Hearts};
use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiHeartConfig {
    pub hearts_max: u64,
    pub hearts_refill_price: u64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiHearts {
    pub hearts: u64,
}

impl From<HeartConfig> for ApiHeartConfig {
    fn from(value: HeartConfig) -> Self {
        Self {
            hearts_max: value.hearts_max,
            hearts_refill_price: value.hearts_refill_price,
        }
    }
}

impl From<Hearts> for ApiHearts {
    fn from(value: Hearts) -> Self {
        Self {
            hearts: value.hearts,
        }
    }
}
