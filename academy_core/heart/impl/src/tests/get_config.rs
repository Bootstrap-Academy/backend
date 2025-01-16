use academy_core_heart_contracts::HeartFeatureService;
use academy_models::heart::HeartConfig;

use super::Sut;

#[test]
fn ok() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut.get_config();

    // Assert
    assert_eq!(
        result,
        HeartConfig {
            hearts_max: 6,
            hearts_refill_price: 50
        }
    );
}
