use std::future::Future;

use academy_models::{finance::FinancialDocumentNumber, user::UserId};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinanceInvoiceService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Generate or return the archived invoice for the given invoice number.
    fn get_invoice_pdf(
        &self,
        txn: &mut Txn,
        user_id: Option<UserId>,
        invoice_number: u64,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;

    /// Generate or return the archived credit note by month.
    fn get_credit_note(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        year: i32,
        month: u32,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;

    /// Issue the final statement of the given account (AGB Ziffer 6.7).
    ///
    /// The statement records the unused share of the purchased Morphcoins at
    /// the moment the account is deleted, so that it can still be refunded on
    /// request afterwards. It is only issued for accounts that have bought
    /// Morphcoins; for every other account there is nothing to refund and
    /// therefore no reason to keep their name after the deletion.
    ///
    /// Returns the number of the statement, or `None` if none was issued.
    fn create_final_statement(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<FinancialDocumentNumber>>> + Send;
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

    pub fn with_create_final_statement(
        mut self,
        user_id: UserId,
        result: Option<FinancialDocumentNumber>,
    ) -> Self {
        self.expect_create_final_statement()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_get_credit_note(
        mut self,
        user_id: UserId,
        year: i32,
        month: u32,
        result: Option<Vec<u8>>,
    ) -> Self {
        self.expect_get_credit_note()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(year),
                mockall::predicate::eq(month),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
