use std::future::Future;

use academy_models::{
    heart::{HeartOperation, HeartOperationClaim, HeartOperationReceipt, Hearts},
    user::UserId,
};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait HeartRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Serialize this operation until transaction completion and return its exact receipt.
    fn claim_operation(
        &self,
        txn: &mut Txn,
        operation: &HeartOperation,
    ) -> impl Future<Output = anyhow::Result<HeartOperationClaim>> + Send;

    /// Serialize against heart refills, premium changes, and account deletion.
    fn lock_user(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    /// Save the immutable result in the same transaction as the debit.
    fn complete_operation(
        &self,
        txn: &mut Txn,
        operation: &HeartOperation,
        receipt: HeartOperationReceipt,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Export only the owner's attempt charge receipts, without other account data.
    fn export_operations(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<String>> + Send;

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
