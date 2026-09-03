use academy_models::coin::{Balance, CoinConfig};
use rust_decimal::prelude::ToPrimitive;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, JsonSchema)]
pub struct ApiCoinConfig {
    /// Number of Morphcoins that correspond to one Euro
    pub coins_per_euro: u64,
    /// Vat percentage included in all prices
    pub vat_percent: f64,
}

impl From<CoinConfig> for ApiCoinConfig {
    fn from(value: CoinConfig) -> Self {
        Self {
            coins_per_euro: value.coins_per_euro,
            vat_percent: value.vat_percent.to_f64().unwrap_or_default(),
        }
    }
}

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
