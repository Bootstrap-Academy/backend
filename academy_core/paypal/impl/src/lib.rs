use std::ops::RangeInclusive;

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_paypal_contracts::{
    coin_order::PaypalCoinOrderService, PaypalCaptureCoinOrderError, PaypalCreateCoinOrderError,
    PaypalFeatureService,
};
use academy_di::Build;
use academy_extern_contracts::paypal::{
    PaypalApiService, PaypalCaptureOrderError, PaypalCreateOrderError,
};
use academy_models::{auth::AccessToken, coin::Balance, paypal::PaypalOrderId};
use academy_persistence_contracts::{
    paypal::PaypalRepository, user::UserRepository, Database, Transaction,
};
use academy_utils::trace_instrument;
use anyhow::anyhow;

pub mod coin_order;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct PaypalFeatureServiceImpl<Db, Auth, PaypalApi, UserRepo, PaypalRepo, PaypalCoinOrder> {
    db: Db,
    auth: Auth,
    paypal_api: PaypalApi,
    user_repo: UserRepo,
    paypal_repo: PaypalRepo,
    paypal_coin_order: PaypalCoinOrder,
    config: PaypalFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct PaypalFeatureConfig {
    pub purchase_range: RangeInclusive<u64>,
}

impl<Db, Auth, PaypalApi, UserRepo, PaypalRepo, PaypalCoinOrder> PaypalFeatureService
    for PaypalFeatureServiceImpl<Db, Auth, PaypalApi, UserRepo, PaypalRepo, PaypalCoinOrder>
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    PaypalApi: PaypalApiService,
    UserRepo: UserRepository<Db::Transaction>,
    PaypalRepo: PaypalRepository<Db::Transaction>,
    PaypalCoinOrder: PaypalCoinOrderService<Db::Transaction>,
{
    #[trace_instrument(skip(self))]
    fn get_client_id(&self) -> &str {
        self.paypal_api.client_id()
    }

    #[trace_instrument(skip(self))]
    async fn create_coin_order(
        &self,
        token: &AccessToken,
        coins: u64,
    ) -> Result<PaypalOrderId, PaypalCreateCoinOrderError> {
        if !self.config.purchase_range.contains(&coins) {
            return Err(PaypalCreateCoinOrderError::InvalidAmount(
                self.config.purchase_range.clone(),
            ));
        }

        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_email_verified().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let user_composite = self
            .user_repo
            .get_composite(&mut txn, auth.user_id)
            .await?
            .ok_or_else(|| anyhow!("Failed to fetch authenticated user"))?;

        if !user_composite.can_buy_coins() {
            return Err(PaypalCreateCoinOrderError::IncompleteInvoiceInfo);
        }

        let order_id = self
            .paypal_api
            .create_order(coins)
            .await
            .map_err(|err| match err {
                PaypalCreateOrderError::Failed => PaypalCreateCoinOrderError::CreateOrderFailure,
                PaypalCreateOrderError::Other(err) => err.into(),
            })?;

        let order = self
            .paypal_coin_order
            .create(&mut txn, order_id, auth.user_id, coins)
            .await?;

        txn.commit().await?;

        Ok(order.id)
    }

    #[trace_instrument(skip(self))]
    async fn capture_coin_order(
        &self,
        token: &AccessToken,
        order_id: PaypalOrderId,
    ) -> Result<Balance, PaypalCaptureCoinOrderError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_email_verified().map_auth_err()?;

        let mut txn = self.db.begin_transaction().await?;

        let order = self
            .paypal_repo
            .get_coin_order(&mut txn, &order_id)
            .await?
            .filter(|order| order.user_id == auth.user_id && order.captured_at.is_none())
            .ok_or(PaypalCaptureCoinOrderError::NotFound)?;

        let user_composite = self
            .user_repo
            .get_composite(&mut txn, auth.user_id)
            .await?
            .ok_or_else(|| anyhow!("Failed to fetch authenticated user"))?;

        if !user_composite.can_buy_coins() {
            return Err(PaypalCaptureCoinOrderError::IncompleteInvoiceInfo);
        }

        self.paypal_api
            .capture_order(&order.id)
            .await
            .map_err(|err| match err {
                PaypalCaptureOrderError::Failed => PaypalCaptureCoinOrderError::CaptureOrderFailure,
                PaypalCaptureOrderError::Other(err) => err.into(),
            })?;

        let new_balance = self.paypal_coin_order.capture(&mut txn, order).await?;

        txn.commit().await?;

        Ok(new_balance)
    }
}
