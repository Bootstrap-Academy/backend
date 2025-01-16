use std::future::Future;

use academy_models::{
    premium::{Premium, PremiumId, PremiumPlan},
    user::UserId,
};
use chrono::{DateTime, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PremiumRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// Return the most recent premium membership for the given user.
    fn get_latest_by_user_id(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<Premium>>> + Send;

    /// Create a new premium membership.
    fn create(
        &self,
        txn: &mut Txn,
        premium: Premium,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Update the `until` field of the given premium membership.
    fn extend(
        &self,
        txn: &mut Txn,
        id: PremiumId,
        until: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Return all ids of users subscribed to premium.
    fn list_subscription_users(
        &self,
        txn: &mut Txn,
    ) -> impl Future<Output = anyhow::Result<Vec<UserId>>> + Send;

    /// Return the premium subscription of the given user.
    fn get_subscription(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<PremiumPlan>>> + Send;

    /// Update or cancel the premium subscription of the given user.
    fn set_subscription(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        plan: Option<PremiumPlan>,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

#[cfg(feature = "mock")]
impl<Txn: Send + Sync + 'static> MockPremiumRepository<Txn> {
    pub fn with_get_latest_by_user_id(mut self, user_id: UserId, result: Option<Premium>) -> Self {
        self.expect_get_latest_by_user_id()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_create(mut self, premium: Premium) -> Self {
        self.expect_create()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(premium),
            )
            .return_once(|_, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_extend(mut self, id: PremiumId, until: DateTime<Utc>) -> Self {
        self.expect_extend()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(id),
                mockall::predicate::eq(until),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }

    pub fn with_get_subscription(mut self, user_id: UserId, result: Option<PremiumPlan>) -> Self {
        self.expect_get_subscription()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
            )
            .return_once(move |_, _| Box::pin(std::future::ready(Ok(result))));
        self
    }

    pub fn with_set_subscription(mut self, user_id: UserId, plan: Option<PremiumPlan>) -> Self {
        self.expect_set_subscription()
            .once()
            .with(
                mockall::predicate::always(),
                mockall::predicate::eq(user_id),
                mockall::predicate::eq(plan),
            )
            .return_once(|_, _, _| Box::pin(std::future::ready(Ok(()))));
        self
    }
}
