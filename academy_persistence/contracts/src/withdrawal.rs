use std::future::Future;

use academy_models::{user::UserId, withdrawal::WithdrawalConsent};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait WithdrawalRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Record the declarations a consumer gave before placing an order.
    fn create(
        &self,
        txn: &mut Txn,
        consent: &WithdrawalConsent,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return all consents of the given user, oldest first.
    fn list_by_user_id(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Vec<WithdrawalConsent>>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockWithdrawalRepository<Txn> {
    pub fn with_create(mut self, consent: WithdrawalConsent) -> Self {
        self.expect_create()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(consent),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
