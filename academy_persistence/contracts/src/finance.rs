use std::future::Future;

use academy_models::{
    finance::{FinancialDocument, FinancialDocumentNumber},
    user::UserId,
};
use chrono::{DateTime, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinancialDocumentRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Record a document that has been issued.
    ///
    /// Values that have already been recorded for this document number are
    /// kept, so that neither a repeated rendering nor a later change of the
    /// user's invoice information can alter an issued document.
    fn record(
        &self,
        txn: &mut Txn,
        document: &FinancialDocument,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the document with the given number.
    fn get(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
    ) -> impl Future<Output = anyhow::Result<Option<FinancialDocument>>> + Send;

    /// Replace the customer details of all documents of the given user and
    /// return the number of documents that were changed.
    fn pseudonymize(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        customer_details: &[String],
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// Return all documents that were issued before the given timestamp.
    fn list_issued_before(
        &self,
        txn: &mut Txn,
        issued_before: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<Vec<FinancialDocument>>> + Send;

    /// Delete all documents that were issued before the given timestamp and
    /// return the number of documents that were deleted.
    fn delete_issued_before(
        &self,
        txn: &mut Txn,
        issued_before: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockFinancialDocumentRepository<Txn> {
    pub fn with_record(mut self, document: FinancialDocument) -> Self {
        self.expect_record()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(document),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_get(
        mut self,
        number: FinancialDocumentNumber,
        result: Option<FinancialDocument>,
    ) -> Self {
        self.expect_get()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(number))
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_pseudonymize(
        mut self,
        user_id: UserId,
        customer_details: Vec<String>,
        result: u64,
    ) -> Self {
        self.expect_pseudonymize()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::function(move |x: &[String]| x == customer_details),
            )
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
