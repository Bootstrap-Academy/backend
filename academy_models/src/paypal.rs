use chrono::{DateTime, Utc};

use crate::{macros::nutype_string, user::UserId, withdrawal::WithdrawalTextVersion};

nutype_string!(PaypalOrderId(validate(len_char_max = 256)));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaypalCoinOrder {
    pub id: PaypalOrderId,
    pub user_id: UserId,
    pub created_at: DateTime<Utc>,
    pub captured_at: Option<DateTime<Utc>>,
    pub coins: u64,
    pub invoice_number: u64,
    /// Time at which the consumer gave the declarations required by
    /// § 356 Abs. 6 Nr. 2 BGB. `None` for orders created before these
    /// declarations were collected.
    pub withdrawal_consent_at: Option<DateTime<Utc>>,
    /// Version of the withdrawal instruction the declarations were taken from.
    pub withdrawal_text_version: Option<WithdrawalTextVersion>,
}
