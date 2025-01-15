use academy_core_premium_contracts::{plan::MockPremiumPlanService, PremiumFeatureService};
use academy_models::premium::{PremiumPlan, PremiumPlanDetails};

use super::Sut;
use crate::PremiumFeatureServiceImpl;

#[test]
fn ok() {
    // Arrange
    let monthly = PremiumPlanDetails {
        price: 1000,
        months: 1,
    };
    let yearly = PremiumPlanDetails {
        price: 10000,
        months: 12,
    };
    let premium_plan = MockPremiumPlanService::new()
        .with_get_details(PremiumPlan::Monthly, monthly)
        .with_get_details(PremiumPlan::Yearly, yearly);

    let sut = PremiumFeatureServiceImpl {
        premium_plan,
        ..Sut::default()
    };

    // Act
    let result = sut.get_plans();

    // Assert
    assert_eq!(
        result,
        [
            (PremiumPlan::Monthly, monthly),
            (PremiumPlan::Yearly, yearly)
        ]
        .into()
    );
}
