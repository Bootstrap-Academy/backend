use academy_auth_contracts::{AuthResultExt, AuthService, AuthenticateByRefreshTokenError};
use academy_core_admin_audit_contracts::{
    AdminAuditActor, AdminAuditCapture, AdminAuditCredential, AdminAuditFeatureService,
    AdminAuditListError, AdminAuditListQuery, AdminAuditListResult, AdminAuditRequest,
    target_user_id,
};
use academy_di::Build;
use academy_models::{
    admin_audit::AdminAuditLogEntry,
    auth::{AccessToken, AuthenticateError},
    session::SessionOrigin,
};
use academy_persistence_contracts::{
    Database, Transaction, admin_audit::AdminAuditRepository, session::SessionRepository,
    user::UserRepository,
};
use academy_shared_contracts::{id::IdService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::Context;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct AdminAuditFeatureServiceImpl<Db, Auth, Id, Time, AdminAuditRepo, SessionRepo, UserRepo> {
    db: Db,
    auth: Auth,
    id: Id,
    time: Time,
    admin_audit_repo: AdminAuditRepo,
    session_repo: SessionRepo,
    user_repo: UserRepo,
}

impl<Db, Auth, Id, Time, AdminAuditRepo, SessionRepo, UserRepo> AdminAuditFeatureService
    for AdminAuditFeatureServiceImpl<Db, Auth, Id, Time, AdminAuditRepo, SessionRepo, UserRepo>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Id: IdService,
    Time: TimeService,
    AdminAuditRepo: AdminAuditRepository<Db::Transaction>,
    SessionRepo: SessionRepository<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
{
    #[trace_instrument(skip_all)]
    async fn capture(
        &self,
        credential: AdminAuditCredential<'_>,
    ) -> anyhow::Result<AdminAuditCapture> {
        // The ordinary read authentication checks durable authority without
        // taking a write lock. Run it before borrowing our transaction so a
        // one-connection pool works too. Unrecorded owner requests must retain
        // their handler's own lock/acceptance order.
        let access_auth = if let AdminAuditCredential::Access(token) = credential {
            match self.auth.authenticate(token).await {
                Ok(auth) => Some(auth),
                Err(AuthenticateError::InvalidToken) => {
                    return Ok(AdminAuditCapture::InvalidCredential);
                }
                Err(AuthenticateError::Other(error)) => return Err(error),
            }
        } else {
            None
        };
        let mut txn = self.db.begin_transaction().await?;
        let (session_id, admin) = match credential {
            AdminAuditCredential::Access(token) => {
                let auth = access_auth.expect("Access credential was authenticated");
                let observed_session = self.session_repo.get(&mut txn, auth.session_id).await?;
                if !auth.admin
                    && !observed_session.is_some_and(|session| {
                        matches!(session.origin, SessionOrigin::Impersonation { .. })
                    })
                {
                    txn.rollback().await?;
                    return Ok(AdminAuditCapture::Unrecorded);
                }
                // Recorded actors are checked again under the current account
                // and session locks before their immutable origin is captured.
                let auth = match self.auth.authenticate_in_transaction(&mut txn, token).await {
                    Ok(auth) => auth,
                    Err(AuthenticateError::InvalidToken) => {
                        return Ok(AdminAuditCapture::InvalidCredential);
                    }
                    Err(AuthenticateError::Other(error)) => return Err(error),
                };
                (auth.session_id, auth.admin)
            }
            AdminAuditCredential::Refresh(token) => {
                let id = match self
                    .auth
                    .authenticate_by_refresh_token(&mut txn, token)
                    .await
                {
                    Ok(id) => id,
                    Err(
                        AuthenticateByRefreshTokenError::Invalid
                        | AuthenticateByRefreshTokenError::Expired(_),
                    ) => return Ok(AdminAuditCapture::InvalidCredential),
                    Err(AuthenticateByRefreshTokenError::Other(error)) => return Err(error),
                };
                let session = self
                    .session_repo
                    .get(&mut txn, id)
                    .await?
                    .context("Authenticated refresh session disappeared")?;
                let user = self
                    .user_repo
                    .get_composite(&mut txn, session.user_id)
                    .await?
                    .context("Authenticated refresh account disappeared")?;
                (id, user.user.admin)
            }
        };
        let session = self
            .session_repo
            .get(&mut txn, session_id)
            .await?
            .context("Authenticated audit session disappeared")?;
        let actor = match session.origin {
            SessionOrigin::Impersonation { admin } => {
                AdminAuditCapture::Recorded(AdminAuditActor {
                    user_id: session.user_id,
                    admin_user_id: admin,
                    impersonated: true,
                })
            }
            _ if admin => AdminAuditCapture::Recorded(AdminAuditActor {
                user_id: session.user_id,
                admin_user_id: Some(session.user_id),
                impersonated: false,
            }),
            _ => AdminAuditCapture::Unrecorded,
        };
        // Release the read/authentication locks before the handler starts.
        txn.rollback().await?;
        Ok(actor)
    }

    #[trace_instrument(skip(self, request), fields(path = %*request.path))]
    async fn record(&self, request: AdminAuditRequest) -> anyhow::Result<bool> {
        let target_user_id = target_user_id(
            &request.path,
            request.route.as_deref().map(String::as_str),
            request.actor.user_id,
        )
        .or_else(|| request.actor.impersonated.then_some(request.actor.user_id));
        let admin_user_id = request.actor.admin_user_id;
        let mut txn = self.db.begin_transaction().await?;

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
