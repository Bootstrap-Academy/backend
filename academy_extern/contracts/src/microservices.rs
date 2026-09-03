use std::future::Future;

use academy_models::user::UserId;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait MicroservicesApiService: Send + Sync + 'static {
    /// Notify all enabled microservices that the given user has been deleted.
    ///
    /// Failures are logged, but never reported back to the caller, so that a
    /// microservice which is unavailable cannot fail the deletion of a user.
    fn delete_user(&self, user_id: UserId) -> impl Future<Output = ()> + Send;
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
}
