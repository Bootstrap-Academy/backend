use academy_core_coin_contracts::CoinFeatureService;
use academy_core_finance_contracts::coin::MockFinanceCoinService;
use academy_models::coin::CoinConfig;
use rust_decimal_macros::dec;

use crate::{CoinFeatureServiceImpl, tests::Sut};

#[test]
fn ok() {
    // Arrange
    let finance_coin = MockFinanceCoinService::new()
        .with_coins_per_euro(100)
        .with_vat_percent(dec!(19));

    let sut = CoinFeatureServiceImpl {
        finance_coin,
        ..Sut::default()
    };

    // Act
    let result = sut.get_config();

    // Assert
    assert_eq!(
        result,
        CoinConfig {
            coins_per_euro: 100,
            vat_percent: dec!(19),
        }
    );
}
