use std::future::Future;

use academy_models::{
    admin_audit::{AdminAuditLogEntry, AdminAuditLogFilter},
    pagination::PaginationSlice,
};
use chrono::{DateTime, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait AdminAuditRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Append an entry to the administrative audit log.
    fn create(
        &self,
        txn: &mut Txn,
        entry: &AdminAuditLogEntry,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return a paginated list of audit log entries, most recent first.
    fn list(
        &self,
        txn: &mut Txn,
        filter: AdminAuditLogFilter,
        pagination: PaginationSlice,
    ) -> impl Future<Output = anyhow::Result<Vec<AdminAuditLogEntry>>> + Send;

    /// Return the total number of audit log entries matching the given filter.
    fn count(
        &self,
        txn: &mut Txn,
        filter: AdminAuditLogFilter,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// Delete all audit log entries recorded before the given timestamp and
    /// return the number of deleted entries.
    fn delete_by_at(
        &self,
        txn: &mut Txn,
        at: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockAdminAuditRepository<Txn> {
    pub fn with_create(mut self, entry: AdminAuditLogEntry) -> Self {
        self.expect_create()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(entry))
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_list(
        mut self,
        filter: AdminAuditLogFilter,
        pagination: PaginationSlice,
        result: Vec<AdminAuditLogEntry>,
    ) -> Self {
        self.expect_list()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(filter),
                mockall::predicate::eq(pagination),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_count(mut self, filter: AdminAuditLogFilter, result: u64) -> Self {
        self.expect_count()
            .once()
            .with(mockall::predicate::always(), mockall::predicate::eq(filter))
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
