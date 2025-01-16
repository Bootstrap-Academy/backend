use std::collections::HashMap;

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_premium_contracts::{
    plan::PremiumPlanService, premium::PremiumService, purchase::PremiumPurchaseService,
    PremiumFeatureService, PremiumGetStatusError, PremiumPurchaseError,
    PremiumUpdateSubscriptionError,
};
use academy_di::Build;
use academy_models::{
    auth::AccessToken,
    premium::{PremiumPlan, PremiumPlanDetails, PremiumStatus},
    user::UserIdOrSelf,
};
use academy_persistence_contracts::{
    premium::PremiumRepository, user::UserRepository, Database, Transaction,
};
use academy_utils::trace_instrument;

pub mod plan;
pub mod premium;
pub mod purchase;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct PremiumFeatureServiceImpl<
    Db,
    Auth,
    PremiumPlanS,
    PremiumS,
    PremiumPurchase,
    UserRepo,
    PremiumRepo,
> {
    db: Db,
    auth: Auth,
    premium_plan: PremiumPlanS,
    premium: PremiumS,
    premium_purchase: PremiumPurchase,
    user_repo: UserRepo,
    premium_repo: PremiumRepo,
}

#[derive(Debug, Clone, Copy)]
pub struct PremiumFeatureConfig {
    pub monthly_price: u64,
    pub yearly_price: u64,
}

impl<Db, Auth, PremiumPlanS, PremiumS, PremiumPurchase, UserRepo, PremiumRepo> PremiumFeatureService
    for PremiumFeatureServiceImpl<
        Db,
        Auth,
        PremiumPlanS,
        PremiumS,
        PremiumPurchase,
        UserRepo,
        PremiumRepo,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    PremiumPlanS: PremiumPlanService,
    PremiumS: PremiumService<Db::Transaction>,
    PremiumPurchase: PremiumPurchaseService<Db::Transaction>,
    UserRepo: UserRepository<Db::Transaction>,
    PremiumRepo: PremiumRepository<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    fn get_plans(&self) -> HashMap<PremiumPlan, PremiumPlanDetails> {
        [PremiumPlan::Monthly, PremiumPlan::Yearly]
            .into_iter()
            .map(|plan| (plan, self.premium_plan.get_details(plan)))
            .collect()
    }

    #[trace_instrument(skip(self))]
    async fn get_status(
        &self,
        token: &AccessToken,
        user_id: UserIdOrSelf,
    ) -> Result<Option<PremiumStatus>, PremiumGetStatusError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = user_id.unwrap_or(auth.user_id);
        auth.ensure_self_or_admin(user_id).map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if !self.user_repo.exists(&mut txn, user_id).await? {
            return Err(PremiumGetStatusError::NotFound);
        }

        let Some(premium) = self.premium.get_active(&mut txn, user_id).await? else {
            txn.commit().await?;
            return Ok(None);
        };

        let subscription = self
            .premium_repo
            .get_subscription(&mut txn, user_id)
            .await?;

        txn.commit().await?;

        Ok(Some(PremiumStatus {
            since: premium.since,
            until: premium.until,
            subscription,
        }))
    }

    #[trace_instrument(skip(self))]
    async fn purchase(
        &self,
        token: &AccessToken,
        plan: PremiumPlan,
        subscribe: bool,
    ) -> Result<PremiumStatus, PremiumPurchaseError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = auth.user_id;
        auth.ensure_email_verified().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let premium = self
            .premium_purchase
            .purchase(&mut txn, user_id, plan)
            .await
            .map_err(|err| {
                use academy_core_premium_contracts::purchase::PremiumPurchaseError as E;
                match err {
                    E::NotEnoughCoins => PremiumPurchaseError::NotEnoughCoins,
                    E::Other(err) => err.into(),
                }
            })?;

        let subscription = if subscribe {
            self.premium_repo
                .set_subscription(&mut txn, user_id, Some(plan))
                .await?;
            Some(plan)
        } else {
            self.premium_repo
                .get_subscription(&mut txn, user_id)
                .await?
        };

        txn.commit().await?;

        Ok(PremiumStatus {
            since: premium.since,
            until: premium.until,
            subscription,
        })
    }

    #[trace_instrument(skip(self))]
    async fn update_subscription(
        &self,
        token: &AccessToken,
        plan: Option<PremiumPlan>,
    ) -> Result<(), PremiumUpdateSubscriptionError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        let user_id = auth.user_id;
        auth.ensure_email_verified().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        if self.premium.get_active(&mut txn, user_id).await?.is_none() {
            txn.commit().await?;
            return Err(PremiumUpdateSubscriptionError::NoPremium);
        }

        self.premium_repo
            .set_subscription(&mut txn, user_id, plan)
            .await?;

        txn.commit().await?;

        Ok(())
    }
}
