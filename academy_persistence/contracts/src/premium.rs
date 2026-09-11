use std::future::Future;

use academy_models::{
    premium::{
        Premium, PremiumId, PremiumPlan, PremiumRenewalAgreement, PremiumRenewalId,
        PremiumRenewalStatus,
    },
    user::UserId,
};
use chrono::{DateTime, Utc};

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait PremiumRepository<Txn: Send + Sync + 'static>: Send + Sync + 'static {
    /// JSON containing legacy observations, agreements/documents and delivery/cancellation state.
    fn export_renewal_evidence(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<String>> + Send;

    fn prune_renewal_evidence(
        &self,
        txn: &mut Txn,
        cutoff: DateTime<Utc>,
    ) -> impl Future<Output = anyhow::Result<u64>> + Send;

    /// Serialize with account restrictions; paid periods and agreements remain intact.
    fn renewal_allowed(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<bool>> + Send;

    fn get_renewal(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> impl Future<Output = anyhow::Result<Option<PremiumRenewalStatus>>> + Send;

    fn get_renewal_agreement(
        &self,
        txn: &mut Txn,
        id: PremiumRenewalId,
    ) -> impl Future<Output = anyhow::Result<Option<PremiumRenewalAgreement>>> + Send;

    /// Atomically persist declaration/outbox and activate monthly renewal.
    fn create_renewal(
        &self,
        txn: &mut Txn,
        agreement: &PremiumRenewalAgreement,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    fn pending_renewal_confirmations(
        &self,
        txn: &mut Txn,
    ) -> impl Future<Output = anyhow::Result<Vec<PremiumRenewalAgreement>>> + Send;

    fn record_renewal_delivery(
        &self,
        txn: &mut Txn,
        id: PremiumRenewalId,
        sent: bool,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Lock the user and terminate renewal whose immutable confirmation deadline
    /// was missed, then return their latest paid membership. Call before purchase
    /// can change paid dates; reconciliation never charges or removes paid access.
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
