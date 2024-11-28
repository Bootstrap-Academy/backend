use academy_core_finance_contracts::{
    coin::{CoinPrices, FinanceCoinService},
    invoice::FinanceInvoiceService,
};
use academy_di::Build;
use academy_models::user::UserId;
use academy_persistence_contracts::{paypal::PaypalRepository, user::UserRepository};
use academy_render_contracts::pdf::RenderPdfService;
use academy_shared_contracts::fs::FsService;
use academy_templates_contracts::{InvoiceItem, InvoiceTemplate, TemplateService};
use anyhow::Context;
use tracing::instrument;

use crate::FinanceServiceConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct FinanceInvoiceServiceImpl<Fs, Template, RenderPdf, PaypalRepo, UserRepo, FinanceCoin> {
    fs: Fs,
    template: Template,
    render_pdf: RenderPdf,
    paypal_repo: PaypalRepo,
    user_repo: UserRepo,
    finance_coin: FinanceCoin,
    config: FinanceServiceConfig,
}

impl<Txn, Fs, Template, RenderPdf, PaypalRepo, UserRepo, FinanceCoin> FinanceInvoiceService<Txn>
    for FinanceInvoiceServiceImpl<Fs, Template, RenderPdf, PaypalRepo, UserRepo, FinanceCoin>
where
    Txn: Send + Sync + 'static,
    Fs: FsService,
    Template: TemplateService,
    RenderPdf: RenderPdfService,
    PaypalRepo: PaypalRepository<Txn>,
    UserRepo: UserRepository<Txn>,
    FinanceCoin: FinanceCoinService,
{
    #[instrument(skip(self, txn))]
    async fn get_invoice_pdf(
        &self,
        txn: &mut Txn,
        user_id: Option<UserId>,
        invoice_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let formatted_invoice_number = format!("R{invoice_number:07}");
        let archive_path = self
            .config
            .invoices_archive
            .join(format!("{formatted_invoice_number}.pdf"));

        let cached = self.fs.read_file(&archive_path).await?;
        if user_id.is_none() {
            if let Some(invoice) = cached {
                return Ok(Some(invoice));
            }
        }

        let Some(coin_order) = self
            .paypal_repo
            .get_coin_order_by_invoice_number(txn, invoice_number)
            .await?
            .filter(|order| user_id.is_none_or(|user_id| order.user_id == user_id))
        else {
            return Ok(None);
        };

        if let Some(invoice) = cached {
            return Ok(Some(invoice));
        }

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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use academy_core_finance_contracts::coin::MockFinanceCoinService;
    use academy_demo::user::{BAR, FOO};
    use academy_models::paypal::{PaypalCoinOrder, PaypalOrderId};
    use academy_persistence_contracts::{paypal::MockPaypalRepository, user::MockUserRepository};
    use academy_render_contracts::pdf::MockRenderPdfService;
    use academy_shared_contracts::fs::MockFsService;
    use academy_templates_contracts::MockTemplateService;
    use rust_decimal_macros::dec;

    use super::*;

    type Sut = FinanceInvoiceServiceImpl<
        MockFsService,
        MockTemplateService,
        MockRenderPdfService,
        MockPaypalRepository<()>,
        MockUserRepository<()>,
        MockFinanceCoinService,
    >;

    #[tokio::test]
    async fn ok() {
        // Arrange
        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
        };

        let pdf = vec![1, 2, 3, 4];

        let path = PathBuf::from("/invoices/R0000042.pdf");
        let fs = MockFsService::new()
            .with_read_file(path.clone(), None)
            .with_store_file(path, pdf.clone());

        let paypal_repo = MockPaypalRepository::new()
            .with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let user_repo =
            MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

        let prices = CoinPrices {
            net_unit: 1.into(),
            net_total: 2.into(),
            vat_total: 3.into(),
            gross_total: 4.into(),
        };
        let finance_coin = MockFinanceCoinService::new().with_get_price(1337, prices);

        let template = MockTemplateService::new().with_render(
            InvoiceTemplate {
                title: "Rechnung",
                customer_details: FOO
                    .invoice_info
                    .clone()
                    .into_details(Some(FOO.profile.display_name.clone().into_inner())),
                timestamp: order.created_at,
                invoice_number: "R0000042".into(),
                items: vec![InvoiceItem {
                    description: "MorphCoins".into(),
                    net_unit: prices.net_unit,
                    count: order.coins,
                    net_total: prices.net_total,
                }],
                vat_percent: dec!(19),
                net_total: prices.net_total,
                vat_total: prices.vat_total,
                gross_total: prices.gross_total,
                _static: Default::default(),
            },
            "invoice-template-html".into(),
        );

        let render_pdf =
            MockRenderPdfService::new().with_render("invoice-template-html".into(), pdf.clone());

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            user_repo,
            render_pdf,
            finance_coin,
            template,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, Some(pdf));
    }

    #[tokio::test]
    async fn cached_no_user_id_check() {
        // Arrange
        let pdf = vec![1, 2, 3, 4];

        let fs =
            MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), Some(pdf.clone()));

        let sut = FinanceInvoiceServiceImpl {
            fs,
            ..Sut::default()
        };

        // Act
        let result = sut.get_invoice_pdf(&mut (), None, 42).await.unwrap();

        // Assert
        assert_eq!(result, Some(pdf));
    }

    #[tokio::test]
    async fn cached_with_successful_user_id_check() {
        // Arrange
        let pdf = vec![1, 2, 3, 4];

        let fs =
            MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), Some(pdf.clone()));

        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
        };
        let paypal_repo = MockPaypalRepository::new()
            .with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, Some(pdf));
    }

    #[tokio::test]
    async fn cached_with_failing_user_id_check() {
        // Arrange
        let pdf = vec![1, 2, 3, 4];

        let fs =
            MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), Some(pdf.clone()));

        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: FOO.user.id,
            created_at: BAR.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
        };
        let paypal_repo = MockPaypalRepository::new()
            .with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, Some(pdf));
    }

    #[tokio::test]
    async fn not_found() {
        // Arrange
        let fs = MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), None);

        let paypal_repo =
            MockPaypalRepository::new().with_get_coin_order_by_invoice_number(42, None);

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn different_user() {
        // Arrange
        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: BAR.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
        };

        let fs = MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), None);

        let paypal_repo = MockPaypalRepository::new()
            .with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, None);
    }
}
