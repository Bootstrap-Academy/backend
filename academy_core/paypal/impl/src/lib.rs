use std::{ops::RangeInclusive, path::Path, sync::Arc};

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
use academy_models::{auth::AccessToken, coin::Balance, paypal::PaypalOrderId};
use academy_persistence_contracts::{
    paypal::PaypalRepository, user::UserRepository, Database, Transaction,
};
use academy_render_contracts::pdf::RenderPdfService;
use academy_shared_contracts::{fs::FsService, time::TimeService};
use academy_templates_contracts::{
    InvoiceItem, InvoiceTemplate, PurchaseConfirmationTemplate, TemplateService, LOGO_BASE64,
};
use academy_utils::trace_instrument;
use anyhow::{anyhow, Context};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

pub mod coin_order;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct PaypalFeatureServiceImpl<
    Db,
    Auth,
    Time,
    PaypalApi,
    UserRepo,
    PaypalRepo,
    PaypalCoinOrder,
    Template,
    TemplateEmail,
    RenderPdf,
    Fs,
> {
    db: Db,
    auth: Auth,
    time: Time,
    paypal_api: PaypalApi,
    user_repo: UserRepo,
    paypal_repo: PaypalRepo,
    paypal_coin_order: PaypalCoinOrder,
    template_email: TemplateEmail,
    template: Template,
    render_pdf: RenderPdf,
    fs: Fs,
    config: PaypalFeatureConfig,
}

#[derive(Debug, Clone)]
pub struct PaypalFeatureConfig {
    pub purchase_range: RangeInclusive<u64>,
    pub vat_percent: Decimal,
    pub invoices_archive: Arc<Path>,
}

impl<
        Db,
        Auth,
        Time,
        PaypalApi,
        UserRepo,
        PaypalRepo,
        PaypalCoinOrder,
        Template,
        TemplateEmail,
        RenderPdf,
        Fs,
    > PaypalFeatureService
    for PaypalFeatureServiceImpl<
        Db,
        Auth,
        Time,
        PaypalApi,
        UserRepo,
        PaypalRepo,
        PaypalCoinOrder,
        Template,
        TemplateEmail,
        RenderPdf,
        Fs,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    Time: TimeService,
    PaypalApi: PaypalApiService,
    UserRepo: UserRepository<Db::Transaction>,
    PaypalRepo: PaypalRepository<Db::Transaction>,
    PaypalCoinOrder: PaypalCoinOrderService<Db::Transaction>,
    Template: TemplateService,
    TemplateEmail: TemplateEmailService,
    RenderPdf: RenderPdfService,
    Fs: FsService,
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
        let timestamp = self.time.now();
        let new_balance = self.paypal_coin_order.capture(&mut txn, order).await?;

        if let Some(email) = user_composite.user.email {
            let invoice_number = format!("R{invoice_number:07}");
            let vat_factor = self.config.vat_percent / dec!(100);
            let net_unit = dec!(0.01) / (dec!(1) + vat_factor);
            let net_total = net_unit * Decimal::from(coins);
            let vat_total = net_total * vat_factor;
            let gross_total = dec!(0.01) * Decimal::from(coins);
            debug_assert_eq!(gross_total, (net_total + vat_total).round_dp(4));

            let archive_path = self
                .config
                .invoices_archive
                .join(format!("{invoice_number}.pdf"));

            let invoice_html = self
                .template
                .render(&InvoiceTemplate {
                    logo_base64: &LOGO_BASE64,
                    title: "Rechnung",
                    customer_details: user_composite.invoice_info.into_details(Some(
                        user_composite.profile.display_name.clone().into_inner(),
                    )),
                    timestamp,
                    invoice_number,
                    items: vec![InvoiceItem {
                        description: "MorphCoins".into(),
                        net_unit,
                        count: coins,
                        net_total,
                    }],
                    vat_percent: self.config.vat_percent,
                    net_total,
                    vat_total,
                    gross_total,
                })
                .context("Failed to render invoice template")?;
            let invoice_pdf = self
                .render_pdf
                .render(&invoice_html)
                .await
                .context("Failed to render invoice pdf")?;
            self.fs.store_file(&archive_path, &invoice_pdf).await?;

            self.template_email
                .send_purchase_confirmation_email(
                    email.with_name(user_composite.profile.display_name.into_inner()),
                    &PurchaseConfirmationTemplate {
                        coins,
                        vat_percent: self.config.vat_percent,
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
