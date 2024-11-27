use std::{path::Path, sync::Arc};

use academy_di::Build;
use academy_finance_contracts::{
    coin::{CoinPrices, FinanceCoinService},
    FinanceService,
};
use academy_persistence_contracts::{paypal::PaypalRepository, user::UserRepository};
use academy_render_contracts::pdf::RenderPdfService;
use academy_shared_contracts::fs::FsService;
use academy_templates_contracts::{InvoiceItem, InvoiceTemplate, TemplateService};
use anyhow::Context;
use rust_decimal::Decimal;

pub mod coin;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct FinanceServiceImpl<Fs, Template, RenderPdf, PaypalRepo, UserRepo, FinanceCoin> {
    fs: Fs,
    template: Template,
    render_pdf: RenderPdf,
    paypal_repo: PaypalRepo,
    user_repo: UserRepo,
    finance_coin: FinanceCoin,
    config: FinanceServiceConfig,
}

#[derive(Debug, Clone)]
pub struct FinanceServiceConfig {
    pub vat_percent: Decimal,
    pub invoices_archive: Arc<Path>,
}

impl<Txn, Fs, Template, RenderPdf, PaypalRepo, UserRepo, FinanceCoin> FinanceService<Txn>
    for FinanceServiceImpl<Fs, Template, RenderPdf, PaypalRepo, UserRepo, FinanceCoin>
where
    Txn: Send + Sync + 'static,
    Fs: FsService,
    Template: TemplateService,
    RenderPdf: RenderPdfService,
    PaypalRepo: PaypalRepository<Txn>,
    UserRepo: UserRepository<Txn>,
    FinanceCoin: FinanceCoinService,
{
    fn vat_percent(&self) -> Decimal {
        self.config.vat_percent
    }

    async fn get_invoice_pdf(
        &self,
        txn: &mut Txn,
        invoice_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let formatted_invoice_number = format!("R{invoice_number:07}");
        let archive_path = self
            .config
            .invoices_archive
            .join(format!("{formatted_invoice_number}.pdf"));

        if let Some(invoice) = self.fs.read_file(&archive_path).await? {
            return Ok(Some(invoice));
        }

        let Some(coin_order) = self
            .paypal_repo
            .get_coin_order_by_invoice_number(txn, invoice_number)
            .await?
        else {
            return Ok(None);
        };

        let coins = coin_order.coins;
        let timestamp = coin_order.created_at;

        let Some(user_composite) = self
            .user_repo
            .get_composite(txn, coin_order.user_id)
            .await?
        else {
            return Ok(None);
        };

        let CoinPrices {
            net_unit,
            net_total,
            vat_total,
            gross_total,
        } = self.finance_coin.get_price(coins);

        let invoice_html = self
            .template
            .render(&InvoiceTemplate {
                title: "Rechnung",
                customer_details: user_composite.invoice_info.into_details(Some(
                    user_composite.profile.display_name.clone().into_inner(),
                )),
                timestamp,
                invoice_number: formatted_invoice_number,
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
                _static: Default::default(),
            })
            .context("Failed to render invoice template")?;

        let invoice_pdf = self
            .render_pdf
            .render(&invoice_html)
            .await
            .context("Failed to render invoice pdf")?;

        self.fs.store_file(&archive_path, &invoice_pdf).await?;

        Ok(Some(invoice_pdf))
    }
}
