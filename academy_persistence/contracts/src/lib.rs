use std::future::Future;

pub mod coin;
pub mod mfa;
pub mod oauth2;
pub mod session;
pub mod user;

#[cfg_attr(feature = "mock", mockall::automock(type Transaction = MockTransaction;))]
pub trait Database: Send + Sync + 'static {
    type Transaction: Transaction;

    /// Start a new transaction which can be used to interact with the database.
    ///
    /// Changes are persisted only after explicitly invoking
    /// [`Transaction::commit()`].
    fn begin_transaction(&self) -> impl Future<Output = anyhow::Result<Self::Transaction>> + Send;

    /// Verify the connection to the database.
    fn ping(&self) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait Transaction: Send + Sync + 'static {
    /// Persist any changes made to the database using this transaction.
    fn commit(self) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Explicitly discard any changes made to the database using this
    /// transaction.
    fn rollback(self) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl MockDatabase {
    pub fn build(expect_commit: bool) -> Self {
        let mut txn = MockTransaction::new();
        if expect_commit {
            txn.expect_commit()
                .once()
                .return_once(|| Box::pin(std::future::ready(Ok(()))));
        }

        let mut db = Self::new();
        db.expect_begin_transaction()
            .once()
            .return_once(|| Box::pin(std::future::ready(Ok(txn))));
        db
    }

    pub fn build_expect_rollback() -> Self {
        let mut txn = MockTransaction::new();
        txn.expect_rollback()
            .once()
            .return_once(|| Box::pin(std::future::ready(Ok(()))));

        let mut db = Self::new();
        db.expect_begin_transaction()
            .once()
            .return_once(|| Box::pin(std::future::ready(Ok(txn))));
        db
    }
}
