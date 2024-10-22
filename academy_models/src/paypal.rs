use chrono::{DateTime, Utc};

use crate::{macros::nutype_string, user::UserId};

nutype_string!(PaypalOrderId(validate(len_char_max = 256)));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaypalCoinOrder {
    pub id: PaypalOrderId,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub captured_at: Option<DateTime<Utc>>,
    pub coins: u64,
    pub invoice_number: u64,
}
