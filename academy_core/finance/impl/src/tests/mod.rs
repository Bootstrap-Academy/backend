use std::{path::Path, time::Duration};

use academy_auth_contracts::MockAuthService;
use academy_core_finance_contracts::invoice::MockFinanceInvoiceService;
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, finance::MockFinancialDocumentRepository,
};
use academy_shared_contracts::jwt::MockJwtService;
use rust_decimal_macros::dec;

use crate::{FinanceFeatureConfig, FinanceFeatureServiceImpl};

mod download_credit_note;
mod download_invoice;
mod get_download_token;
mod list_documents;

type Sut = FinanceFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockJwtService,
    MockFinanceInvoiceService<MockTransaction>,
    MockFinancialDocumentRepository<MockTransaction>,
>;

impl Default for FinanceFeatureConfig {
    fn default() -> Self {
        Self {
            vat_percent: dec!(19),
            invoices_archive: Path::new("/invoices").into(),
            credit_notes_archive: Path::new("/credit_notes").into(),
            final_statements_archive: Path::new("/final_statements").into(),
            retention_years: 8,
            download_token_ttl: Duration::from_secs(600),
        }
    }
}
