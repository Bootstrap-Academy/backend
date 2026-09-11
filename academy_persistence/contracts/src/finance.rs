use std::future::Future;

use academy_models::{
    finance::{FinancialDocument, FinancialDocumentKind, FinancialDocumentNumber},
    pagination::PaginationSlice,
    user::UserId,
};
use chrono::{DateTime, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait FinancialDocumentRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Resolve only existing original-document ownership, including the exact
    /// retained inventory after erasure. Never allocates a customer number.
    fn owned_original_number(
        &self,
        txn: &mut Txn,
        user: UserId,
        kind: FinancialDocumentKind,
        number: u64,
        month: u32,
    ) -> impl Future<Output = anyhow::Result<Option<FinancialDocumentNumber>>> + Send;
    fn original_invoice(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
    ) -> impl Future<Output = anyhow::Result<Option<Vec<u8>>>> + Send;
    fn record_original_invoice(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
        pdf: &[u8],
        provenance: &str,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn flag_missing_invoice(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    fn invoice_reconciliation(
        &self,
        txn: &mut Txn,
    ) -> impl Future<Output = anyhow::Result<String>> + Send;
    /// Read each original's exact namespace, record presence and database bytes.
    fn archive_inventory(&self, txn: &mut Txn) -> impl Future<Output=anyhow::Result<Vec<(FinancialDocumentNumber, FinancialDocumentKind, bool, bool)>>> + Send;
    /// Serialize archive adoption with disposal. False means a retired original.
    fn lock_archive(
        &self, txn: &mut Txn, number: &FinancialDocumentNumber,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
    /// Require READ COMMITTED and commit this admission before touching the file.
    /// False preserves originals and any begun intent, and authorizes no file call.
    /// An uncertain earlier removal must pass current admission again.
    fn begin_archive_disposal(
        &self, txn: &mut Txn, number: &FinancialDocumentNumber, kind: FinancialDocumentKind,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;
    /// File removals authorized by a committed record disposal or reviewed orphan.
    fn pending_archive_disposals(
        &self,
        txn: &mut Txn,
    ) -> impl Future<Output = anyhow::Result<Vec<(FinancialDocumentNumber, FinancialDocumentKind)>>> + Send;
    fn acknowledge_archive_disposal(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
        kind: FinancialDocumentKind,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
    /// An unrecorded archive is reviewed, never erased merely because of its filename.
    fn observe_unrecorded_archive(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
        kind: FinancialDocumentKind,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

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

    /// Preserve compatibility for historical staff assertions. This is not
    /// evidence of payment and must not be used to satisfy a commercial claim.
    fn settle(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
        settled_at: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Replace the customer details of the documents of the given user and
    /// return the number of documents that were changed.
    ///
    /// Final statements keep the details they were issued with, because they
    /// exist to make a refund possible after the account has been deleted.
    fn pseudonymize(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        customer_details: &[String],
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// Return all documents of the given user, oldest first.
    fn list_by_user_id(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Vec<FinancialDocument>>> + Send;

    /// Return a page of documents, newest first.
    ///
    /// `search` matches the document number and the recorded customer details
    /// case-insensitively.
    fn list(
        &self,
        txn: &mut Txn,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
        pagination: PaginationSlice,
    ) -> impl Future<Output = anyhow::Result<Vec<FinancialDocument>>> + Send;

    /// Return the number of documents matching the given filters.
    fn count(
        &self,
        txn: &mut Txn,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// Return the numbers of all recorded documents.
    fn list_numbers(
        &self,
        txn: &mut Txn,
    ) -> impl Future<Output = anyhow::Result<Vec<FinancialDocumentNumber>>> + Send;

    /// Return all documents that were issued before the given timestamp.
    fn list_issued_before(
        &self,
        txn: &mut Txn,
        issued_before: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<Vec<FinancialDocument>>> + Send;

    /// Under READ COMMITTED, delete currently eligible documents issued before
    /// the timestamp. Skip pending or held originals and count actual deletions.
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

    pub fn with_list_by_user_id(mut self, user_id: UserId, result: Vec<FinancialDocument>) -> Self {
        self.expect_list_by_user_id()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_list(
        mut self,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
        pagination: PaginationSlice,
        result: Vec<FinancialDocument>,
    ) -> Self {
        self.expect_list()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(kind),
                mockall::predicate::eq(search),
                mockall::predicate::eq(pagination),
            )
            .return_once(|_, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_count(
        mut self,
        kind: Option<FinancialDocumentKind>,
        search: Option<String>,
        result: u64,
    ) -> Self {
        self.expect_count()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(kind),
                mockall::predicate::eq(search),
            )
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(result))));
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
