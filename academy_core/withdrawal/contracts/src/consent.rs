use std::future::Future;

use academy_models::{
    user::UserId,
    withdrawal::{
        WithdrawalConsent, WithdrawalReference, WithdrawalSubject, WithdrawalTextVersion,
    },
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait WithdrawalConsentService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Record the declarations a consumer gave before placing an order.
    fn record(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        subject: WithdrawalSubject,
        reference: Option<WithdrawalReference>,
        text_version: WithdrawalTextVersion,
    ) -> impl Future<Output = anyhow::Result<WithdrawalConsent>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockWithdrawalConsentService<Txn> {
    pub fn with_record(mut self, result: WithdrawalConsent) -> Self {
        self.expect_record()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(result.user_id),
                mockall::predicate::eq(result.subject),
                mockall::predicate::eq(result.reference.clone()),
                mockall::predicate::eq(result.text_version.clone()),
            )
            .return_once(|_, _, _, _, _| Box::pin(std::future::ready(Ok(result))));
        self
    }
}
