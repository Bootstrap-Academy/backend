use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_admin_audit_contracts::{
    AdminAuditFeatureService, AdminAuditListError, AdminAuditListQuery, AdminAuditListResult,
    AdminAuditRequest, target_user_id,
};
use academy_di::Build;
use academy_models::{admin_audit::AdminAuditLogEntry, auth::AccessToken};
use academy_persistence_contracts::{Database, Transaction, admin_audit::AdminAuditRepository};
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::Context;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct AdminAuditFeatureServiceImpl<Db, Auth, Id, Time, AdminAuditRepo> {
    db: Db,
    auth: Auth,
    id: Id,
    time: Time,
    admin_audit_repo: AdminAuditRepo,
}

impl<Db, Auth, Id, Time, AdminAuditRepo> AdminAuditFeatureService
    for AdminAuditFeatureServiceImpl<Db, Auth, Id, Time, AdminAuditRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Id: IdService,
    Time: TimeService,
    AdminAuditRepo: AdminAuditRepository<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    async fn record(&self, request: AdminAuditRequest) -> anyhow::Result<bool> {
        // An expired or invalidated token identifies nobody, so there is
        // nothing to attribute the request to.
        let Ok(auth) = self.auth.authenticate(&request.token).await else {
            return Ok(false);
        };

        // The entry is written whenever the request was made with an
        // administrator's token, including for requests that were rejected.
        if !auth.admin {
            return Ok(false);
        }

        let entry = AdminAuditLogEntry {
            id: self.id.generate(),
            at: self.time.now(),
            admin_user_id: auth.user_id,
            target_user_id: target_user_id(
                &request.path,
                request.route.as_deref().map(String::as_str),
                auth.user_id,
            ),
            method: request.method,
            path: request.path,
            status: request.status,
            request_id: request.request_id,
        };

        let mut txn = self.db.begin_transaction().await?;

        self.admin_audit_repo
            .create(&mut txn, &entry)
            .await
            .context("Failed to create audit log entry in database")?;

        txn.commit().await?;

        Ok(true)
    }

    #[trace_instrument(skip(self))]
    async fn list(
        &self,
        token: &AccessToken,
        query: AdminAuditListQuery,
    ) -> Result<AdminAuditListResult, AdminAuditListError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_admin().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let total = self
            .admin_audit_repo
            .count(&mut txn, query.filter)
            .await
            .context("Failed to count audit log entries in database")?;

        let entries = self
            .admin_audit_repo
            .list(&mut txn, query.filter, query.pagination)
            .await
            .context("Failed to get audit log entries from database")?;

        Ok(AdminAuditListResult { total, entries })
    }
}
