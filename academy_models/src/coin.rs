#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Balance {
    pub coins: u64,
    pub withheld_coins: u64,
}
