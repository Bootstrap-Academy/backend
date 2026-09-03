use academy_core_finance_contracts::coin::{CoinPrices, FinanceCoinService};
use academy_di::Build;
use academy_utils::trace_instrument;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::FinanceFeatureConfig;

/// Number of Morphcoins that correspond to one Euro.
const COINS_PER_EURO: u64 = 100;

#[derive(Debug, Clone, Build)]
pub struct FinanceCoinServiceImpl {
    config: FinanceFeatureConfig,
}

impl FinanceCoinService for FinanceCoinServiceImpl {
    #[trace_instrument(skip(self))]
    fn vat_percent(&self) -> Decimal {
        self.config.vat_percent
    }

    #[trace_instrument(skip(self))]
    fn coins_per_euro(&self) -> u64 {
        COINS_PER_EURO
    }

    #[trace_instrument(skip(self))]
    fn get_price(&self, coins: u64) -> CoinPrices {
        let vat_factor = self.config.vat_percent / dec!(100);

        let gross_unit = Decimal::ONE / Decimal::from(COINS_PER_EURO);
        let net_unit = gross_unit / (dec!(1) + vat_factor);
        let net_total = net_unit * Decimal::from(coins);
        let vat_total = net_total * vat_factor;
        let gross_total = gross_unit * Decimal::from(coins);

        debug_assert_eq!(gross_total, (net_total + vat_total).round_dp(4));

        CoinPrices {
            net_unit,
            net_total,
            vat_total,
            gross_total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sut() -> FinanceCoinServiceImpl {
        FinanceCoinServiceImpl {
            config: Default::default(),
        }
    }

    #[test]
    fn coins_per_euro() {
        assert_eq!(sut().coins_per_euro(), 100);
    }

    #[test]
    fn get_price() {
        // 500 Morphcoins cost 5,00 € including 19 % vat
        let result = sut().get_price(500);

        assert_eq!(result.gross_total, dec!(5));
        assert_eq!(result.net_total.round_dp(2), dec!(4.20));
        assert_eq!(result.vat_total.round_dp(2), dec!(0.80));
        assert_eq!((result.net_total + result.vat_total).round_dp(2), dec!(5));
        assert_eq!((result.net_unit * dec!(500)), result.net_total);
    }
}
