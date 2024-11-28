use std::future::Future;

use academy_models::user::UserId;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinanceInvoiceService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Generate or return the archived invoice for the given invoice number.
    fn get_invoice_pdf(
        &self,
        txn: &mut Txn,
        user_id: Option<UserId>,
        invoice_number: u64,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockFinanceInvoiceService<Txn> {
    pub fn with_get_invoice_pdf(
        mut self,

        user_id: Option<UserId>,
        invoice_number: u64,
        result: Option<Vec<u8>>,
    ) -> Self {
        self.expect_get_invoice_pdf()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(invoice_number),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
