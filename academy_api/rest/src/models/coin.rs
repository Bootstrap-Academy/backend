use academy_models::coin::Balance;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ApiBalance {
    /// Number of Morphcoins the user owns
    pub coins: u64,
    /// Number of Morphcoins withheld until the user completes their invoice
    /// info
    pub withheld_coins: u64,
}

impl From<Balance> for ApiBalance {
    fn from(value: Balance) -> Self {
        Self {
            coins: value.coins,
            withheld_coins: value.withheld_coins,
        }
    }
}
