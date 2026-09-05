use std::{collections::BTreeMap, future::Future};

use academy_models::user::UserId;

/// The data the microservices store about a user, keyed by the name of the
/// service.
pub type MicroserviceExports = BTreeMap<String, serde_json::Value>;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait MicroservicesApiService: Send + Sync + 'static {
    /// Notify all enabled microservices that the given user has been deleted.
    ///
    /// Failures are logged, but never reported back to the caller, so that a
    /// microservice which is unavailable cannot fail the deletion of a user.
    fn delete_user(&self, user_id: UserId) -> impl Future<Output = ()> + Send;

    /// Return the data all enabled microservices store about the given user.
    ///
    /// Unlike [`MicroservicesApiService::delete_user`], a failure of any single
    /// microservice fails the whole call, so that an incomplete export is never
    /// handed out as if it were complete.
    fn export_user(
        &self,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<MicroserviceExports>> + Send;
}

#[cfg(feature = "mock")]
impl MockMicroservicesApiService {
    pub fn with_delete_user(mut self, user_id: UserId) -> Self {
        self.expect_delete_user()
            .once()
            .with(mockall::predicate::eq(user_id))
            .return_once(|_| Box::pin(std::future::ready(())));
        self
    }

    pub fn with_export_user(mut self, user_id: UserId, result: MicroserviceExports) -> Self {
        self.expect_export_user()
            .once()
            .with(mockall::predicate::eq(user_id))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_export_user_error(mut self, user_id: UserId) -> Self {
        self.expect_export_user()
            .once()
            .with(mockall::predicate::eq(user_id))
            .return_once(|_| {
                Box::pin(std::future::ready(Err(anyhow::anyhow!(
                    "Failed to export user"
                ))))
            });
        self
    }
}
