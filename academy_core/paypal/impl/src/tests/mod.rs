use std::path::Path;

use academy_auth_contracts::MockAuthService;
use academy_core_paypal_contracts::coin_order::MockPaypalCoinOrderService;
use academy_email_contracts::template::MockTemplateEmailService;
use academy_extern_contracts::paypal::MockPaypalApiService;
use academy_persistence_contracts::{
    paypal::MockPaypalRepository, user::MockUserRepository, MockDatabase, MockTransaction,
};
use academy_render_contracts::pdf::MockRenderPdfService;
use academy_shared_contracts::{fs::MockFsService, time::MockTimeService};
use academy_templates_contracts::MockTemplateService;
use rust_decimal_macros::dec;

use crate::{PaypalFeatureConfig, PaypalFeatureServiceImpl};

mod capture_coin_order;
mod create_coin_order;

type Sut = PaypalFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockTimeService,
    MockPaypalApiService,
    MockUserRepository<MockTransaction>,
    MockPaypalRepository<MockTransaction>,
    MockPaypalCoinOrderService<MockTransaction>,
    MockTemplateService,
    MockTemplateEmailService,
    MockRenderPdfService,
    MockFsService,
>;

impl Default for PaypalFeatureConfig {
    fn default() -> Self {
        Self {
            purchase_range: 5..=5000,
            vat_percent: dec!(19),
            invoices_archive: Path::new("/invoices").into(),
        }
    }
}
