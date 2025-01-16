use academy_core_premium_contracts::plan::PremiumPlanService;
use academy_di::Build;
use academy_models::premium::{PremiumPlan, PremiumPlanDetails};

use crate::PremiumFeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct PremiumPlanServiceImpl {
    config: PremiumFeatureConfig,
}

impl PremiumPlanService for PremiumPlanServiceImpl {
    fn get_details(&self, plan: PremiumPlan) -> PremiumPlanDetails {
        let price = match plan {
            PremiumPlan::Monthly => self.config.monthly_price,
            PremiumPlan::Yearly => self.config.yearly_price,
        };

        let months = match plan {
            PremiumPlan::Monthly => 1,
            PremiumPlan::Yearly => 12,
        };

        PremiumPlanDetails { price, months }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_details() {
        // Arrange
        let sut = PremiumPlanServiceImpl::default();

        // Act
        let result_monthly = sut.get_details(PremiumPlan::Monthly);
        let result_yearly = sut.get_details(PremiumPlan::Yearly);

        // Assert
        assert_eq!(
            result_monthly,
            PremiumPlanDetails {
                price: 1000,
                months: 1
            }
        );
        assert_eq!(
            result_yearly,
            PremiumPlanDetails {
                price: 10000,
                months: 12
            }
        );
    }
}
