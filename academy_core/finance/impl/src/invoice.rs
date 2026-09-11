use academy_core_finance_contracts::{
    coin::FinanceCoinService,
    invoice::{FinanceInvoiceService, PendingFinalStatement},
};
use academy_di::Build;
use academy_extern_contracts::render::RenderApiService;
use academy_models::{
    finance::{
        FinancialDocument, FinancialDocumentKind, FinancialDocumentNumber, final_statement_number,
        unused_purchased_coins,
    },
    paypal::PaypalPayment,
    retention::retention_cutoff,
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
    async fn get_original_pdf(
        &self,
        txn: &mut Txn,
        number: &FinancialDocumentNumber,
        kind: FinancialDocumentKind,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if !self.document_repo.lock_archive(txn, number).await? {
            return Ok(None);
        }
        let archive = match kind {
            FinancialDocumentKind::Invoice => {
                if let Some(pdf) = self.document_repo.original_invoice(txn, number).await? {
                    return Ok(Some(pdf));
                }
                &self.config.invoices_archive
            }
            FinancialDocumentKind::CreditNote => &self.config.credit_notes_archive,
            FinancialDocumentKind::FinalStatement => &self.config.final_statements_archive,
        };
        self.fs
            .read_file(&archive.join(format!("{}.pdf", number.as_str())))
            .await
    }

    async fn record_payment_invoice(
        &self,
        txn: &mut Txn,
        payment: &PaypalPayment,
    ) -> anyhow::Result<()> {
        let snapshot = &payment.snapshot;
        let capture = payment
            .capture
            .as_ref()
            .context("Payment has no capture evidence")?;
        self.document_repo
            .record(
                txn,
                &FinancialDocument {
                    number: format!("R{:07}", snapshot.order.invoice_number).try_into()?,
                    kind: FinancialDocumentKind::Invoice,
                    user_id: Some(snapshot.order.user_id),
                    issued_at: capture.created_at,
                    customer_details: Some(snapshot.customer_details.clone()),
                    coins: Some(snapshot.order.coins),
                    net_total_cents: to_cents(snapshot.net_total),
                    vat_total_cents: to_cents(snapshot.vat_total),
                    gross_total_cents: to_cents(snapshot.gross_total),
                    settled_at: None,
                    withdrawal_consent_at: snapshot.order.withdrawal_consent_at,
                    withdrawal_text_version: snapshot.order.withdrawal_text_version.clone(),
                },
            )
            .await
    }

    async fn render_payment_invoice(
        &self,
        txn: &mut Txn,
        payment: &PaypalPayment,
    ) -> anyhow::Result<Vec<u8>> {
        let snapshot = &payment.snapshot;
        let capture = payment
            .capture
            .as_ref()
            .context("Payment has no capture evidence")?;
        anyhow::ensure!(
            payment.fulfilled_at.is_some(),
            "Invoice has not been committed"
        );
        let cutoff = retention_cutoff(self.time.now(), self.config.retention_years)
            .context("Failed to determine document retention cutoff")?;
        anyhow::ensure!(
            capture.created_at >= cutoff,
            "Invoice retention period expired; unresolved delivery requires review"
        );
        let number = format!("R{:07}", snapshot.order.invoice_number);
        let path = self.config.invoices_archive.join(format!("{number}.pdf"));
        let document_number = FinancialDocumentNumber::try_new(number.clone())?;
        anyhow::ensure!(
            self.document_repo
                .lock_archive(txn, &document_number)
                .await?,
            "Retired invoice requires independent review, not recreation"
        );
        if let Some(pdf) = self
            .document_repo
            .original_invoice(txn, &document_number)
            .await?
        {
            return Ok(pdf);
        }
        if let Some(pdf) = self.fs.read_file(&path).await? {
            self.document_repo
                .record_original_invoice(
                    txn,
                    &document_number,
                    &pdf,
                    "existing_payment_invoice_archive",
                )
                .await?;
            return Ok(pdf);
        }
        let html = self.template.render(&InvoiceTemplate {
            title: "Rechnung",
            customer_details: snapshot.customer_details.clone(),
            timestamp: capture.created_at,
            invoice_number: number,
            items: vec![InvoiceItem {
                description: "MorphCoins".into(),
                net_unit: snapshot.net_unit,
                count: snapshot.order.coins,
                net_total: snapshot.net_total,
            }],
            vat_percent: snapshot.vat_percent,
            net_total: snapshot.net_total,
            vat_total: snapshot.vat_total,
            gross_total: snapshot.gross_total,
        })?;
        let pdf = self.render_api.render_html_to_pdf(html).await?;
        self.document_repo
            .record_original_invoice(
                txn,
                &document_number,
                &pdf,
                "first_render_from_complete_immutable_payment_snapshot",
            )
            .await?;
        self.fs.store_file(&path, &pdf).await?;
        Ok(pdf)
    }

    #[instrument(skip(self, txn))]
    async fn get_invoice_pdf(
        &self,
        txn: &mut Txn,
        user_id: Option<UserId>,
        invoice_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(payment) = self
            .paypal_repo
            .get_payment_by_invoice(txn, invoice_number)
            .await?
        {
            if user_id.is_some_and(|id| id != payment.snapshot.order.user_id)
                || payment.fulfilled_at.is_none()
            {
                return Ok(None);
            }
            return self.render_payment_invoice(txn, &payment).await.map(Some);
        }

        let formatted_invoice_number = format!("R{invoice_number:07}");
        let archive_path = self
            .config
            .invoices_archive
            .join(format!("{formatted_invoice_number}.pdf"));

        let number = FinancialDocumentNumber::try_new(formatted_invoice_number.clone())?;
        let coin_order = self
            .paypal_repo
            .get_coin_order_by_invoice_number(txn, invoice_number)
            .await?;
        // A live legacy order is authoritative for ownership. An orphan can
        // instead retain its owner in the issued financial document.
        let recorded = if coin_order.is_none() {
            self.document_repo.get(txn, &number).await?
        } else {
            None
        };
        if let Some(user) = user_id {
            let owner = coin_order
                .as_ref()
                .map(|o| o.user_id)
                .or_else(|| recorded.as_ref().and_then(|d| d.user_id));
            if owner != Some(user) {
                return Ok(None);
            }
        }
        if !self.document_repo.lock_archive(txn, &number).await? {
            return Ok(None);
        }
        // Authorization precedes archive access. The immutable original has the
        // same authority for legacy and durable payments, including admin use.
        if let Some(original) = self.document_repo.original_invoice(txn, &number).await? {
            return Ok(Some(original));
        }
        if let Some(invoice) = self.fs.read_file(&archive_path).await? {
            self.document_repo
                .record_original_invoice(txn, &number, &invoice, "existing_legacy_invoice_archive")
                .await?;
            return Ok(Some(invoice));
        }
        // Legacy records do not retain every original tax/price/capture fact.
        // A missing capture timestamp is unknown, never order.created_at. Keep a
        // repair obligation rather than issue a newly priced historical invoice.
        self.document_repo
            .flag_missing_invoice(txn, &number)
            .await?;
        Ok(None)
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

        let number = FinancialDocumentNumber::try_new(credit_note_number.clone())?;
        if !self.document_repo.lock_archive(txn, &number).await? {
            return Ok(None);
        }
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
                    // A credit note records no claim that could be settled.
                    settled_at: None,
                    // Only an order carries the declarations; a credit note
                    // certifies coins the user earned.
                    withdrawal_consent_at: None,
                    withdrawal_text_version: None,
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
    ) -> anyhow::Result<Option<PendingFinalStatement>> {
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

        // This legacy-format statement covers only an unused-purchase upper
        // bound. The independent commercial envelope preserves other balances,
        // service/instructor claims and unknown history even when this is zero.
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
                    // Historical settlement stamps are not payment proof. New
                    // dispositions use the independent reservation/outcome journal.
                    settled_at: None,
                    // A final statement is not an order either.
                    withdrawal_consent_at: None,
                    withdrawal_text_version: None,
                },
            )
            .await
            .context("Failed to record the final statement")?;

        let html = self
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

        // The pdf is produced by `archive_final_statement`, after the caller
        // has committed. Rendering it needs an http request, which must not be
        // made while the transaction that deletes the account is open.
        Ok(Some(PendingFinalStatement { number, html }))
    }

    #[instrument(skip(self, statement))]
    async fn archive_final_statement(&self, statement: PendingFinalStatement) {
        let PendingFinalStatement { number, html } = statement;
        let archive_path = self
            .config
            .final_statements_archive
            .join(format!("{}.pdf", *number));

        // Only the document number is logged. The statement itself names the
        // person it was issued for.
        let result = match self.render_api.render_html_to_pdf(html).await {
            Ok(statement_pdf) => self.fs.store_file(&archive_path, &statement_pdf).await,
            Err(err) => Err(err),
        };

        // `academy task list-orphan-documents` reports the records whose pdf
        // is missing, so the file can be produced later. The record itself is
        // what a refund needs and is already committed.
        if let Err(err) = result {
            tracing::warn!(
                document = %*number,
                "Failed to archive the pdf of a final statement: {err:#}"
            );
        }
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
    use academy_core_finance_contracts::coin::MockFinanceCoinService;
    use academy_demo::{
        UUID1,
        user::{BAR, FOO},
    };
    use academy_extern_contracts::render::MockRenderApiService;
    use academy_models::{
        coin::{Balance, Transaction},
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
    use academy_core_finance_contracts::coin::CoinPrices;

    // Existing nonretired fixtures permit the new archive serialization read.
    // Real PostgreSQL/CLI controls cover retirement admission and conflicting imports.
    fn existing_archive_documents() -> MockFinancialDocumentRepository<()> {
        let mut repo = MockFinancialDocumentRepository::new();
        repo.expect_lock_archive()
            .returning(|_, _| Box::pin(async { Ok(true) }));
        repo
    }

    fn legacy_paypal_repo() -> MockPaypalRepository<()> {
        let mut repo = MockPaypalRepository::new();
        repo.expect_get_payment_by_invoice()
            .returning(|_, _| Box::pin(std::future::ready(Ok(None))));
        repo
    }

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

    fn legacy_order(captured: bool) -> PaypalCoinOrder {
        PaypalCoinOrder {
            id: PaypalOrderId::try_new("legacy42").unwrap(),
            user_id: FOO.user.id,
            created_at: FOO.user.created_at,
            captured_at: captured.then_some(FOO.user.created_at + chrono::Duration::days(3)),
            coins: 1337,
            invoice_number: 42,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        }
    }

    fn legacy_cached_documents() -> MockFinancialDocumentRepository<()> {
        let mut repo = existing_archive_documents();
        repo.expect_original_invoice()
            .once()
            .returning(|_, _| Box::pin(async { Ok(None) }));
        repo.expect_record_original_invoice()
            .once()
            .returning(|_, _, _, provenance| {
                assert_eq!(provenance, "existing_legacy_invoice_archive");
                Box::pin(async { Ok(()) })
            });
        repo
    }

    #[tokio::test]
    async fn legacy_missing_pdf_does_not_invent_price_tax_customer_or_capture() {
        // Even a known capture does not supply historical price/tax/customer
        // facts. Both variants retain an unresolved repair obligation. All
        // finance, user, renderer and money mocks have zero expectations.
        for captured in [false, true] {
            let mut documents = existing_archive_documents();
            documents
                .expect_original_invoice()
                .once()
                .returning(|_, _| Box::pin(async { Ok(None) }));
            documents
                .expect_flag_missing_invoice()
                .once()
                .returning(|_, _| Box::pin(async { Ok(()) }));
            let sut = Sut {
                fs: MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), None),
                paypal_repo: legacy_paypal_repo()
                    .with_get_coin_order_by_invoice_number(42, Some(legacy_order(captured))),
                document_repo: documents,
                ..Sut::default()
            };
            assert_eq!(
                sut.get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
                    .await
                    .unwrap(),
                None
            );
        }
    }

    #[tokio::test]
    async fn legacy_original_bytes_are_retrieved_without_reconstruction_or_payment_claim() {
        // Original document existence is useful even with unknown capture.
        // Neither lookup nor byte retrieval mutates money, customer or status.
        let original = b"%PDF-original historical invoice with exact rounding".to_vec();
        let expected = original.clone();
        let mut documents = existing_archive_documents();
        documents
            .expect_original_invoice()
            .once()
            .returning(move |_, _| {
                let pdf = original.clone();
                Box::pin(async { Ok(Some(pdf)) })
            });
        let sut = Sut {
            // Even a conflicting filesystem archive must not be consulted.
            fs: MockFsService::new(),
            paypal_repo: legacy_paypal_repo()
                .with_get_coin_order_by_invoice_number(42, Some(legacy_order(false))),
            document_repo: documents,
            ..Sut::default()
        };
        assert_eq!(
            sut.get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
                .await
                .unwrap(),
            Some(expected)
        );
    }

    #[tokio::test]
    async fn orphan_original_requires_retained_owner_for_customer_access() {
        let original = b"%PDF-orphan original".to_vec();
        let record = FinancialDocument {
            number: "R0000042".try_into().unwrap(),
            kind: FinancialDocumentKind::Invoice,
            user_id: Some(FOO.user.id),
            issued_at: FOO.user.created_at,
            customer_details: Some(vec!["Historical recipient".into()]),
            coins: Some(1337),
            net_total_cents: Some(1124),
            vat_total_cents: Some(213),
            gross_total_cents: Some(1337),
            settled_at: None,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
        };
        for (user, allowed) in [(FOO.user.id, true), (BAR.user.id, false)] {
            let mut documents = existing_archive_documents()
                .with_get("R0000042".try_into().unwrap(), Some(record.clone()));
            if allowed {
                let pdf = original.clone();
                documents
                    .expect_original_invoice()
                    .once()
                    .returning(move |_, _| {
                        let pdf = pdf.clone();
                        Box::pin(async { Ok(Some(pdf)) })
                    });
            }
            let sut = Sut {
                fs: MockFsService::new(),
                paypal_repo: legacy_paypal_repo().with_get_coin_order_by_invoice_number(42, None),
                document_repo: documents,
                ..Sut::default()
            };
            assert_eq!(
                sut.get_invoice_pdf(&mut (), Some(user), 42).await.unwrap(),
                allowed.then(|| original.clone())
            );
        }
    }

    #[tokio::test]
    async fn get_invoice_cached_no_user_id_check() {
        // Arrange
        let pdf = vec![1, 2, 3, 4];

        let fs =
            MockFsService::new().with_read_file("/invoices/R0000042.pdf".into(), Some(pdf.clone()));

        let sut = FinanceInvoiceServiceImpl {
            paypal_repo: legacy_paypal_repo().with_get_coin_order_by_invoice_number(42, None),
            document_repo: legacy_cached_documents().with_get("R0000042".try_into().unwrap(), None),
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
        let paypal_repo =
            legacy_paypal_repo().with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            document_repo: legacy_cached_documents(),
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
    async fn legacy_owner_without_capture_can_retrieve_existing_archive() {
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
        let paypal_repo =
            legacy_paypal_repo().with_get_coin_order_by_invoice_number(42, Some(order.clone()));

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            document_repo: legacy_cached_documents(),
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
        let fs = MockFsService::new();

        let paypal_repo = legacy_paypal_repo().with_get_coin_order_by_invoice_number(42, None);

        let sut = FinanceInvoiceServiceImpl {
            fs,
            paypal_repo,
            document_repo: existing_archive_documents()
                .with_get("R0000042".try_into().unwrap(), None),
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

        let fs = MockFsService::new();

        let paypal_repo =
            legacy_paypal_repo().with_get_coin_order_by_invoice_number(42, Some(order.clone()));

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

        let document_repo = existing_archive_documents()
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
                settled_at: None,
                withdrawal_consent_at: None,
                withdrawal_text_version: None,
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
            existing_archive_documents().with_get("G202402-7".try_into().unwrap(), None);

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
            document_repo: existing_archive_documents(),
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
            document_repo: existing_archive_documents(),
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

        let time = MockTimeService::new().with_now(now);

        let user_repo = MockUserRepository::new()
            .with_get_composite(FOO.user.id, Some(FOO.clone()))
            .with_get_number(FOO.user.id, 7);

        // Only the captured orders were paid and invoiced.
        let paypal_repo = legacy_paypal_repo().with_list_coin_orders_by_user_id(
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

        let document_repo = existing_archive_documents().with_record(FinancialDocument {
            number: "S7".try_into().unwrap(),
            kind: FinancialDocumentKind::FinalStatement,
            user_id: Some(FOO.user.id),
            issued_at: now,
            customer_details: Some(customer_details.clone()),
            coins: Some(1200),
            net_total_cents: None,
            vat_total_cents: None,
            gross_total_cents: Some(1200),
            settled_at: None,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
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

        // Nothing is rendered or written yet: the pdf is produced by
        // `archive_final_statement`, after the caller has committed.
        let sut = FinanceInvoiceServiceImpl {
            time,
            user_repo,
            paypal_repo,
            coin_repo,
            document_repo,
            finance_coin,
            template,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(
            result.unwrap(),
            Some(PendingFinalStatement {
                number: "S7".try_into().unwrap(),
                html: "final-statement-html".into(),
            })
        );
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

        let paypal_repo = legacy_paypal_repo()
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

        let document_repo = existing_archive_documents().with_record(FinancialDocument {
            number: "S7".try_into().unwrap(),
            kind: FinancialDocumentKind::FinalStatement,
            user_id: Some(FOO.user.id),
            issued_at: now,
            customer_details: Some(final_statement_customer_details()),
            coins: Some(500),
            net_total_cents: None,
            vat_total_cents: None,
            gross_total_cents: Some(500),
            settled_at: None,
            withdrawal_consent_at: None,
            withdrawal_text_version: None,
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

        let sut = FinanceInvoiceServiceImpl {
            time,
            user_repo,
            paypal_repo,
            coin_repo,
            document_repo,
            finance_coin,
            template,
            ..Sut::default()
        };

        // Act
        let result = sut.create_final_statement(&mut (), FOO.user.id).await;

        // Assert
        assert_eq!(
            result.unwrap(),
            Some(PendingFinalStatement {
                number: "S7".try_into().unwrap(),
                html: "final-statement-html".into(),
            })
        );
    }

    /// An account that never bought Morphcoins gets no statement, so nothing
    /// about it is kept beyond the deletion.
    #[tokio::test]
    async fn create_final_statement_without_purchases() {
        // Arrange
        let user_repo =
            MockUserRepository::new().with_get_composite(FOO.user.id, Some(FOO.clone()));

        let paypal_repo = legacy_paypal_repo()
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

        let paypal_repo = legacy_paypal_repo()
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

    /// The pdf is produced outside the transaction that deleted the account,
    /// and only then is it archived.
    #[tokio::test]
    async fn archive_final_statement_ok() {
        // Arrange
        let pdf = vec![1, 2, 3, 4];

        let render_api = MockRenderApiService::new()
            .with_render_html_to_pdf("final-statement-html".into(), pdf.clone());

        let fs =
            MockFsService::new().with_store_file("/final_statements/S7.pdf".into(), pdf.clone());

        let sut = FinanceInvoiceServiceImpl {
            fs,
            render_api,
            ..Sut::default()
        };

        // Act
        sut.archive_final_statement(PendingFinalStatement {
            number: "S7".try_into().unwrap(),
            html: "final-statement-html".into(),
        })
        .await;
    }

    /// A render daemon that is unavailable must not stop an account from being
    /// deleted, so the failure is only logged; the record is already
    /// committed.
    #[tokio::test]
    async fn archive_final_statement_without_a_render_daemon() {
        // Arrange
        let mut render_api = MockRenderApiService::new();
        render_api
            .expect_render_html_to_pdf()
            .once()
            .return_once(|_| Box::pin(std::future::ready(Err(anyhow::anyhow!("no daemon")))));

        // Nothing is written to the archive.
        let fs = MockFsService::new();

        let sut = FinanceInvoiceServiceImpl {
            fs,
            render_api,
            ..Sut::default()
        };

        // Act
        sut.archive_final_statement(PendingFinalStatement {
            number: "S7".try_into().unwrap(),
            html: "final-statement-html".into(),
        })
        .await;
    }
    #[tokio::test]
    async fn retained_original_prefers_immutable_bytes_without_render_or_identity() {
        let expected = b"%PDF-exact issued original".to_vec();
        let pdf = expected.clone();
        let mut document_repo = existing_archive_documents();
        document_repo
            .expect_original_invoice()
            .once()
            .return_once(|_, _| Box::pin(async move { Ok(Some(pdf)) }));
        let sut = Sut {
            document_repo,
            ..Sut::default()
        };
        assert_eq!(
            sut.get_original_pdf(
                &mut (),
                &"R0000042".try_into().unwrap(),
                FinancialDocumentKind::Invoice
            )
            .await
            .unwrap(),
            Some(expected)
        );
        // All renderer, user, wallet, filesystem and record-write mocks have
        // zero expectations, including if another cached version existed.
    }

    #[tokio::test]
    async fn retained_archive_and_missing_bytes_never_generate_financial_facts() {
        for (kind, number, path) in [
            (
                FinancialDocumentKind::Invoice,
                "R0000042",
                "/invoices/R0000042.pdf",
            ),
            (
                FinancialDocumentKind::CreditNote,
                "G202608-7",
                "/credit_notes/G202608-7.pdf",
            ),
        ] {
            for pdf in [None, Some(b"%PDF-existing archived bytes".to_vec())] {
                let mut document_repo = existing_archive_documents();
                if kind == FinancialDocumentKind::Invoice {
                    document_repo
                        .expect_original_invoice()
                        .once()
                        .returning(|_, _| Box::pin(async { Ok(None) }));
                }
                let sut = Sut {
                    document_repo,
                    fs: MockFsService::new().with_read_file(path.into(), pdf.clone()),
                    ..Sut::default()
                };
                assert_eq!(
                    sut.get_original_pdf(&mut (), &number.try_into().unwrap(), kind)
                        .await
                        .unwrap(),
                    pdf
                );
                // Missing original is still missing: no render, user-number,
                // record/adoption, reconciliation or settlement write occurs.
            }
        }
    }

    fn pending_identity_documents() -> MockFinancialDocumentRepository<()> {
        let mut repo = MockFinancialDocumentRepository::new();
        repo.expect_lock_archive().once().returning(|_, _| {
            Box::pin(async {
                anyhow::bail!("Original invoice identity is pending independent review")
            })
        });
        repo
    }

    #[tokio::test]
    async fn identity_pending_stops_original_before_database_or_archive_fallback() {
        let sut = Sut {
            document_repo: pending_identity_documents(),
            ..Sut::default()
        };
        let error = sut
            .get_original_pdf(
                &mut (),
                &"R1000000".try_into().unwrap(),
                FinancialDocumentKind::Invoice,
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("pending independent review"));
        // All original, filesystem, adoption, renderer and user mocks have no
        // expected call. Real SQL separately proves a nonempty stored PDF is fenced.
    }

    #[tokio::test]
    async fn identity_pending_stops_legacy_fallback_after_owner_admission() {
        let sut = Sut {
            paypal_repo: legacy_paypal_repo()
                .with_get_coin_order_by_invoice_number(42, Some(legacy_order(true))),
            document_repo: pending_identity_documents(),
            ..Sut::default()
        };
        assert!(
            sut.get_invoice_pdf(&mut (), Some(FOO.user.id), 42)
                .await
                .unwrap_err()
                .to_string()
                .contains("pending independent review")
        );
        let foreign = Sut {
            paypal_repo: legacy_paypal_repo()
                .with_get_coin_order_by_invoice_number(42, Some(legacy_order(true))),
            ..Sut::default()
        };
        assert_eq!(
            foreign
                .get_invoice_pdf(&mut (), Some(BAR.user.id), 42)
                .await
                .unwrap(),
            None
        );
        // Foreign ownership is denied before archive/fence inspection. Owned
        // pending cannot read files, flag absence, adopt or render an original.
    }

    #[tokio::test]
    async fn identity_pending_stops_fulfilled_payment_before_first_render_or_archive() {
        use academy_models::paypal::{PaypalCapture, PaypalPaymentSnapshot};
        let at = FOO.user.created_at;
        let payment = PaypalPayment {
            snapshot: PaypalPaymentSnapshot {
                order: legacy_order(true),
                request_id: UUID1,
                merchant_id: "synthetic".into(),
                currency: "EUR".into(),
                gross_total: dec!(1),
                net_unit: dec!(1),
                net_total: dec!(1),
                vat_total: dec!(0),
                vat_percent: dec!(0),
                customer_details: vec!["Synthetic original".into()],
                recipient: "Synthetic <synthetic@example.invalid>".parse().unwrap(),
                consent_text: "Synthetic original".into(),
                contract_order_id: None,
                provision_deadline: None,
            },
            started_at: Some(at),
            attempts: 1,
            capture: Some(PaypalCapture {
                id: "synthetic-capture".into(),
                status: "COMPLETED".into(),
                currency: "EUR".into(),
                amount: dec!(1),
                created_at: at,
            }),
            balance: None,
            fulfilled_at: Some(at),
            receipt_sent_at: None,
            receipt_attempts: 0,
            last_error: None,
        };
        let sut = Sut {
            time: MockTimeService::new().with_now(at),
            document_repo: pending_identity_documents(),
            ..Sut::default()
        };
        assert!(
            sut.render_payment_invoice(&mut (), &payment)
                .await
                .unwrap_err()
                .to_string()
                .contains("pending independent review")
        );
        // A real complete synthetic payment reaches the guard; no DB original,
        // filesystem, template, renderer, adoption or storage effect follows.
    }
}
