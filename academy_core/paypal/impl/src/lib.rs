use std::ops::RangeInclusive;

use academy_auth_contracts::{AuthResultExt, AuthService};
use academy_core_finance_contracts::{coin::FinanceCoinService, invoice::FinanceInvoiceService};
use academy_core_paypal_contracts::{
    PaypalCaptureCoinOrderError, PaypalCreateCoinOrderError, PaypalFeatureService,
    coin_order::PaypalCoinOrderService,
};
use academy_core_purchase_contracts::PurchaseFeatureService;
use academy_di::Build;
use academy_extern_contracts::paypal::{PaypalApiService, PaypalCreateOrderError};
use academy_models::{
    auth::AccessToken,
    coin::Balance,
    paypal::{PaypalOrderId, PaypalPayment, PaypalPaymentSnapshot, PaypalRemoteOrder},
    purchase::{PurchaseAcceptance, PurchaseProduct, PurchaseStatus},
    user::UserComposite,
};
use academy_persistence_contracts::{
    Database, Transaction, paypal::PaypalRepository, user::UserRepository,
};
use academy_utils::trace_instrument;
use anyhow::{Context, anyhow};
use chrono::{TimeDelta, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use tracing::warn;
use uuid::Uuid;

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
    Purchase,
    FinanceInvoice,
    FinanceCoin,
> {
    db: Db,
    auth: Auth,
    paypal_api: PaypalApi,
    user_repo: UserRepo,
    paypal_repo: PaypalRepo,
    paypal_coin_order: PaypalCoinOrder,
    purchase: Purchase,
    finance_invoice: FinanceInvoice,
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
    Purchase,
    FinanceInvoice,
    FinanceCoin,
> PaypalFeatureService
    for PaypalFeatureServiceImpl<
        Db,
        Auth,
        PaypalApi,
        UserRepo,
        PaypalRepo,
        PaypalCoinOrder,
        Purchase,
        FinanceInvoice,
        FinanceCoin,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    PaypalApi: PaypalApiService,
    UserRepo: UserRepository<Db::Transaction>,
    PaypalRepo: PaypalRepository<Db::Transaction>,
    PaypalCoinOrder: PaypalCoinOrderService<Db::Transaction>,
    Purchase: PurchaseFeatureService,
    FinanceInvoice: FinanceInvoiceService<Db::Transaction>,
    FinanceCoin: FinanceCoinService,
{
    async fn offer_coin_order(
        &self,
        token: &AccessToken,
        coins: u64,
    ) -> Result<PurchaseStatus, PaypalCreateCoinOrderError> {
        if !self.config.purchase_range.contains(&coins) {
            return Err(PaypalCreateCoinOrderError::InvalidAmount(
                self.config.purchase_range.clone(),
            ));
        }
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_email_verified().map_auth_err()?;
        let mut txn = self.db.begin_transaction().await?;
        let user = self
            .user_repo
            .get_composite(&mut txn, auth.user_id)
            .await?
            .context("Missing verified account")?;
        if !user.can_buy_coins() {
            return Err(PaypalCreateCoinOrderError::IncompleteInvoiceInfo);
        }
        txn.commit().await?;
        self.purchase
            .cash_offer(token, self.coin_product(coins, &user))
            .await
            .map_err(|e| anyhow!(e).into())
    }
    #[trace_instrument(skip(self))]
    fn get_client_id(&self) -> &str {
        self.paypal_api.client_id()
    }

    #[trace_instrument(skip(self))]
    async fn create_coin_order(
        &self,
        token: &AccessToken,
        coins: u64,
        declaration: PurchaseAcceptance,
    ) -> Result<PaypalOrderId, PaypalCreateCoinOrderError> {
        let auth = self.auth.authenticate(token).await.map_auth_err()?;
        auth.ensure_email_verified().map_auth_err()?;
        let offered = self
            .purchase
            .get(token, declaration.order_id)
            .await
            .map_err(|_| PaypalCreateCoinOrderError::OfferChanged)?;
        if offered.offer.source != "paypal" || offered.offer.product.coins != coins {
            return Err(PaypalCreateCoinOrderError::OfferChanged);
        }
        if offered.state == "offered" {
            if !self.config.purchase_range.contains(&coins) {
                return Err(PaypalCreateCoinOrderError::InvalidAmount(
                    self.config.purchase_range.clone(),
                ));
            }
            let mut txn = self.db.begin_transaction().await?;
            let current = self
                .user_repo
                .get_composite(&mut txn, auth.user_id)
                .await?
                .context("Missing account")?;
            if !current.can_buy_coins() {
                return Err(PaypalCreateCoinOrderError::IncompleteInvoiceInfo);
            }
            if self.coin_product(coins, &current) != offered.offer.product {
                return Err(PaypalCreateCoinOrderError::OfferChanged);
            }
            txn.commit().await?;
        }
        let accepted = self
            .purchase
            .cash_accept(token, declaration)
            .await
            .map_err(|_| PaypalCreateCoinOrderError::OfferChanged)?;
        if accepted.accepted_at.is_none() || accepted.state == "failed" {
            return Err(PaypalCreateCoinOrderError::OfferChanged);
        }
        let mut txn = self.db.begin_transaction().await?;
        if let Some(existing) = self
            .paypal_repo
            .lock_contract_order(&mut txn, accepted.offer.id)
            .await?
        {
            txn.commit().await?;
            return Ok(existing);
        }
        // Unapproved remote creation may be retried after a lost response; no ID
        // is exposed until its original accepted offer and payment snapshot commit.
        let facts = &accepted.offer.product.facts;
        let withdrawal_text_version = "L1-request-2026-09"
            .try_into()
            .context("Request revision")?;
        let order_id = self
            .paypal_api
            .create_order(coins)
            .await
            .map_err(|err| match err {
                PaypalCreateOrderError::Failed => PaypalCreateCoinOrderError::CreateOrderFailure,
                PaypalCreateOrderError::Other(err) => err.into(),
            })?;

        let order = academy_models::paypal::PaypalCoinOrder {
            id: order_id,
            user_id: auth.user_id,
            created_at: Utc::now(),
            captured_at: None,
            coins,
            invoice_number: self.paypal_repo.get_next_invoice_number(&mut txn).await?,
            withdrawal_consent_at: accepted.accepted_at,
            withdrawal_text_version: Some(withdrawal_text_version),
        };
        self.paypal_repo.create_coin_order(&mut txn, &order).await?;

        // Verify the provider-created order and persist all commercial facts before returning
        // its ID to the browser. A create response lost before this commit cannot be approved.
        let remote = self.paypal_api.get_order(&order.id).await?;
        let amount = |name: &str| {
            facts[name]
                .as_str()
                .context("Missing accepted price")?
                .parse::<rust_decimal::Decimal>()
                .context("Invalid accepted price")
        };
        let snapshot = PaypalPaymentSnapshot {
            contract_order_id: Some(accepted.offer.id),
            provision_deadline: accepted.provision_deadline(),
            order: order.clone(),
            request_id: Uuid::new_v4(),
            merchant_id: remote.merchant_id.clone(),
            currency: "EUR".into(),
            gross_total: amount("gross_total")?,
            net_unit: amount("net_unit")?,
            net_total: amount("net_total")?,
            vat_total: amount("vat_total")?,
            vat_percent: amount("vat_percent")?,
            customer_details: serde_json::from_value(facts["customer_details"].clone())
                .context("Accepted invoice details")?,
            recipient: accepted
                .offer
                .recipient
                .parse()
                .context("Accepted recipient")?,
            consent_text: accepted.offer.declaration,
        };
        if !snapshot.matches_remote(&remote) || !remote.captures.is_empty() {
            return Err(anyhow!("Unexpected created PayPal order").into());
        }
        self.paypal_repo
            .create_payment(
                &mut txn,
                &PaypalPayment {
                    snapshot,
                    started_at: None,
                    attempts: 0,
                    capture: None,
                    balance: None,
                    fulfilled_at: None,
                    receipt_sent_at: None,
                    receipt_attempts: 0,
                    last_error: None,
                },
            )
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
        let Some(mut payment) = self.paypal_repo.get_payment(&mut txn, &order_id).await? else {
            // A pre-migration order can already have an unrecorded capture. No blind adoption.
            let legacy = self.paypal_repo.get_coin_order(&mut txn, &order_id).await?;
            return Err(if legacy.is_some_and(|o| o.user_id == auth.user_id) {
                PaypalCaptureCoinOrderError::Pending
            } else {
                PaypalCaptureCoinOrderError::NotFound
            });
        };
        if payment.snapshot.order.user_id != auth.user_id {
            return Err(PaypalCaptureCoinOrderError::NotFound);
        }
        if payment.started_at.is_none() {
            payment.started_at = Some(Utc::now());
            self.paypal_repo.update_payment(&mut txn, &payment).await?;
        }
        // A durable intent exists before any provider capture is attempted.
        txn.commit().await?;
        match self.process_payment(&order_id).await {
            Ok(Some(balance)) => Ok(balance),
            Ok(None) => Err(PaypalCaptureCoinOrderError::Pending),
            Err(err) => {
                warn!(order_id=?order_id, error=%err, "PayPal payment requires reconciliation");
                self.record_failure(&order_id, "local_processing_failed")
                    .await;
                Err(PaypalCaptureCoinOrderError::Pending)
            }
        }
    }

    async fn retry_payments(&self) -> anyhow::Result<()> {
        let mut txn = self.db.begin_transaction().await?;
        let ids = self.paypal_repo.pending_payments(&mut txn).await?;
        txn.commit().await?;
        for id in ids {
            if let Err(err) = self.process_payment(&id).await {
                warn!(order_id=?id, error=%err, "PayPal recovery failed; obligation retained");
                self.record_failure(&id, "local_processing_failed").await;
            }
        }
        Ok(())
    }
}

impl<
    Db,
    Auth,
    PaypalApi,
    UserRepo,
    PaypalRepo,
    PaypalCoinOrder,
    Purchase,
    FinanceInvoice,
    FinanceCoin,
>
    PaypalFeatureServiceImpl<
        Db,
        Auth,
        PaypalApi,
        UserRepo,
        PaypalRepo,
        PaypalCoinOrder,
        Purchase,
        FinanceInvoice,
        FinanceCoin,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    PaypalApi: PaypalApiService,
    UserRepo: UserRepository<Db::Transaction>,
    PaypalRepo: PaypalRepository<Db::Transaction>,
    PaypalCoinOrder: PaypalCoinOrderService<Db::Transaction>,
    Purchase: PurchaseFeatureService,
    FinanceInvoice: FinanceInvoiceService<Db::Transaction>,
    FinanceCoin: FinanceCoinService,
{
    fn coin_product(&self, coins: u64, user: &UserComposite) -> PurchaseProduct {
        let price = self.finance_coin.get_price(coins);
        let facts = json!({"gross_total":price.gross_total.to_string(),"net_unit":price.net_unit.to_string(),"net_total":price.net_total.round_dp(2).to_string(),"vat_total":(price.gross_total-price.net_total.round_dp(2)).to_string(),"vat_percent":self.finance_coin.vat_percent().to_string(),"customer_details":user.invoice_info.clone().into_details(Some(user.profile.display_name.to_string()),user.user.email.as_ref().map(ToString::to_string))});
        PurchaseProduct {
            kind: "coins".into(),
            reference: coins.to_string(),
            title: format!("{coins} MorphCoins"),
            description: format!(
                "Du kaufst einmalig {coins} MorphCoins für Angebote auf Bootstrap Academy. Kein Abo.\nRechnungsangaben: {}",
                facts["customer_details"]
                    .as_array()
                    .map(|a| a
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", "))
                    .unwrap_or_default()
            ),
            coins,
            revision: format!("{:x}", Sha256::digest(facts.to_string().as_bytes())),
            facts,
            service_starts_at: None,
        }
    }
    async fn record_failure(&self, id: &PaypalOrderId, failure: &str) {
        let result = async {
            let mut txn = self.db.begin_transaction().await?;
            if let Some(mut payment) = self.paypal_repo.get_payment(&mut txn, id).await?
                && payment.receipt_sent_at.is_none()
            {
                payment.last_error = Some(failure.into());
                self.paypal_repo.update_payment(&mut txn, &payment).await?;
            }
            txn.commit().await
        }
        .await;
        if let Err(err) = result {
            warn!(order_id=?id, error=%err, "Could not record payment failure; pending work remains");
        }
    }

    async fn process_payment(&self, id: &PaypalOrderId) -> anyhow::Result<Option<Balance>> {
        // Persist attempts before crossing the provider boundary. A restart observes the same key.
        let mut txn = self.db.begin_transaction().await?;
        let mut payment = self
            .paypal_repo
            .get_payment(&mut txn, id)
            .await?
            .context("Payment missing")?;
        if payment.started_at.is_none() {
            return Ok(None);
        }
        if payment.capture.is_none() {
            payment.attempts += 1;
            self.paypal_repo.update_payment(&mut txn, &payment).await?;
        }
        txn.commit().await?;

        let mut txn = self.db.begin_transaction().await?;
        payment = self
            .paypal_repo
            .get_payment(&mut txn, id)
            .await?
            .context("Payment missing")?;
        if payment.capture.is_none() {
            // The lock serializes provider attempts too. The durable key protects the crash window.
            let recipient_exists = self
                .user_repo
                .get_composite(&mut txn, payment.snapshot.order.user_id)
                .await?
                .is_some();
            match self.reconcile_capture(&payment, recipient_exists).await {
                Ok(remote)
                    if payment.snapshot.matches_remote(&remote)
                        && remote.status == "COMPLETED"
                        && remote.captures.len() == 1
                        && remote.captures[0].status == "COMPLETED" =>
                {
                    payment.capture = remote.captures.into_iter().next();
                    payment.last_error = None;
                }
                Ok(remote) => {
                    payment.last_error = Some(
                        if payment
                            .snapshot
                            .provision_deadline
                            .is_some_and(|d| Utc::now() >= d)
                        {
                            "provision_deadline_passed_provider_outcome_unresolved"
                        } else if payment.snapshot.matches_remote(&remote) {
                            "provider_outcome_unresolved"
                        } else {
                            "provider_evidence_mismatch"
                        }
                        .into(),
                    );
                }
                Err(_) => {
                    payment.last_error = Some("provider_unavailable_or_unproven".into());
                }
            }
            self.paypal_repo.update_payment(&mut txn, &payment).await?;
        }
        // Capture proof survives a later failure to credit, issue an invoice or commit fulfillment.
        txn.commit().await?;
        if payment.capture.is_none() {
            return Ok(None);
        }

        let mut provision_deadline = None;
        let mut provision_ready = true;
        if payment.snapshot.contract_order_id.is_some() && payment.balance.is_none() {
            let status = self.purchase.cash_captured(payment.clone()).await?;
            provision_deadline = status.provision_deadline();
            provision_ready =
                status.confirmation_smtp_accepted_at.is_some() && status.state == "paid";
        }
        let mut txn = self.db.begin_transaction().await?;
        payment = self
            .paypal_repo
            .get_payment(&mut txn, id)
            .await?
            .context("Payment missing")?;
        if payment.balance.is_none() {
            // No account recreation or alternative settlement destination is invented here.
            if self
                .user_repo
                .get_composite(&mut txn, payment.snapshot.order.user_id)
                .await?
                .is_none()
            {
                payment.last_error = Some("recipient_missing_requires_settlement_review".into());
                self.paypal_repo.update_payment(&mut txn, &payment).await?;
                txn.commit().await?;
                return Ok(None);
            }
            if !provision_ready || provision_deadline.is_some_and(|d| Utc::now() >= d) {
                payment.last_error = Some(
                    if provision_deadline.is_some_and(|d| Utc::now() >= d) {
                        "provision_deadline_passed_requires_review"
                    } else {
                        "contract_confirmation_pending_or_requires_review"
                    }
                    .into(),
                );
                self.paypal_repo.update_payment(&mut txn, &payment).await?;
                txn.commit().await?;
                return Ok(None);
            }
            let mut order = payment.snapshot.order.clone();
            order.captured_at = payment.capture.as_ref().map(|capture| capture.created_at);
            let balance = self.paypal_coin_order.capture(&mut txn, order).await?;
            self.finance_invoice
                .record_payment_invoice(&mut txn, &payment)
                .await?;
            payment.balance = Some(balance);
            payment.fulfilled_at = Some(Utc::now());
            payment.last_error = None;
            self.paypal_repo.update_payment(&mut txn, &payment).await?;
        }
        txn.commit().await?;
        // PostgreSQL timestamps have microsecond precision. Report the actual
        // committed row so first completion and crash/replay compare identical
        // facts, never a pre-commit nanosecond value.
        let mut txn = self.db.begin_transaction().await?;
        payment = self
            .paypal_repo
            .get_payment(&mut txn, id)
            .await?
            .context("Payment missing")?;
        txn.commit().await?;
        let balance = payment.balance;
        if payment.snapshot.contract_order_id.is_some() {
            self.purchase.cash_fulfilled(payment.clone()).await?;
        }
        // PDF/storage/SMTP failures never change a committed purchase or its successful replay.
        if let Err(err) = self.deliver_receipt(id).await {
            warn!(order_id=?id, error=%err, "PayPal receipt remains pending");
            self.record_failure(id, "receipt_delivery_failed").await;
        }
        Ok(balance)
    }

    async fn reconcile_capture(
        &self,
        payment: &PaypalPayment,
        recipient_exists: bool,
    ) -> anyhow::Result<PaypalRemoteOrder> {
        let snapshot = &payment.snapshot;
        let remote = self.paypal_api.get_order(&snapshot.order.id).await?;
        if !snapshot.matches_remote(&remote)
            || !remote.captures.is_empty()
            || remote.status != "APPROVED"
            || !recipient_exists
        {
            return Ok(remote);
        }
        // PayPal's documented default is six hours. Use five with margin, never a fresh key.
        // Outside this window only read reconciliation is allowed; absence of proof is not failure.
        let elapsed = Utc::now() - payment.started_at.context("Payment not started")?;
        if elapsed < TimeDelta::zero()
            || elapsed >= TimeDelta::hours(5)
            || snapshot.provision_deadline.is_some_and(|d| Utc::now() >= d)
        {
            return Ok(remote);
        }
        match self
            .paypal_api
            .capture_order(&snapshot.order.id, snapshot.request_id)
            .await
        {
            Ok(remote) => Ok(remote),
            // This also resolves ORDER_ALREADY_CAPTURED and a lost successful response.
            Err(_) => self.paypal_api.get_order(&snapshot.order.id).await,
        }
    }

    async fn deliver_receipt(&self, id: &PaypalOrderId) -> anyhow::Result<()> {
        let mut txn = self.db.begin_transaction().await?;
        let mut payment = self
            .paypal_repo
            .get_payment(&mut txn, id)
            .await?
            .context("Payment missing")?;
        if payment.receipt_sent_at.is_some() || payment.fulfilled_at.is_none() {
            return Ok(());
        }
        payment.receipt_attempts += 1;
        self.paypal_repo.update_payment(&mut txn, &payment).await?;
        txn.commit().await?;
        // A row lock prevents simultaneous receipt workers. SMTP remains at-least-once if its
        // acceptance response or the acknowledgement COMMIT is lost; monetary fulfillment is once.
        let mut txn = self.db.begin_transaction().await?;
        payment = self
            .paypal_repo
            .get_payment(&mut txn, id)
            .await?
            .context("Payment missing")?;
        if payment.receipt_sent_at.is_some() {
            return Ok(());
        }
        let pdf = self
            .finance_invoice
            .render_payment_invoice(&mut txn, &payment)
            .await?;
        // Freeze invoice bytes before any SMTP side effect. Reacquire the
        // payment lock to preserve receipt-worker serialization.
        txn.commit().await?;
        let mut txn = self.db.begin_transaction().await?;
        payment = self
            .paypal_repo
            .get_payment(&mut txn, id)
            .await?
            .context("Payment missing")?;
        if payment.receipt_sent_at.is_some() {
            return Ok(());
        }
        anyhow::ensure!(
            self.purchase.cash_receipt(payment.clone(), pdf).await?,
            "SMTP did not accept invoice receipt"
        );
        payment.receipt_sent_at = Some(Utc::now());
        payment.last_error = None;
        self.paypal_repo.update_payment(&mut txn, &payment).await?;
        txn.commit().await
    }
}
