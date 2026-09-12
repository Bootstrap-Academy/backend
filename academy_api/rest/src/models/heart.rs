use academy_models::{
    heart::{HeartConfig, HeartOperationId, HeartOperationOutcome, HeartOperationReceipt, Hearts},
    user::UserId,
};
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

#[derive(Debug, Serialize, JsonSchema)]
pub struct ApiHeartOperationReceipt {
    pub operation_id: HeartOperationId,
    pub user_id: UserId,
    pub charged_half_hearts: u64,
    pub hearts: u64,
    pub outcome: &'static str,
}

impl From<HeartOperationReceipt> for ApiHeartOperationReceipt {
    fn from(value: HeartOperationReceipt) -> Self {
        Self {
            operation_id: value.operation_id,
            user_id: value.user_id,
            charged_half_hearts: value.charged_half_hearts,
            hearts: value.hearts,
            outcome: match value.outcome {
                HeartOperationOutcome::Charged => "charged",
                HeartOperationOutcome::Premium => "premium",
                HeartOperationOutcome::Insufficient => "insufficient",
            },
        }
    }
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
