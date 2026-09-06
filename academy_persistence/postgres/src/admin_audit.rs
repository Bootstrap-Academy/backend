use academy_di::Build;
use academy_models::{
    admin_audit::{AdminAuditLogEntry, AdminAuditLogFilter},
    pagination::PaginationSlice,
};
use academy_persistence_contracts::admin_audit::AdminAuditRepository;
use academy_utils::trace_instrument;
use chrono::{DateTime, Utc};
use clorinde::{
    client::Params,
    queries::{
        self,
        admin_audit::{CountParams, CreateParams, ListParams},
    },
};
use futures::{StreamExt, TryStreamExt};

use crate::PostgresTransaction;

#[derive(Debug, Clone, Build)]
pub struct PostgresAdminAuditRepository;

impl AdminAuditRepository<PostgresTransaction> for PostgresAdminAuditRepository {
    #[trace_instrument(skip(self, txn))]
    async fn create(
        &self,
        txn: &mut PostgresTransaction,
        entry: &AdminAuditLogEntry,
    ) -> anyhow::Result<()> {
        let params = CreateParams {
            id: *entry.id,
            at: entry.at.into(),
            admin_user_id: *entry.admin_user_id,
            method: &*entry.method,
            path: &*entry.path,
            target_user_id: entry.target_user_id.map(|user_id| *user_id),
            status: entry.status.into(),
            request_id: &*entry.request_id,
        };

        queries::admin_audit::create()
            .params(txn.txn(), &params)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    #[trace_instrument(skip(self, txn))]
    async fn list(
        &self,
        txn: &mut PostgresTransaction,
        filter: AdminAuditLogFilter,
        pagination: PaginationSlice,
    ) -> anyhow::Result<Vec<AdminAuditLogEntry>> {
        let params = ListParams {
            admin_user_id: filter.admin_user_id.map(|user_id| *user_id),
            target_user_id: filter.target_user_id.map(|user_id| *user_id),
            limit: (*pagination.limit).try_into()?,
            offset: pagination.offset.try_into()?,
        };

        queries::admin_audit::list()
            .params(txn.txn(), &params)
            .iter()
            .await?
            .map(|row| row.map_err(Into::into).and_then(decode_entry))
            .try_collect()
            .await
    }

    #[trace_instrument(skip(self, txn))]
    async fn count(
        &self,
        txn: &mut PostgresTransaction,
        filter: AdminAuditLogFilter,
    ) -> anyhow::Result<u64> {
        let params = CountParams {
            admin_user_id: filter.admin_user_id.map(|user_id| *user_id),
            target_user_id: filter.target_user_id.map(|user_id| *user_id),
        };

        queries::admin_audit::count()
            .params(txn.txn(), &params)
            .one()
            .await
            .map_err(Into::into)
            .and_then(|row| row.try_into().map_err(Into::into))
    }

    #[trace_instrument(skip(self, txn))]
    async fn delete_by_at(
        &self,
        txn: &mut PostgresTransaction,
        at: DateTime<Utc>,
    ) -> anyhow::Result<u64> {
        queries::admin_audit::delete_by_at()
            .bind(txn.txn(), &at.into())
            .await
            .map_err(Into::into)
    }
}

fn decode_entry(
    value: queries::admin_audit::AdminAuditLogEntry,
) -> anyhow::Result<AdminAuditLogEntry> {
    Ok(AdminAuditLogEntry {
        id: value.id.into(),
        at: value.at.into(),
        admin_user_id: value.admin_user_id.into(),
        method: value.method.try_into()?,
        path: value.path.try_into()?,
        target_user_id: value.target_user_id.map(Into::into),
        status: value.status.try_into()?,
        request_id: value.request_id.try_into()?,
    })
}
