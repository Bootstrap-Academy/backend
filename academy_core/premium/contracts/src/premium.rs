use std::future::Future;

use academy_models::{premium::Premium, user::UserId};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PremiumService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the currently active premium membership of the given user, if
    /// any.
    ///
    /// Handles subscriptions transparently.
    fn get_active(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<Premium>>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockPremiumService<Txn> {
    pub fn with_get_active(mut self, user_id: UserId, result: Option<Premium>) -> Self {
        self.expect_get_active()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
