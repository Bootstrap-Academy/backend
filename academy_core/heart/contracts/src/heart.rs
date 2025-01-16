use std::future::Future;

use academy_models::{heart::Hearts, user::UserId};
use thiserror::Error;

/// Get and update user hearts.
///
/// Handles auto refill transparently.
#[cfg_attr(feature = "mock", mockall::automock)]
pub trait HeartService<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the hearts of the given user.
    fn get(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Hearts>> + Send;

    /// Add hearts for the given user.
    ///
    /// Limits the number of hearts to the configured maximum.
    /// Trying to reduce the number of hearts below zero returns an error.
    fn add(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        hearts: i64,
    ) -> impl Future<Output = Result<Hearts, HeartAddError>> + Send;
}

#[derive(Debug, Error)]
pub enum HeartAddError {
    #[error("The user does not have enough hearts")]
    NotEnoughHearts,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockHeartService<Txn> {
    pub fn with_get(mut self, user_id: UserId, result: Hearts) -> Self {
        self.expect_get()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_add(
        mut self,
        user_id: UserId,
        hearts: i64,
        result: Result<Hearts, HeartAddError>,
    ) -> Self {
        self.expect_add()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(hearts),
            )
            .return_once(move |_, _, _| Box::pin(std::future::ready(result)));
        self
    }
}
