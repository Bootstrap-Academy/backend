use std::future::Future;

use academy_models::{
    finance::{FinancialDocumentKind, FinancialDocumentNumber},
    paypal::PaypalPayment,
    user::UserId,
};

/// A final statement whose record has been written and whose pdf has still to
/// be produced.
///
/// The record carries everything a later refund needs, so it is written inside
/// the transaction that deletes the account. The pdf is not: producing it
/// means an http request to the render daemon, which must not be made while a
/// database transaction and the row lock of the account it deletes are held
/// open.
#[derive(Clone, PartialEq, Eq)]
pub struct PendingFinalStatement {
    pub number: FinancialDocumentNumber,
    /// The rendered document, waiting to be turned into a pdf.
    pub html: String,
}

/// The document names the person it was issued for, so only its number is
/// ever printed.
impl std::fmt::Debug for PendingFinalStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("PendingFinalStatement")
            .field("number", &self.number)
            .finish_non_exhaustive()
    }
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinanceInvoiceService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Read the exact archived original. Authorization belongs to the caller;
    /// missing bytes never cause rendering, identity allocation or new evidence.
    fn get_original_pdf(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
        kind: FinancialDocumentKind,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;
    /// Record the immutable invoice before any PDF is written or mail is sent.
    fn record_payment_invoice(
        &self,
        txn: &mut Txn,
        payment: &PaypalPayment,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    /// Render/archive an already committed payment invoice from its original snapshot.
    fn render_payment_invoice(
        &self,
        txn: &mut Txn,
        payment: &PaypalPayment,
    ) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;

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
    /// Returns the statement whose pdf still has to be archived with
    /// [`FinanceInvoiceService::archive_final_statement`], or `None` if no
    /// statement was issued.
    fn create_final_statement(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<PendingFinalStatement>>> + Send;

    /// Render the pdf of a recorded final statement and put it into the
    /// archive.
    ///
    /// Has to be called after the transaction that recorded the statement has
    /// been committed, because it calls the render daemon over http. A failure
    /// is logged with the document number and nothing else and is not
    /// returned: the record is what a refund needs, and
    /// `academy task list-orphan-documents` reports the records whose pdf is
    /// missing.
    fn archive_final_statement(
        &self,
        statement: PendingFinalStatement,
    ) -> impl Future<Output = ()> + Send;
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
        result: Option<PendingFinalStatement>,
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

    pub fn with_archive_final_statement(mut self, statement: PendingFinalStatement) -> Self {
        self.expect_archive_final_statement()
            .once()
            .with(mockall::predicate::eq(statement))
            .return_once(|_| Box::pin(std::future::ready(())));
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
