use rust_decimal::Decimal;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinanceCoinService: Send + Sync + 'static {
    /// Return the configured vat percentage.
    fn vat_percent(&self) -> Decimal;

    /// Return the number of Morphcoins that correspond to one Euro.
    fn coins_per_euro(&self) -> u64;

    /// Calculate the prices to purchase the given number of morphcoins.
    fn get_price(&self, coins: u64) -> CoinPrices;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoinPrices {
    pub net_unit: Decimal,
    pub net_total: Decimal,
    pub vat_total: Decimal,
    pub gross_total: Decimal,
}

#[cfg(feature = "mock")]
impl MockFinanceCoinService {
    pub fn with_vat_percent(mut self, result: Decimal) -> Self {
        self.expect_vat_percent()
            .once()
            .with()
            .return_once(move || result);
        self
    }

    pub fn with_coins_per_euro(mut self, result: u64) -> Self {
        self.expect_coins_per_euro()
            .once()
            .with()
            .return_once(move || result);
        self
    }

    pub fn with_get_price(mut self, coins: u64, result: CoinPrices) -> Self {
        self.expect_get_price()
            .once()
            .with(mockall::predicate::eq(coins))
            .return_once(move |_| result);
        self
    }
}
