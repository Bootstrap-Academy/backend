use chrono::{DateTime, Utc};

use crate::{
    macros::{id, nutype_string},
    user::UserId,
};

id!(TransactionId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Balance {
    pub coins: u64,
    pub withheld_coins: u64,
}

nutype_string!(TransactionDescription(validate(len_char_max = 4096)));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: TransactionId,
    pub user_id: UserId,
    pub coins: i64,
    pub description: Option<TransactionDescription>,
    pub created_at: DateTime<Utc>,
    pub include_in_credit_note: bool,
}
