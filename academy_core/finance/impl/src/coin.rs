use academy_core_finance_contracts::coin::{CoinPrices, FinanceCoinService};
use academy_di::Build;
use academy_utils::trace_instrument;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::FinanceFeatureConfig;

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
    fn get_price(&self, coins: u64) -> CoinPrices {
        let vat_factor = self.config.vat_percent / dec!(100);

        let net_unit = dec!(0.01) / (dec!(1) + vat_factor);
        let net_total = net_unit * Decimal::from(coins);
        let vat_total = net_total * vat_factor;
        let gross_total = dec!(0.01) * Decimal::from(coins);

        debug_assert_eq!(gross_total, (net_total + vat_total).round_dp(4));

        CoinPrices {
            net_unit,
            net_total,
            vat_total,
            gross_total,
        }
    }
}
