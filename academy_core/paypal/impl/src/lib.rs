use std::ops::RangeInclusive;

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_paypal_contracts::{
    coin_order::PaypalCoinOrderService, PaypalCaptureCoinOrderError, PaypalCreateCoinOrderError,
    PaypalFeatureService,
};
use academy_di::Build;
use academy_email_contracts::template::TemplateEmailService;
use academy_extern_contracts::paypal::{
    PaypalApiService, PaypalCaptureOrderError, PaypalCreateOrderError,
};
use academy_finance_contracts::{
    coin::{CoinPrices, FinanceCoinService},
    FinanceService,
};
use academy_models::{auth::AccessToken, coin::Balance, paypal::PaypalOrderId};
use academy_persistence_contracts::{
    paypal::PaypalRepository, user::UserRepository, Database, Transaction,
};
use academy_templates_contracts::PurchaseConfirmationTemplate;
use academy_utils::trace_instrument;
use anyhow::{anyhow, Context};

pub mod coin_order;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct PaypalFeatureServiceImpl<
    Db,
    Auth,
    PaypalApi,
    UserRepo,
    PaypalRepo,
    PaypalCoinOrder,
    TemplateEmail,
    Finance,
    FinanceCoin,
> {
    db: Db,
    auth: Auth,
    paypal_api: PaypalApi,
    user_repo: UserRepo,
    paypal_repo: PaypalRepo,
    paypal_coin_order: PaypalCoinOrder,
    template_email: TemplateEmail,
    finance: Finance,
    finance_coin: FinanceCoin,
    config: PaypalFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct PaypalFeatureConfig {
    pub purchase_range: RangeInclusive<u64>,
}

impl<
        Db,
        Auth,
        PaypalApi,
        UserRepo,
        PaypalRepo,
        PaypalCoinOrder,
        TemplateEmail,
        Finance,
        FinanceCoin,
    > PaypalFeatureService
    for PaypalFeatureServiceImpl<
        Db,
        Auth,
        PaypalApi,
        UserRepo,
        PaypalRepo,
        PaypalCoinOrder,
        TemplateEmail,
        Finance,
        FinanceCoin,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    PaypalApi: PaypalApiService,
    UserRepo: UserRepository<Db::Transaction>,
    PaypalRepo: PaypalRepository<Db::Transaction>,
    PaypalCoinOrder: PaypalCoinOrderService<Db::Transaction>,
    TemplateEmail: TemplateEmailService,
    Finance: FinanceService<Db::Transaction>,
    FinanceCoin: FinanceCoinService,
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

        let invoice_number = order.invoice_number;
        let coins = order.coins;
        let new_balance = self.paypal_coin_order.capture(&mut txn, order).await?;

        if let Some(email) = user_composite.user.email {
            let Some(invoice_pdf) = self
                .finance
                .get_invoice_pdf(&mut txn, invoice_number)
                .await?
            else {
                return Err(
                    anyhow!("Failed to get invoice of order that has just been captured.").into(),
                );
            };

            let CoinPrices {
                vat_total,
                gross_total,
                ..
            } = self.finance_coin.get_price(coins);

            self.template_email
                .send_purchase_confirmation_email(
                    email.with_name(user_composite.profile.display_name.into_inner()),
                    &PurchaseConfirmationTemplate {
                        coins,
                        vat_percent: self.finance.vat_percent(),
                        vat_total,
                        gross_total,
                    },
                    invoice_pdf,
                )
                .await
                .context("Failed to send purchse confirmation email")?;
        }

        txn.commit().await?;

        Ok(new_balance)
    }
}
