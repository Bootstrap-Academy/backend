use academy_core_finance_contracts::{
    coin::{CoinPrices, FinanceCoinService},
    invoice::FinanceInvoiceService,
};
use academy_di::Build;
use academy_extern_contracts::render::RenderApiService;
use academy_models::{
    finance::{
        FinancialDocument, FinancialDocumentKind, FinancialDocumentNumber, final_statement_number,
        retention_cutoff, unused_purchased_coins,
    },
    user::UserId,
};
use academy_persistence_contracts::{
    coin::CoinRepository, finance::FinancialDocumentRepository, paypal::PaypalRepository,
    user::UserRepository,
};
use academy_shared_contracts::{fs::FsService, time::TimeService};
use academy_templates_contracts::{
    FinalStatementTemplate, InvoiceItem, InvoiceTemplate, TemplateService, format::AMOUNT_DECIMALS,
};
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
        let number = FinancialDocumentNumber::try_new(formatted_invoice_number.clone())?;
        let recorded = self.document_repo.get(txn, &number).await?;

        // An invoice is issued for the payment, so it is dated with the time
        // the order was captured and not with the time it was created: an
        // order created on 31 December and paid on 2 January belongs to the
        // new year's vat period. A document that has already been issued keeps
        // the date it was recorded with, so neither the printed date nor the
        // retention clock of an existing document ever moves.
        let timestamp = match &recorded {
            Some(recorded) => recorded.issued_at,
            None => coin_order.captured_at.unwrap_or(coin_order.created_at),
        };

        // Documents whose retention period has expired have been removed by
        // `academy task prune-documents` and are not created again.
        let cutoff = retention_cutoff(self.time.now(), self.config.retention_years)
            .context("Failed to determine the document retention cutoff")?;
        if timestamp < cutoff {
            return Ok(None);
        }

        let customer_details = match recorded.and_then(|document| document.customer_details) {
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
            gross_total,
            ..
        } = self.finance_coin.get_price(coins);

        let items = vec![InvoiceItem {
            description: "MorphCoins".into(),
            net_unit,
            count: coins,
            net_total,
        }];
        let PrintedTotals {
            net_total,
            vat_total,
        } = PrintedTotals::of(&items, gross_total);

        let invoice_html = self
            .template
            .render(&InvoiceTemplate {
                title: "Rechnung",
                customer_details: customer_details.clone(),
                timestamp,
                invoice_number: formatted_invoice_number,
                items,
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

        // A month without a single credited transaction has nothing to
        // certify, so no document is issued for it.
        if items.is_empty() {
            return Ok(None);
        }

        let coins_total = items.iter().map(|item| item.count).sum();
        let gross_total = self.finance_coin.get_price(coins_total).gross_total;
        let PrintedTotals {
            net_total,
            vat_total,
        } = PrintedTotals::of(&items, gross_total);

        let credit_note_html = self
            .template
            .render(&InvoiceTemplate {
                title: "Gutschrift",
                customer_details: customer_details.clone(),
                timestamp,
                invoice_number: credit_note_number,
                items,
                vat_percent: self.config.vat_percent,
                net_total,
                vat_total,
                gross_total,
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
                    net_total_cents: to_cents(net_total),
                    vat_total_cents: to_cents(vat_total),
                    gross_total_cents: to_cents(gross_total),
                },
            )
            .await
            .context("Failed to record the credit note")?;

        Ok(Some(credit_note_pdf))
    }

    #[instrument(skip(self, txn))]
    async fn create_final_statement(
        &self,
        txn: &mut Txn,
        user_id: UserId,
    ) -> anyhow::Result<Option<FinancialDocumentNumber>> {
        let Some(user_composite) = self.user_repo.get_composite(txn, user_id).await? else {
            return Ok(None);
        };

        // Only the captured orders were paid and invoiced.
        let purchased_coins = self
            .paypal_repo
            .list_coin_orders_by_user_id(txn, user_id)
            .await?
            .into_iter()
            .filter(|order| order.captured_at.is_some())
            .map(|order| order.coins)
            .sum::<u64>();

        // An account that never bought Morphcoins has nothing that could be
        // refunded later, so no statement is issued and nothing about it is
        // kept beyond the deletion.
        if purchased_coins == 0 {
            return Ok(None);
        }

        let balance_coins = self.coin_repo.get_balance(txn, user_id).await?.coins;
        let unused_coins = unused_purchased_coins(balance_coins, purchased_coins);

        // The same applies to an account that spent everything it bought. The
        // statement keeps the name and the email address only so that the
        // amount it records can still be refunded; with nothing left to refund
        // there is no reason to keep them.
        if unused_coins == 0 {
            return Ok(None);
        }

        let user_number = self.user_repo.get_number(txn, user_id).await?;
        let number = FinancialDocumentNumber::try_new(final_statement_number(user_number))?;
        let archive_path = self
            .config
            .final_statements_archive
            .join(format!("{}.pdf", *number));

        let timestamp = self.time.now();

        let customer_details = user_composite.invoice_info.into_details(
            Some(user_composite.profile.display_name.clone().into_inner()),
            user_composite.user.email.as_ref().map(ToString::to_string),
        );

        let refund_amount = self.finance_coin.get_price(unused_coins).gross_total;

        // The record is written before the pdf, because it carries everything
        // a later refund needs. A render daemon that is unavailable must not
        // stop an account from being deleted.
        self.document_repo
            .record(
                txn,
                &FinancialDocument {
                    number: number.clone(),
                    kind: FinancialDocumentKind::FinalStatement,
                    user_id: Some(user_id),
                    issued_at: timestamp,
                    customer_details: Some(customer_details.clone()),
                    coins: Some(unused_coins),
                    // A final statement is not an invoice and shows no vat;
                    // the gross total is the amount that can still be
                    // refunded.
                    net_total_cents: None,
                    vat_total_cents: None,
                    gross_total_cents: to_cents(refund_amount),
                },
            )
            .await
            .context("Failed to record the final statement")?;

        let statement_html = self
            .template
            .render(&FinalStatementTemplate {
                title: "Schlussabrechnung",
                customer_details,
                timestamp,
                statement_number: number.clone().into_inner(),
                purchased_coins,
                balance_coins,
                unused_coins,
                coins_per_euro: self.finance_coin.coins_per_euro(),
                refund_amount,
            })
            .context("Failed to render final statement template")?;

        match self.render_api.render_html_to_pdf(statement_html).await {
            Ok(statement_pdf) => self.fs.store_file(&archive_path, &statement_pdf).await?,
            // `academy task list-orphan-documents` reports the records whose
            // pdf is missing, so the file can be produced later.
            Err(err) => tracing::warn!(
                "Failed to render the pdf of final statement {}: {err:#}",
                *number
            ),
        }

        Ok(Some(number))
    }
}

/// The net and vat totals of a document as they are printed on it.
///
/// Every amount on a document is printed with two decimal places, so the
/// totals have to be derived from the rounded values and not from the exact
/// ones. Otherwise the line items do not add up to the net total, and the net
/// total plus the vat do not add up to the gross total.
///
/// The gross total is the amount that was actually paid and is therefore the
/// fixed point: the net total is the sum of the line totals as they are
/// printed, and the vat is what is left of the gross total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PrintedTotals {
    net_total: Decimal,
    vat_total: Decimal,
}

impl PrintedTotals {
    fn of(items: &[InvoiceItem], gross_total: Decimal) -> Self {
        let net_total = items
            .iter()
            .map(|item| item.net_total.round_dp(AMOUNT_DECIMALS))
            .sum::<Decimal>();

        Self {
            net_total,
            vat_total: gross_total.round_dp(AMOUNT_DECIMALS) - net_total,
        }
    }
}

/// Convert a euro amount into cents, rounded exactly as it is printed on the
/// document.
fn to_cents(amount: Decimal) -> Option<i64> {
    (amount.round_dp(AMOUNT_DECIMALS) * Decimal::ONE_HUNDRED).to_i64()
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
        coin::{Balance, Transaction},
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
    ///
    /// The calendar year is taken in `Europe/Berlin`, where 23:59:59 UTC on
    /// 31 December is already the next year.
    fn within_retention() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2032, 12, 31, 12, 0, 0).unwrap()
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
            // The invoice is issued for the payment, so it is dated with the
            // capture time and not with the time the order was created.
            captured_at: Some(FOO.user.created_at + chrono::Duration::days(3)),
            coins: 1337,
            invoice_number: 42,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        };
        let captured_at = order.captured_at.unwrap();

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
                issued_at: captured_at,
                customer_details: Some(customer_details.clone()),
                coins: Some(1337),
                net_total_cents: Some(200),
                // The document shows the gross total minus the net total as vat, so
                // that the printed amounts add up (`PrintedTotals`).
                vat_total_cents: Some(200),
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
                timestamp: captured_at,
                invoice_number: "R0000042".into(),
                items: vec![InvoiceItem {
                    description: "MorphCoins".into(),
                    net_unit: prices.net_unit,
                    count: order.coins,
                    net_total: prices.net_total,
                }],
                vat_percent: dec!(19),
                net_total: prices.net_total,
                vat_total: prices.gross_total - prices.net_total,
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

    /// An invoice keeps the address block and the date it was issued with, so
    /// a pseudonymized record renders the retention marker instead of the
    /// user's details, and a document that has already been issued is never
    /// re-dated.
    #[tokio::test]
    async fn get_invoice_uses_the_recorded_customer_details() {
        // Arrange
        let order = PaypalCoinOrder {
            id: PaypalOrderId::try_new("asdf1234").unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            // Later than the date on the record, which is what the document
            // has to keep.
            captured_at: Some(FOO.user.created_at + chrono::Duration::days(3)),
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
            // The document shows the gross total minus the net total as vat, so
            // that the printed amounts add up (`PrintedTotals`).
            vat_total_cents: Some(200),
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
                vat_total: prices.gross_total - prices.net_total,
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

        let document_repo =
            MockFinancialDocumentRepository::new().with_get("R0000042".try_into().unwrap(), None);

        let sut = FinanceInvoiceServiceImpl {
            time,
            fs,
            paypal_repo,
            document_repo,
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
                // The document shows the gross total minus the net total as vat, so
                // that the printed amounts add up (`PrintedTotals`).
                vat_total_cents: Some(200),
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
                vat_total: prices.gross_total - prices.net_total,
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

    /// A month in which nothing was credited has nothing to certify, so no
    /// numbered document is issued and nothing is rendered or recorded.
    #[tokio::test]
    async fn get_credit_note_without_transactions() {
        // Arrange
        let now = Utc.with_ymd_and_hms(2024, 3, 14, 0, 0, 0).unwrap();

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new()
            .with_get_number(FOO.user.id, 7)
            .with_get_composite(FOO.user.id, Some(FOO.clone()));

        let fs = MockFsService::new().with_read_file("/credit_notes/G202402-7.pdf".into(), None);

        let coin_repo = MockCoinRepository::new().with_get_transactions(
            FOO.user.id,
            Utc.with_ymd_and_hms(2024, 2, 1, 0, 0, 0).unwrap()
                ..Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap(),
            Vec::new(),
        );

        let document_repo =
            MockFinancialDocumentRepository::new().with_get("G202402-7".try_into().unwrap(), None);

        let sut = FinanceInvoiceServiceImpl {
            time,
            user_repo,
            fs,
            coin_repo,
            document_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.get_credit_note(&mut (), FOO.user.id, 2024, 2).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }

    /// One Morphcoin line of `coins` coins, priced the way
    /// `FinanceCoinService` prices them at the shipped vat rate of 19 %.
    fn line(coins: u64) -> InvoiceItem {
        let net_unit = Decimal::ONE / dec!(100) / dec!(1.19);
        InvoiceItem {
            description: "MorphCoins".into(),
            net_unit,
            count: coins,
            net_total: net_unit * Decimal::from(coins),
        }
    }

    /// The line totals of a document add up to its net total and the net total
    /// plus the vat to its gross total, with the two decimal places all three
    /// are printed with.
    #[test]
    fn printed_totals_add_up() {
        // Two lines of ten coins each: 0,08 € + 0,08 €. Rounding the exact net
        // total of twenty coins instead would print 0,17 € under two lines
        // that add up to 0,16 €.
        let totals = PrintedTotals::of(&[line(10), line(10)], dec!(0.2));
        assert_eq!(totals.net_total, dec!(0.16));
        assert_eq!(totals.vat_total, dec!(0.04));

        // A single line, the shape of every invoice.
        let totals = PrintedTotals::of(&[line(1337)], dec!(13.37));
        assert_eq!(totals.net_total, dec!(11.24));
        assert_eq!(totals.vat_total, dec!(2.13));

        for counts in [
            vec![1, 1, 1, 1],
            vec![7, 7, 7, 7, 7],
            vec![3, 9, 27, 81],
            vec![500],
            vec![1],
        ] {
            let items = counts.iter().copied().map(line).collect::<Vec<_>>();
            let gross_total = Decimal::from(counts.iter().sum::<u64>()) / dec!(100);
            let totals = PrintedTotals::of(&items, gross_total);

            assert_eq!(
                totals.net_total + totals.vat_total,
                gross_total.round_dp(AMOUNT_DECIMALS),
                "{counts:?}"
            );
        }
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

    fn captured_order(coins: u64, invoice_number: u64) -> PaypalCoinOrder {
        PaypalCoinOrder {
            id: PaypalOrderId::try_new(format!("order{invoice_number}")).unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: Some(FOO.user.created_at),
            coins,
            invoice_number,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        }
    }

    fn open_order(coins: u64, invoice_number: u64) -> PaypalCoinOrder {
        PaypalCoinOrder {
            captured_at: None,
            ..captured_order(coins, invoice_number)
        }
    }

    fn final_statement_customer_details() -> Vec<String> {
        FOO.invoice_info.clone().into_details(
            Some(FOO.profile.display_name.clone().into_inner()),
            FOO.user.email.as_ref().map(ToString::to_string),
        )
    }

    /// The statement records the unused share of the purchased Morphcoins and
    /// keeps the name and email address a later refund has to be offered to.
    #[tokio::test]
    async fn create_final_statement_ok() {
        // Arrange
        let now = within_retention();
        let pdf = vec![1, 2, 3, 4];

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new()
            .with_get_composite(FOO.user.id, Some(FOO.clone()))
            .with_get_number(FOO.user.id, 7);

        // Only the captured orders were paid and invoiced.
        let paypal_repo = MockPaypalRepository::new().with_list_coin_orders_by_user_id(
            FOO.user.id,
            vec![
                captured_order(1000, 1),
                open_order(9999, 2),
                captured_order(500, 3),
            ],
        );

        // 1200 coins left of 1500 bought, so 1200 are refundable.
        let coin_repo = MockCoinRepository::new().with_get_balance(
            FOO.user.id,
            Balance {
                coins: 1200,
                withheld_coins: 0,
            },
        );

        let prices = CoinPrices {
            net_unit: dec!(0.0084),
            net_total: dec!(10.08),
            vat_total: dec!(1.92),
            gross_total: dec!(12),
        };
        let finance_coin = MockFinanceCoinService::new()
            .with_get_price(1200, prices)
            .with_coins_per_euro(100);

        let customer_details = final_statement_customer_details();

        let document_repo = MockFinancialDocumentRepository::new().with_record(FinancialDocument {
            number: "S7".try_into().unwrap(),
            kind: FinancialDocumentKind::FinalStatement,
            user_id: Some(FOO.user.id),
            issued_at: now,
            customer_details: Some(customer_details.clone()),
            coins: Some(1200),
            net_total_cents: None,
            vat_total_cents: None,
            gross_total_cents: Some(1200),
        });

        let template = MockTemplateService::new().with_render(
            FinalStatementTemplate {
                title: "Schlussabrechnung",
                customer_details,
                timestamp: now,
                statement_number: "S7".into(),
                purchased_coins: 1500,
                balance_coins: 1200,
                unused_coins: 1200,
                coins_per_euro: 100,
                refund_amount: dec!(12),
            },
            "final-statement-html".into(),
        );

        let render_api = MockRenderApiService::new()
            .with_render_html_to_pdf("final-statement-html".into(), pdf.clone());

        let fs =
            MockFsService::new().with_store_file("/final_statements/S7.pdf".into(), pdf.clone());

        let sut = FinanceInvoiceServiceImpl {
            time,
            fs,
            user_repo,
            paypal_repo,
            coin_repo,
            document_repo,
            finance_coin,
            template,
            render_api,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), Some("S7".try_into().unwrap()));
    }

    /// Reward coins count as consumed first, so the unused share never exceeds
    /// what was bought.
    #[tokio::test]
    async fn create_final_statement_caps_at_the_purchased_coins() {
        // Arrange
        let now = within_retention();

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new()
            .with_get_composite(FOO.user.id, Some(FOO.clone()))
            .with_get_number(FOO.user.id, 7);

        let paypal_repo = MockPaypalRepository::new()
            .with_list_coin_orders_by_user_id(FOO.user.id, vec![captured_order(500, 1)]);

        // Far more coins than were bought, the rest are reward coins.
        let coin_repo = MockCoinRepository::new().with_get_balance(
            FOO.user.id,
            Balance {
                coins: 100_000,
                withheld_coins: 42,
            },
        );

        let prices = CoinPrices {
            net_unit: dec!(0.0084),
            net_total: dec!(4.2),
            vat_total: dec!(0.8),
            gross_total: dec!(5),
        };
        let finance_coin = MockFinanceCoinService::new()
            .with_get_price(500, prices)
            .with_coins_per_euro(100);

        let document_repo = MockFinancialDocumentRepository::new().with_record(FinancialDocument {
            number: "S7".try_into().unwrap(),
            kind: FinancialDocumentKind::FinalStatement,
            user_id: Some(FOO.user.id),
            issued_at: now,
            customer_details: Some(final_statement_customer_details()),
            coins: Some(500),
            net_total_cents: None,
            vat_total_cents: None,
            gross_total_cents: Some(500),
        });

        let template = MockTemplateService::new().with_render(
            FinalStatementTemplate {
                title: "Schlussabrechnung",
                customer_details: final_statement_customer_details(),
                timestamp: now,
                statement_number: "S7".into(),
                purchased_coins: 500,
                balance_coins: 100_000,
                unused_coins: 500,
                coins_per_euro: 100,
                refund_amount: dec!(5),
            },
            "final-statement-html".into(),
        );

        let render_api = MockRenderApiService::new()
            .with_render_html_to_pdf("final-statement-html".into(), vec![1]);

        let fs = MockFsService::new().with_store_file("/final_statements/S7.pdf".into(), vec![1]);

        let sut = FinanceInvoiceServiceImpl {
            time,
            fs,
            user_repo,
            paypal_repo,
            coin_repo,
            document_repo,
            finance_coin,
            template,
            render_api,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), Some("S7".try_into().unwrap()));
    }

    /// An account that never bought Morphcoins gets no statement, so nothing
    /// about it is kept beyond the deletion.
    #[tokio::test]
    async fn create_final_statement_without_purchases() {
        // Arrange
        let user_repo =
            MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

        let paypal_repo = MockPaypalRepository::new()
            .with_list_coin_orders_by_user_id(FOO.user.id, vec![open_order(1000, 1)]);

        let sut = FinanceInvoiceServiceImpl {
            user_repo,
            paypal_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }

    /// An account that spent everything it bought has nothing left that could
    /// be refunded, so it gets no statement either and its name and email
    /// address are not kept.
    #[tokio::test]
    async fn create_final_statement_without_anything_to_refund() {
        // Arrange
        let user_repo =
            MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

        let paypal_repo = MockPaypalRepository::new()
            .with_list_coin_orders_by_user_id(FOO.user.id, vec![captured_order(1000, 1)]);

        let coin_repo = MockCoinRepository::new().with_get_balance(
            FOO.user.id,
            Balance {
                coins: 0,
                withheld_coins: 0,
            },
        );

        let sut = FinanceInvoiceServiceImpl {
            user_repo,
            paypal_repo,
            coin_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn create_final_statement_user_not_found() {
        // Arrange
        let user_repo = MockUserRepository::new().with_get_composite(FOO.user.id, None);

        let sut = FinanceInvoiceServiceImpl {
            user_repo,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), None);
    }

    /// A render daemon that is unavailable must not stop an account from being
    /// deleted, so the record is kept even without its pdf.
    #[tokio::test]
    async fn create_final_statement_without_pdf() {
        // Arrange
        let now = within_retention();

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new()
            .with_get_composite(FOO.user.id, Some(FOO.clone()))
            .with_get_number(FOO.user.id, 7);

        let paypal_repo = MockPaypalRepository::new()
            .with_list_coin_orders_by_user_id(FOO.user.id, vec![captured_order(500, 1)]);

        let coin_repo = MockCoinRepository::new().with_get_balance(
            FOO.user.id,
            Balance {
                coins: 500,
                withheld_coins: 0,
            },
        );

        let finance_coin = MockFinanceCoinService::new()
            .with_get_price(
                500,
                CoinPrices {
                    net_unit: dec!(0.0084),
                    net_total: dec!(4.2),
                    vat_total: dec!(0.8),
                    gross_total: dec!(5),
                },
            )
            .with_coins_per_euro(100);

        let document_repo = MockFinancialDocumentRepository::new().with_record(FinancialDocument {
            number: "S7".try_into().unwrap(),
            kind: FinancialDocumentKind::FinalStatement,
            user_id: Some(FOO.user.id),
            issued_at: now,
            customer_details: Some(final_statement_customer_details()),
            coins: Some(500),
            net_total_cents: None,
            vat_total_cents: None,
            gross_total_cents: Some(500),
        });

        let template = MockTemplateService::new().with_render(
            FinalStatementTemplate {
                title: "Schlussabrechnung",
                customer_details: final_statement_customer_details(),
                timestamp: now,
                statement_number: "S7".into(),
                purchased_coins: 500,
                balance_coins: 500,
                unused_coins: 500,
                coins_per_euro: 100,
                refund_amount: dec!(5),
            },
            "final-statement-html".into(),
        );

        let mut render_api = MockRenderApiService::new();
        render_api
            .expect_render_html_to_pdf()
            .once()
            .return_once(|_| Box::pin(std::future::ready(Err(anyhow::anyhow!("no daemon")))));

        // Nothing is written to the archive.
        let fs = MockFsService::new();

        let sut = FinanceInvoiceServiceImpl {
            time,
            fs,
            user_repo,
            paypal_repo,
            coin_repo,
            document_repo,
            finance_coin,
            template,
            render_api,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(result.unwrap(), Some("S7".try_into().unwrap()));
    }
}
