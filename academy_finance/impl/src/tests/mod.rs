use std::path::Path;

use academy_finance_contracts::coin::MockFinanceCoinService;
use academy_persistence_contracts::{paypal::MockPaypalRepository, user::MockUserRepository};
use academy_render_contracts::pdf::MockRenderPdfService;
use academy_shared_contracts::fs::MockFsService;
use academy_templates_contracts::MockTemplateService;
use rust_decimal_macros::dec;

use crate::{FinanceServiceConfig, FinanceServiceImpl};

mod get_invoice_pdf;

type Sut = FinanceServiceImpl<
    MockFsService,
    MockTemplateService,
    MockRenderPdfService,
    MockPaypalRepository<()>,
    MockUserRepository<()>,
    MockFinanceCoinService,
>;

impl Default for FinanceServiceConfig {
    fn default() -> Self {
        Self {
            vat_percent: dec!(19),
            invoices_archive: Path::new("/invoices").into(),
        }
    }
}
