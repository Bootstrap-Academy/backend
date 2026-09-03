use academy_auth_contracts::MockAuthService;
use academy_core_premium_contracts::{
    plan::MockPremiumPlanService, premium::MockPremiumService, purchase::MockPremiumPurchaseService,
};
use academy_core_withdrawal_contracts::consent::MockWithdrawalConsentService;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, premium::MockPremiumRepository, user::MockUserRepository,
};

use crate::{PremiumFeatureConfig, PremiumFeatureServiceImpl};

mod get_plans;
mod get_status;
mod purchase;
mod update_subscription;

type Sut = PremiumFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockPremiumPlanService,
    MockPremiumService<MockTransaction>,
    MockPremiumPurchaseService<MockTransaction>,
    MockUserRepository<MockTransaction>,
    MockPremiumRepository<MockTransaction>,
    MockWithdrawalConsentService<MockTransaction>,
>;

impl Default for PremiumFeatureConfig {
    fn default() -> Self {
        Self {
            monthly_price: 1000,
            yearly_price: 10000,
        }
    }
}
