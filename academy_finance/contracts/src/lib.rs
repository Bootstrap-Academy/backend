use std::future::Future;

use rust_decimal::Decimal;

pub mod coin;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinanceService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the configured vat percentage.
    fn vat_percent(&self) -> Decimal;

    /// Generate or return the archived invoice for the given invoice number.
    fn get_invoice_pdf(
        &self,
        txn: &mut Txn,
        invoice_number: u64,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockFinanceService<Txn> {
    pub fn with_vat_percent(mut self, result: Decimal) -> Self {
        self.expect_vat_percent()
            .once()
            .with()
            .return_once(move || result);
        self
    }

    pub fn with_get_invoice_pdf(mut self, invoice_number: u64, result: Option<Vec<u8>>) -> Self {
        self.expect_get_invoice_pdf()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(invoice_number),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
