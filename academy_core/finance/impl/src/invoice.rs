use academy_core_finance_contracts::{
    coin::{CoinPrices, FinanceCoinService},
    invoice::FinanceInvoiceService,
};
use academy_di::Build;
use academy_extern_contracts::render::RenderApiService;
use academy_models::{
    finance::{
        FinancialDocument, FinancialDocumentKind, FinancialDocumentNumber, retention_cutoff,
    },
    user::UserId,
};
use academy_persistence_contracts::{
    coin::CoinRepository, finance::FinancialDocumentRepository, paypal::PaypalRepository,
    user::UserRepository,
};
use academy_shared_contracts::{fs::FsService, time::TimeService};
use academy_templates_contracts::{InvoiceItem, InvoiceTemplate, TemplateService};
use anyhow::Context;
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use rust_decimal::{Decimal, prelude::ToPrimitive};
use tracing::instrument;

use crate::FinanceFeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct FinanceInvoiceServiceImpl<
    Time,
    Fs,
    Template,
    RenderApi,
    PaypalRepo,
    UserRepo,
    CoinRepo,
    DocumentRepo,
    FinanceCoin,
> {
    time: Time,
    fs: Fs,
    template: Template,
    render_api: RenderApi,
    paypal_repo: PaypalRepo,
    user_repo: UserRepo,
    coin_repo: CoinRepo,
    document_repo: DocumentRepo,
    finance_coin: FinanceCoin,
    config: FinanceFeatureConfig,
}

impl<Txn, Time, Fs, Template, RenderApi, PaypalRepo, UserRepo, CoinRepo, DocumentRepo, FinanceCoin>
    FinanceInvoiceService<Txn>
    for FinanceInvoiceServiceImpl<
        Time,
        Fs,
        Template,
        RenderApi,
        PaypalRepo,
        UserRepo,
        CoinRepo,
        DocumentRepo,
        FinanceCoin,
    >
where
    Txn: Send + Sync + 'static,
    Time: TimeService,
    Fs: FsService,
    Template: TemplateService,
    RenderApi: RenderApiService,
    PaypalRepo: PaypalRepository<Txn>,
    UserRepo: UserRepository<Txn>,
    CoinRepo: CoinRepository<Txn>,
    DocumentRepo: FinancialDocumentRepository<Txn>,
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
        if user_id.is_none()
            && let Some(invoice) = cached
        {
            return Ok(Some(invoice));
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

        // Documents whose retention period has expired have been removed by
        // `academy task prune-documents` and are not created again.
        let cutoff = retention_cutoff(self.time.now(), self.config.retention_years)
            .context("Failed to determine the document retention cutoff")?;
        if timestamp < cutoff {
            return Ok(None);
        }

        let number = FinancialDocumentNumber::try_new(formatted_invoice_number.clone())?;

        let customer_details = match self
            .document_repo
            .get(txn, &number)
            .await?
            .and_then(|document| document.customer_details)
        {
            // An invoice keeps the address block it was issued with.
            Some(customer_details) => customer_details,
            None => {
                let Some(user_composite) = self
                    .user_repo
                    .get_composite(txn, coin_order.user_id)
                    .await?
                else {
                    return Ok(None);
                };

                user_composite.invoice_info.into_details(
                    Some(user_composite.profile.display_name.clone().into_inner()),
                    user_composite.user.email.as_ref().map(ToString::to_string),
                )
            }
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
                customer_details: customer_details.clone(),
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
            })
            .context("Failed to render invoice template")?;

        let invoice_pdf = self
            .render_api
            .render_html_to_pdf(invoice_html)
            .await
            .context("Failed to render invoice pdf")?;

        self.fs.store_file(&archive_path, &invoice_pdf).await?;

        self.document_repo
            .record(
                txn,
                &FinancialDocument {
                    number,
                    kind: FinancialDocumentKind::Invoice,
                    user_id: Some(coin_order.user_id),
                    issued_at: timestamp,
                    customer_details: Some(customer_details),
                    coins: Some(coins),
                    net_total_cents: to_cents(net_total),
                    vat_total_cents: to_cents(vat_total),
                    gross_total_cents: to_cents(gross_total),
                },
            )
            .await
            .context("Failed to record the invoice")?;

        Ok(Some(invoice_pdf))
    }

    #[instrument(skip(self, txn))]
    async fn get_credit_note(
        &self,
        txn: &mut Txn,
        user_id: UserId,
        year: i32,
        month: u32,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(start_of_month) = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single() else {
            return Ok(None);
        };
        let Some(date) = first_day_of_next_month(year, month) else {
            return Ok(None);
        };
        let timestamp = date
            .and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())
            .and_utc();

        let now = self.time.now();
        if now < timestamp {
            return Ok(None);
        }

        let user_number = self.user_repo.get_number(txn, user_id).await?;

        let credit_note_number = format!("G{year:04}{month:02}-{user_number}");
        let archive_path = self
            .config
            .credit_notes_archive
            .join(format!("{credit_note_number}.pdf"));

        if let Some(credit_note) = self.fs.read_file(&archive_path).await? {
            return Ok(Some(credit_note));
        }

        // Documents whose retention period has expired have been removed by
        // `academy task prune-documents` and are not created again.
        let cutoff = retention_cutoff(now, self.config.retention_years)
            .context("Failed to determine the document retention cutoff")?;
        if timestamp < cutoff {
            return Ok(None);
        }

        let number = FinancialDocumentNumber::try_new(credit_note_number.clone())?;

        let customer_details = match self
            .document_repo
            .get(txn, &number)
            .await?
            .and_then(|document| document.customer_details)
        {
            // A credit note keeps the address block it was issued with.
            Some(customer_details) => customer_details,
            None => {
                let Some(user_composite) = self.user_repo.get_composite(txn, user_id).await? else {
                    return Ok(None);
                };

                user_composite.invoice_info.into_details(
                    Some(user_composite.profile.display_name.clone().into_inner()),
                    user_composite.user.email.as_ref().map(ToString::to_string),
                )
            }
        };

        let transactions = self
            .coin_repo
            .get_transactions(txn, user_id, start_of_month..timestamp)
            .await?;

        let items = transactions
            .into_iter()
            .filter(|t| t.include_in_credit_note && t.coins > 0)
            .map(|t| {
                let coins = t.coins as u64;
                let prices = self.finance_coin.get_price(coins);
                InvoiceItem {
                    description: t.description.map(|x| x.into_inner()).unwrap_or_default(),
                    net_unit: prices.net_unit,
                    count: coins,
                    net_total: prices.net_total,
                }
            })
            .collect::<Vec<InvoiceItem>>();

        let coins_total = items.iter().map(|item| item.count).sum();
        let price_total = self.finance_coin.get_price(coins_total);

        let credit_note_html = self
            .template
            .render(&InvoiceTemplate {
                title: "Gutschrift",
                customer_details: customer_details.clone(),
                timestamp,
                invoice_number: credit_note_number,
                items,
                vat_percent: self.config.vat_percent,
                net_total: price_total.net_total,
                vat_total: price_total.vat_total,
                gross_total: price_total.gross_total,
            })
            .context("Failed to render credit note template")?;

        let credit_note_pdf = self
            .render_api
            .render_html_to_pdf(credit_note_html)
            .await
            .context("Failed to render credit note pdf")?;

        self.fs.store_file(&archive_path, &credit_note_pdf).await?;

        self.document_repo
            .record(
                txn,
                &FinancialDocument {
                    number,
                    kind: FinancialDocumentKind::CreditNote,
                    user_id: Some(user_id),
                    issued_at: timestamp,
                    customer_details: Some(customer_details),
                    coins: Some(coins_total),
                    net_total_cents: to_cents(price_total.net_total),
                    vat_total_cents: to_cents(price_total.vat_total),
                    gross_total_cents: to_cents(price_total.gross_total),
                },
            )
            .await
            .context("Failed to record the credit note")?;

        Ok(Some(credit_note_pdf))
    }
}

/// Convert a euro amount into cents, rounded exactly as it is printed on the
/// document.
fn to_cents(amount: Decimal) -> Option<i64> {
    (amount.round_dp(2) * Decimal::ONE_HUNDRED).to_i64()
}

fn first_day_of_next_month(year: i32, month: u32) -> Option<NaiveDate> {
    debug_assert!((1..=12).contains(&month));
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use academy_core_finance_contracts::coin::MockFinanceCoinService;
    use academy_demo::{
        UUID1,
        user::{BAR, FOO},
    };
    use academy_extern_contracts::render::MockRenderApiService;
    use academy_models::{
        coin::Transaction,
        finance::RETENTION_MARKER,
        paypal::{PaypalCoinOrder, PaypalOrderId},
    };
    use academy_persistence_contracts::{
        coin::MockCoinRepository, finance::MockFinancialDocumentRepository,
        paypal::MockPaypalRepository, user::MockUserRepository,
    };
    use academy_shared_contracts::{fs::MockFsService, time::MockTimeService};
    use academy_templates_contracts::MockTemplateService;
    use chrono::DateTime;
    use rust_decimal_macros::dec;

    use super::*;

    type Sut = FinanceInvoiceServiceImpl<
        MockTimeService,
        MockFsService,
        MockTemplateService,
        MockRenderApiService,
        MockPaypalRepository<()>,
        MockUserRepository<()>,
        MockCoinRepository<()>,
        MockFinancialDocumentRepository<()>,
        MockFinanceCoinService,
    >;

    /// A moment at which a document issued in 2024 still has to be kept.
    fn within_retention() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2032, 12, 31, 23, 59, 59).unwrap()
    }

    /// The first moment at which a document issued in 2024 may be deleted.
    fn after_retention() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2033, 1, 1, 0, 0, 0).unwrap()
    }

    #[tokio::test]
    async fn get_invoice_ok() {
        // Arrange
        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        };

        let pdf = vec![1, 2, 3, 4];

        let path = PathBuf::from("/invoices/R0000042.pdf");
        let fs = MockFsService::new()
            .with_read_file(path.clone(), None)
            .with_store_file(path, pdf.clone());

        let time = MockTimeService::new().with_now(within_retention());

        let paypal_repo = MockPaypalRepository::new()
            .with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let user_repo =
            MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

        let customer_details = FOO.invoice_info.clone().into_details(
            Some(FOO.profile.display_name.clone().into_inner()),
            FOO.user.email.as_ref().map(ToString::to_string),
        );

        let document_repo = MockFinancialDocumentRepository::new()
            .with_get("R0000042".try_into().unwrap(), None)
            .with_record(FinancialDocument {
                number: "R0000042".try_into().unwrap(),
                kind: FinancialDocumentKind::Invoice,
                user_id: Some(FOO.user.id),
                issued_at: order.created_at,
                customer_details: Some(customer_details.clone()),
                coins: Some(1337),
                net_total_cents: Some(200),
                vat_total_cents: Some(300),
                gross_total_cents: Some(400),
            });

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
                customer_details,
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
            },
            "invoice-template-html".into(),
        );

        let render_api = MockRenderApiService::new()
            .with_render_html_to_pdf("invoice-template-html".into(), pdf.clone());

        let sut = FinanceInvoiceServiceImpl {
            time,
            fs,
            paypal_repo,
            user_repo,
            document_repo,
            render_api,
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

    /// An invoice keeps the address block it was issued with, so a pseudonymized
    /// record renders the retention marker instead of the user's details.
    #[tokio::test]
    async fn get_invoice_uses_the_recorded_customer_details() {
        // Arrange
        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        };

        let pdf = vec![1, 2, 3, 4];

        let path = PathBuf::from("/invoices/R0000042.pdf");
        let fs = MockFsService::new()
            .with_read_file(path.clone(), None)
            .with_store_file(path, pdf.clone());

        let time = MockTimeService::new().with_now(within_retention());

        let paypal_repo = MockPaypalRepository::new()
            .with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let customer_details = vec![RETENTION_MARKER.to_owned()];

        let recorded = FinancialDocument {
            number: "R0000042".try_into().unwrap(),
            kind: FinancialDocumentKind::Invoice,
            user_id: None,
            issued_at: order.created_at,
            customer_details: Some(customer_details.clone()),
            coins: Some(1337),
            net_total_cents: Some(200),
            vat_total_cents: Some(300),
            gross_total_cents: Some(400),
        };

        let document_repo = MockFinancialDocumentRepository::new()
            .with_get("R0000042".try_into().unwrap(), Some(recorded.clone()))
            .with_record(FinancialDocument {
                user_id: Some(FOO.user.id),
                ..recorded
            });

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
                customer_details,
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
            },
            "invoice-template-html".into(),
        );

        let render_api = MockRenderApiService::new()
            .with_render_html_to_pdf("invoice-template-html".into(), pdf.clone());

        // The user repository is never asked, so a deleted account is not needed
        // to render the document.
        let sut = FinanceInvoiceServiceImpl {
            time,
            fs,
            paypal_repo,
            document_repo,
            render_api,
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

    /// Once the retention period has expired the archived pdf has been deleted
    /// and the document must not be created again.
    #[tokio::test]
    async fn get_invoice_retention_expired() {
        // Arrange
        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        };

        let fs = MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), None);

        let time = MockTimeService::new().with_now(after_retention());

        let paypal_repo = MockPaypalRepository::new()
            .with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let sut = FinanceInvoiceServiceImpl {
            time,
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
    async fn get_invoice_cached_no_user_id_check() {
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
    async fn get_invoice_cached_with_successful_user_id_check() {
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
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
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
    async fn get_invoice_cached_with_failing_user_id_check() {
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
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
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
    async fn get_invoice_not_found() {
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
    async fn get_invoice_different_user() {
        // Arrange
        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: BAR.user.id,
            created_at: FOO.user.created_at,
            captured_at: None,
            coins: 1337,
            invoice_number: 42,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
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

    #[tokio::test]
    async fn get_credit_note_ok() {
        // Arrange
        let now = Utc.with_ymd_and_hms(2024, 3, 14, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new()
            .with_get_number(FOO.user.id, 7)
            .with_get_composite(FOO.user.id, Some(FOO.clone()));

        let pdf = vec![1, 2, 3, 4];

        let fs = MockFsService::new()
            .with_read_file("/credit_notes/G202402-7.pdf".into(), None)
            .with_store_file("/credit_notes/G202402-7.pdf".into(), pdf.clone());

        let transaction = Transaction {
            id: UUID1.into(),
            user_id: FOO.user.id,
            coins: 1337,
            description: Some("hello world".try_into().unwrap()),
            created_at: Utc.with_ymd_and_hms(2024, 2, 7, 13, 37, 42).unwrap(),
            include_in_credit_note: true,
        };

        let timestamp = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let coin_repo = MockCoinRepository::new().with_get_transactions(
            FOO.user.id,
            Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap()..timestamp,
            vec![transaction.clone()],
        );

        let prices = CoinPrices {
            net_unit: 1.into(),
            net_total: 2.into(),
            vat_total: 3.into(),
            gross_total: 4.into(),
        };
        let finance_coin = MockFinanceCoinService::new()
            .with_get_price(1337, prices)
            .with_get_price(1337, prices);

        let customer_details = FOO.invoice_info.clone().into_details(
            Some(FOO.profile.display_name.clone().into_inner()),
            FOO.user.email.as_ref().map(ToString::to_string),
        );

        let document_repo = MockFinancialDocumentRepository::new()
            .with_get("G202402-7".try_into().unwrap(), None)
            .with_record(FinancialDocument {
                number: "G202402-7".try_into().unwrap(),
                kind: FinancialDocumentKind::CreditNote,
                user_id: Some(FOO.user.id),
                issued_at: timestamp,
                customer_details: Some(customer_details.clone()),
                coins: Some(1337),
                net_total_cents: Some(200),
                vat_total_cents: Some(300),
                gross_total_cents: Some(400),
            });

        let template = MockTemplateService::new().with_render(
            InvoiceTemplate {
                title: "Gutschrift",
                customer_details,
                timestamp,
                invoice_number: "G202402-7".into(),
                items: vec![InvoiceItem {
                    description: "hello world".into(),
                    net_unit: prices.net_unit,
                    count: 1337,
                    net_total: prices.net_total,
                }],
                vat_percent: dec!(19),
                net_total: prices.net_total,
                vat_total: prices.vat_total,
                gross_total: prices.gross_total,
            },
            "credit-note-template-html".into(),
        );

        let render_api = MockRenderApiService::new()
            .with_render_html_to_pdf("credit-note-template-html".into(), pdf.clone());

        let sut = FinanceInvoiceServiceImpl {
            time,
            user_repo,
            fs,
            coin_repo,
            document_repo,
            finance_coin,
            template,
            render_api,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_credit_note(&mut (), FOO.user.id, 2024, 2)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, Some(pdf));
    }

    /// Once the retention period has expired the archived pdf has been deleted
    /// and the document must not be created again.
    #[tokio::test]
    async fn get_credit_note_retention_expired() {
        // Arrange
        let time = MockTimeService::new().with_now(after_retention());

        let user_repo = MockUserRepository::new().with_get_number(FOO.user.id, 7);

        let fs = MockFsService::new().with_read_file("/credit_notes/G202402-7.pdf".into(), None);

        let sut = FinanceInvoiceServiceImpl {
            time,
            user_repo,
            fs,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_credit_note(&mut (), FOO.user.id, 2024, 2)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn get_credit_note_not_available_yet() {
        // Arrange
        let now = Utc.with_ymd_and_hms(2024, 3, 14, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let sut = FinanceInvoiceServiceImpl {
            time,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_credit_note(&mut (), FOO.user.id, 2024, 3)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn get_credit_note_cached() {
        // Arrange
        let now = Utc.with_ymd_and_hms(2024, 3, 14, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new().with_get_number(FOO.user.id, 7);

        let pdf = vec![1, 2, 3, 4];

        let fs = MockFsService::new()
            .with_read_file("/credit_notes/G202402-7.pdf".into(), Some(pdf.clone()));

        let sut = FinanceInvoiceServiceImpl {
            time,
            user_repo,
            fs,
            ..Sut::default()
        };

        // Act
        let result = sut
            .get_credit_note(&mut (), FOO.user.id, 2024, 2)
            .await
            .unwrap();

        // Assert
        assert_eq!(result, Some(pdf));
    }
}
