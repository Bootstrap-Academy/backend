use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_admin_audit_contracts::{
    AdminAuditFeatureService, AdminAuditListError, AdminAuditListQuery, AdminAuditListResult,
    AdminAuditRequest, target_user_id,
};
use academy_di::Build;
use academy_models::{admin_audit::AdminAuditLogEntry, auth::AccessToken};
use academy_persistence_contracts::{
    Database, Transaction, admin_audit::AdminAuditRepository, session::SessionRepository,
};
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::Context;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct AdminAuditFeatureServiceImpl<Db, Auth, Id, Time, AdminAuditRepo, SessionRepo> {
    db: Db,
    auth: Auth,
    id: Id,
    time: Time,
    admin_audit_repo: AdminAuditRepo,
    session_repo: SessionRepo,
}

impl<Db, Auth, Id, Time, AdminAuditRepo, SessionRepo> AdminAuditFeatureService
    for AdminAuditFeatureServiceImpl<Db, Auth, Id, Time, AdminAuditRepo, SessionRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Id: IdService,
    Time: TimeService,
    AdminAuditRepo: AdminAuditRepository<Db::Transaction>,
    SessionRepo: SessionRepository<Db::Transaction>,
{
    // The request carries the access token it was made with.
    #[trace_instrument(skip(self, request), fields(path = %*request.path))]
    async fn record(&self, request: AdminAuditRequest) -> anyhow::Result<bool> {
        // An expired or invalidated token identifies nobody, so there is
        // nothing to attribute the request to.
        let Ok(auth) = self.auth.authenticate(&request.token).await else {
            return Ok(false);
        };

        let mut txn = self.db.begin_transaction().await?;

        // A session an administrator opened in someone else's account carries
        // that account's token. Its requests are the administrator's and act
        // on that account.
        let impersonated_by = self
            .session_repo
            .get(&mut txn, auth.session_id)
            .await
            .context("Failed to get session from database")?
            .and_then(|session| session.origin.impersonated_by());

        let target_user_id = target_user_id(
            &request.path,
            request.route.as_deref().map(String::as_str),
            auth.user_id,
        );

        // The entry is written whenever the request was made with an
        // administrator's token or in a session an administrator opened,
        // including for requests that were rejected.
        let (admin_user_id, target_user_id) = match impersonated_by {
            Some(admin) => (admin, target_user_id.or(Some(auth.user_id))),
            None if auth.admin => (auth.user_id, target_user_id),
            None => return Ok(false),
        };

        let entry = AdminAuditLogEntry {
            id: self.id.generate(),
            at: self.time.now(),
            admin_user_id,
            target_user_id,
            method: request.method,
            path: request.path,
            status: request.status,
            request_id: request.request_id,
        };

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
