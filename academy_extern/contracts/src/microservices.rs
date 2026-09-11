use std::{collections::BTreeMap, future::Future};

use academy_models::user::UserId;

/// The data the microservices store about a user, keyed by the name of the
/// service.
///
/// The value is [`None`] for a service that could not be read; the reason is
/// logged and deliberately not part of this map, because it can contain
/// internal urls.
pub type MicroserviceExports = BTreeMap<String, Option<serde_json::Value>>;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait MicroservicesApiService: Send + Sync + 'static {
    /// Fixed retained-course evidence/delivery operations; no arbitrary service URL.
    fn course_rights(
        &self,
        operation: &str,
        source_subject: UserId,
        body: serde_json::Value,
    ) -> impl Future<Output = anyhow::Result<serde_json::Value>> + Send;
    /// Fixed retained Events evidence and exact existing-contract continuation.
    fn event_rights(
        &self,
        operation: &str,
        source_subject: UserId,
        body: serde_json::Value,
    ) -> impl Future<Output = anyhow::Result<serde_json::Value>> + Send;
    fn cancel_recipient_event(
        &self,
        user: UserId,
        event_id: uuid::Uuid,
    ) -> impl Future<Output = anyhow::Result<serde_json::Value>> + Send;
    /// Fixed Challenges case/delivery operations. Subject is established by the backend.
    fn moderation(
        &self,
        operation: &str,
        recipient: Option<UserId>,
        body: serde_json::Value,
    ) -> impl Future<Output = anyhow::Result<serde_json::Value>> + Send;

    /// Notify all enabled microservices that the given user has been deleted.
    ///
    /// Failures are logged, but never reported back to the caller, so that a
    /// microservice which is unavailable cannot fail the deletion of a user.
    fn delete_user(&self, user_id: UserId) -> impl Future<Output = ()> + Send;

    /// Deliver durable work to a named service. Disabled/unavailable services remain pending.
    fn delete_user_in_service(
        &self,
        service: &str,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return the data all enabled microservices store about the given user.
    ///
    /// Like [`MicroservicesApiService::delete_user`], the failure of a single
    /// microservice does not fail the whole call. The service is included with
    /// a value of [`None`] instead, so that the caller can hand out the rest of
    /// the export while naming the part that is missing.
    fn export_user(&self, user_id: UserId) -> impl Future<Output = MicroserviceExports> + Send;
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
            .return_once(|_| Box::pin(std::future::ready(result)));
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecipientEventError {
    #[error("Exact original cancellation target required; no declaration recorded")]
    ExactTargetRequired(serde_json::Value),
    #[error("Event not available for this recipient")]
    NotFound,
    #[error("Event timing does not permit this cancellation")]
    Forbidden,
    #[error("Cancellation committed with pending settlement work")]
    Pending(serde_json::Value),
}
