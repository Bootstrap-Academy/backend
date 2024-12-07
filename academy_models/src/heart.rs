use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartConfig {
    pub hearts_max: u64,
    pub hearts_refill_price: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hearts {
    pub hearts: u64,
    pub last_refill: DateTime<Utc>,
}
