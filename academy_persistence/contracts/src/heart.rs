use std::future::Future;

use academy_models::{heart::Hearts, user::UserId};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait HeartRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the hearts of the given user.
    fn get(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<Hearts>>> + Send;

    /// Update the hearts of the given user.
    fn set(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        hearts: Hearts,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockHeartRepository<Txn> {
    pub fn with_get(mut self, user_id: UserId, result: Option<Hearts>) -> Self {
        self.expect_get()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_set(mut self, user_id: UserId, hearts: Hearts) -> Self {
        self.expect_set()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(hearts),
            )
            .return_once(move |_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
