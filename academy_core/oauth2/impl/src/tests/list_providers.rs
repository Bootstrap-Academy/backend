use academy_core_oauth2_contracts::OAuth2FeatureService;
use academy_demo::oauth2::{TEST_OAUTH2_PROVIDER, TEST_OAUTH2_PROVIDER_ID};
use academy_models::oauth2::OAuth2ProviderSummary;

use super::Sut;

#[test]
fn ok() {
    // Arrange
    let sut = Sut::default();

    // Act
    let result = sut.list_providers();

    // Assert
    assert_eq!(
        result,
        [OAuth2ProviderSummary {
            id: TEST_OAUTH2_PROVIDER_ID.clone(),
            name: TEST_OAUTH2_PROVIDER.name.clone(),
        }]
    )
}
