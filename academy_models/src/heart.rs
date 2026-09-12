use chrono::{DateTime, Utc};

use crate::{macros::id, user::UserId};

id!(HeartOperationId);

/// A stable identifier belongs to exactly one final incorrect attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartOperation {
    pub id: HeartOperationId,
    pub user_id: UserId,
    pub half_hearts: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartOperationOutcome {
    Charged,
    Premium,
    Insufficient,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartOperationReceipt {
    pub operation_id: HeartOperationId,
    pub user_id: UserId,
    pub charged_half_hearts: u64,
    pub hearts: u64,
    pub outcome: HeartOperationOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartOperationClaim {
    New,
    Completed(HeartOperationReceipt),
    Conflict,
}

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
