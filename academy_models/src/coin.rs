use crate::macros::nutype_string;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Balance {
    pub coins: u64,
    pub withheld_coins: u64,
}

nutype_string!(TransactionDescription(validate(len_char_max = 4096)));
