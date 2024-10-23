use academy_auth_contracts::MockAuthService;
use academy_core_paypal_contracts::coin_order::MockPaypalCoinOrderService;
use academy_extern_contracts::paypal::MockPaypalApiService;
use academy_persistence_contracts::{
    paypal::MockPaypalRepository, user::MockUserRepository, MockDatabase, MockTransaction,
};

use crate::{PaypalFeatureConfig, PaypalFeatureServiceImpl};

mod capture_coin_order;
mod create_coin_order;

type Sut = PaypalFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockPaypalApiService,
    MockUserRepository<MockTransaction>,
    MockPaypalRepository<MockTransaction>,
    MockPaypalCoinOrderService<MockTransaction>,
>;

impl Default for PaypalFeatureConfig {
    fn default() -> Self {
        Self {
            purchase_range: 5..=5000,
        }
    }
}
