use std::{path::Path, time::Duration};

use academy_auth_contracts::MockAuthService;
use academy_core_finance_contracts::invoice::MockFinanceInvoiceService;
use academy_persistence_contracts::{MockDatabase, MockTransaction};
use academy_shared_contracts::jwt::MockJwtService;
use rust_decimal_macros::dec;

use crate::{FinanceFeatureServiceImpl, FinanceServiceConfig};

mod download_invoice;
mod get_download_token;

type Sut = FinanceFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockJwtService,
    MockFinanceInvoiceService<MockTransaction>,
>;

impl Default for FinanceServiceConfig {
    fn default() -> Self {
        Self {
            vat_percent: dec!(19),
            invoices_archive: Path::new("/invoices").into(),
            download_token_ttl: Duration::from_secs(600),
        }
    }
}
