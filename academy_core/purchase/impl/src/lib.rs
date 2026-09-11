use academy_assets::email::{AGB_2026_09_R2_PDF, WIDERRUFSBELEHRUNG_2026_09_R1_PDF};
use academy_auth_contracts::{AuthService, internal::AuthInternalService};
use academy_core_heart_contracts::heart::HeartService;
use academy_core_heart_impl::HeartFeatureConfig;
use academy_core_premium_impl::{PremiumFeatureConfig, period::add_months};
use academy_core_purchase_contracts::{PurchaseError, PurchaseFeatureService};
use academy_di::Build;
use academy_email_contracts::{
    AttachmentContentType, ContentType, Email, EmailAttachment, EmailService,
};
use academy_models::{
    auth::{AccessToken, InternalToken},
    coin::Transaction as CoinTransaction,
    premium::Premium,
    purchase::*,
    user::UserId,
};
use academy_persistence_contracts::{
    Database, Transaction,
    coin::{CoinRepoAddCoinsError, CoinRepository},
    heart::HeartRepository,
    premium::PremiumRepository,
    purchase::PurchaseRepository,
    user::UserRepository,
};
use anyhow::Context;
use chrono::{TimeDelta, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PurchaseFeatureConfig {
    pub provision_window_seconds: std::collections::HashMap<String, u64>,
}

// An early-performance request is recorded; this release does not assert that
// unresolved product classification or a checkbox extinguishes withdrawal rights.
const DECLARATION: &str = "Ich verlange ausdrücklich, dass mit der bestellten Leistung vor Ablauf der Widerrufsfrist begonnen wird. Meine gesetzlichen Widerrufs- und Mängelrechte bleiben unberührt.";
const COMMENCEMENT: &str = "Premium beginnt erst nach Bereitstellung der Vertragsbestätigung, frühestens nach Ende eines bereits bezahlten Premium-Zeitraums. Der vollständig gekaufte Kalenderzeitraum wird ab diesem Beginn berechnet. Bestehender Zugang und eine gesonderte Verlängerungsvereinbarung bleiben erhalten. Bei ausstehender Bestätigung wird keine neue Leistung begonnen und keine Laufzeit verbraucht; die Bestellung und Ihre Ansprüche bleiben in der Bestellübersicht erhalten.";

#[derive(Debug, Clone, Build)]
pub struct PurchaseFeatureServiceImpl<
    Db,
    Auth,
    InternalAuth,
    UserRepo,
    CoinRepo,
    Heart,
    HeartRepo,
    PremiumRepo,
    PurchaseRepo,
    Mail,
> {
    db: Db,
    auth: Auth,
    internal_auth: InternalAuth,
    user_repo: UserRepo,
    coin_repo: CoinRepo,
    heart: Heart,
    heart_repo: HeartRepo,
    premium_repo: PremiumRepo,
    purchase_repo: PurchaseRepo,
    mail: Mail,
    premium_config: PremiumFeatureConfig,
    heart_config: HeartFeatureConfig,
    purchase_config: PurchaseFeatureConfig,
}

fn docs_hash() -> String {
    let mut hash = Sha256::new();
    for bytes in [AGB_2026_09_R2_PDF, WIDERRUFSBELEHRUNG_2026_09_R1_PDF] {
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}

impl<Db, Auth, InternalAuth, UserRepo, CoinRepo, Heart, HeartRepo, PremiumRepo, PurchaseRepo, Mail>
    PurchaseFeatureServiceImpl<
        Db,
        Auth,
        InternalAuth,
        UserRepo,
        CoinRepo,
        Heart,
        HeartRepo,
        PremiumRepo,
        PurchaseRepo,
        Mail,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    InternalAuth: AuthInternalService,
    UserRepo: UserRepository<Db::Transaction>,
    CoinRepo: CoinRepository<Db::Transaction>,
    Heart: HeartService<Db::Transaction>,
    HeartRepo: HeartRepository<Db::Transaction>,
    PremiumRepo: PremiumRepository<Db::Transaction>,
    PurchaseRepo: PurchaseRepository<Db::Transaction>,
    Mail: EmailService,
{
    async fn principal(&self, token: &AccessToken) -> Result<UserId, PurchaseError> {
        let auth = self
            .auth
            .authenticate(token)
            .await
            .map_err(|_| PurchaseError::NotFound)?;
        Ok(auth.user_id)
    }
    fn internal(&self, token: &InternalToken) -> Result<(), PurchaseError> {
        self.internal_auth
            .authenticate(token, "shop")
            .map_err(|_| PurchaseError::NotFound)
    }
    fn builtin(&self, kind: &str) -> Result<PurchaseProduct, PurchaseError> {
        let (title,description,coins,facts)=match kind {
            "premium_monthly"=>("Premium für einen Kalendermonat",format!("Zugriff auf alle Kurse und Übungen ohne Verbrauch von Herzen; Webinare und Coachings sind nicht enthalten. {COMMENCEMENT}"),self.premium_config.monthly_price,json!({"months":1,"automatic_renewal":false})),
            "premium_yearly"=>("Premium für zwölf Kalendermonate",format!("Zugriff auf alle Kurse und Übungen ohne Verbrauch von Herzen; Webinare und Coachings sind nicht enthalten. {COMMENCEMENT}"),self.premium_config.yearly_price,json!({"months":12,"automatic_renewal":false})),
            "hearts"=>("Herzen auffüllen","Einmaliges Auffüllen des Herzbestands bis zur angegebenen Höchstzahl. Kein Abonnement; der automatische kostenlose tägliche Refill bleibt bestehen.".into(),self.heart_config.hearts_refill_price,json!({"maximum":self.heart_config.hearts_max,"unit":"half_heart"})),
            _=>return Err(PurchaseError::Unavailable),
        };
        Ok(PurchaseProduct {
            kind: kind.into(),
            reference: kind.into(),
            title: title.into(),
            description,
            coins,
            facts,
            revision: format!(
                "purchase-2026-09-v1:{coins}:{}",
                self.heart_config.hearts_max
            ),
            service_starts_at: None,
        })
    }
    async fn unresolved(
        &self,
        txn: &mut Db::Transaction,
        user: UserId,
        source: &str,
        product: &PurchaseProduct,
    ) -> Result<Option<PurchaseStatus>, PurchaseError> {
        // The shared account lock serializes both issuance and first acceptance,
        // including two tabs that acquired different quotes before either paid.
        // Other sources own their reservation/closure or provider-order recovery;
        // a cancelled event can legitimately be booked again with a new order.
        if source != "backend" {
            return Ok(None);
        }
        Ok(self
            .purchase_repo
            .list(txn, *user)
            .await?
            .into_iter()
            .find(|s| {
                s.offer.source == source
                    && matches!(
                        s.state.as_str(),
                        "accepted" | "awaiting_payment" | "paid" | "review"
                    )
                    && ((product.kind.starts_with("premium_")
                        && s.offer.product.kind.starts_with("premium_"))
                        || (s.offer.product.kind == product.kind
                            && s.offer.product.reference == product.reference))
            }))
    }
    async fn issue(
        &self,
        user: UserId,
        source: &str,
        mut product: PurchaseProduct,
    ) -> Result<PurchaseStatus, PurchaseError> {
        if product.title.is_empty()
            || product.title.len() > 512
            || product.description.len() > 32768
            || product.revision.len() > 512
            || product.reference.len() > 256
            || serde_json::to_vec(&product.facts)
                .context("Product serialization")?
                .len()
                > 65536
            || product.coins > i64::MAX as u64
        {
            return Err(PurchaseError::Unavailable);
        }
        let mut txn = self.db.begin_transaction().await?;
        if !self.purchase_repo.lock_user(&mut txn, *user).await? {
            return Err(PurchaseError::NotFound);
        }
        if let Some(existing) = self.unresolved(&mut txn, user, source, &product).await? {
            txn.commit().await?;
            return Ok(existing);
        }
        let provision_window_seconds = if product.service_starts_at.is_none() {
            Some(
                *self
                    .purchase_config
                    .provision_window_seconds
                    .get(&product.kind)
                    .ok_or(PurchaseError::Unavailable)?,
            )
        } else {
            None
        };
        let account = self
            .user_repo
            .get_purchase_composite(&mut txn, user)
            .await?
            .ok_or(PurchaseError::NotFound)?;
        if !account.user.email_verified {
            return Err(PurchaseError::ContactRequired);
        }
        let recipient = account
            .user
            .email
            .ok_or(PurchaseError::ContactRequired)?
            .with_name(account.profile.display_name.to_string())
            .0
            .to_string();
        if product.kind == "hearts" {
            let current = self.heart.get(&mut txn, user).await?;
            let units = self.heart_config.hearts_max.saturating_sub(current.hearts);
            if units == 0 {
                return Err(PurchaseError::Unavailable);
            }
            product.facts["refill_units"] = json!(units);
            product.description = format!(
                "Einmaliger Kauf von {} zusätzlichen Herzen, höchstens {} Herzen insgesamt. Ein Lösungsversuch kostet je nach Aufgabentyp ein halbes oder ein ganzes Herz, auch bei richtiger Lösung; die bestellte Menge ist keine feste Anzahl von Versuchen. Bereitstellung erst nach Vertragsbestätigung. Falls die automatische kostenlose Auffüllung die bestellte Menge inzwischen unmöglich macht, bleibt die bezahlte Bestellung zur Klärung offen. Kein Abonnement. Der automatische kostenlose tägliche Refill bleibt bestehen.",
                display_hearts(units),
                display_hearts(self.heart_config.hearts_max)
            );
        }
        let now = Utc::now();
        let expires_at = product
            .service_starts_at
            .map_or(now + TimeDelta::minutes(20), |at| {
                at.min(now + TimeDelta::minutes(20))
            });
        if expires_at <= now {
            return Err(PurchaseError::Unavailable);
        }
        let mut text = if source == "paypal" {
            format!(
                "Anbieter: bootstrap academy GmbH.\n{}\n{}\nGesamtpreis: {} EUR einschließlich Umsatzsteuer ({} EUR). Zahlung mit PayPal. Keine weiteren Bestellkosten. Guthaben wird erst nach nachgewiesenem Zahlungseingang und Vertragsbestätigung bereitgestellt.\nVertragssprache: Deutsch. Die beigefügten konkreten AGB und Widerrufsinformationen gelten für diese Bestellung.",
                product.title,
                product.description,
                product.facts["gross_total"].as_str().unwrap_or(""),
                product.facts["vat_total"].as_str().unwrap_or("")
            )
        } else {
            format!(
                "Anbieter: bootstrap academy GmbH. Vertragspartner und Beschwerdekontakt wie in den beigefügten AGB.\nBestellte Leistung: {}\n{}\nPreis: {} MorphCoins ({:.2} EUR einschließlich Umsatzsteuer zum Verhältnis 100 MorphCoins = 1 EUR); keine weiteren Bestellkosten. Zahlung ausschließlich aus vorhandenem MorphCoin-Guthaben.\nVertrags- und Erklärungssprache: Deutsch. Die angehängten AGB und Widerrufsinformationen gehören zu diesem konkreten Angebot; andere bestehende Verträge werden nicht geändert.\nWiderruf: https://bootstrap.academy/vertrag-widerrufen. Kündigung: https://bootstrap.academy/vertrag-kuendigen. E-Mail für beide Erklärungen: hallo@bootstrap.academy.\n",
                product.title,
                product.description,
                product.coins,
                product.coins as f64 / 100.0
            )
        };
        if let Some(seconds) = provision_window_seconds {
            text.push_str(&format!("\nVertragsbestätigung und Bereitstellung innerhalb von {seconds} Sekunden ab Eingang Ihrer wirksamen Bestellung. Bei Premium wird innerhalb dieser Frist der volle gekaufte Zeitraum zugeordnet; ein bereits bezahlter Zeitraum bleibt davor erhalten. Nach Fristablauf wird eine noch ausstehende Bereitstellung nicht automatisch nachgeholt; die Bestellung, tatsächliche Zahlung und Ihre Ansprüche bleiben zur Klärung in der Bestellübersicht erhalten. Für Fragen: hallo@bootstrap.academy. Ein ausstehender Zahlungseingang verlängert diese Frist nicht."));
        }
        let mut offer = PurchaseOffer {
            id: Uuid::new_v4(),
            user_id: *user,
            source: source.into(),
            created_at: now,
            expires_at,
            recipient,
            product,
            document_hash: docs_hash(),
            hash: String::new(),
            text,
            declaration: DECLARATION.into(),
            provision_window_seconds,
        };
        offer.hash = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&offer).context("Offer serialization")?)
        );
        let record = PurchaseRecord {
            status: PurchaseStatus {
                offer,
                state: "offered".into(),
                accepted_at: None,
                confirmation_smtp_accepted_at: None,
                fulfillment: None,
                financial_evidence: None,
                review_reason: None,
                provision_deadline: None,
                provision_timing: None,
                document_corrections: Vec::new(),
            },
            terms_pdf: AGB_2026_09_R2_PDF.to_vec(),
            withdrawal_pdf: WIDERRUFSBELEHRUNG_2026_09_R1_PDF.to_vec(),
            confirmation_body: None,
            delivery_generation: 0,
            submission: None,
            message_metadata: None,
        };
        self.purchase_repo.create(&mut txn, &record).await?;
        txn.commit().await?;
        Ok(record.status)
    }
    async fn owned(&self, user: UserId, id: Uuid) -> Result<PurchaseRecord, PurchaseError> {
        let mut txn = self.db.begin_transaction().await?;
        let record = self
            .purchase_repo
            .get(&mut txn, id)
            .await?
            .filter(|r| r.status.offer.user_id == *user)
            .ok_or(PurchaseError::NotFound)?;
        txn.commit().await?;
        Ok(record)
    }
    async fn accept_for(
        &self,
        user: UserId,
        source: &str,
        a: PurchaseAcceptance,
    ) -> Result<PurchaseStatus, PurchaseError> {
        let mut txn = self.db.begin_transaction().await?;
        let exists = self.purchase_repo.lock_user(&mut txn, *user).await?;
        let record = self
            .purchase_repo
            .get(&mut txn, a.order_id)
            .await?
            .ok_or(PurchaseError::NotFound)?;
        let o = &record.status.offer;
        if o.user_id != *user || o.source != source {
            return Err(PurchaseError::NotFound);
        }
        if o.hash != a.offer_hash
            || !a.accepted
            || (o.product.coins > 0 && !a.early_performance_requested)
        {
            return Err(PurchaseError::OfferRequired);
        }
        if record.submission.as_ref().is_some_and(|old| old != &a) {
            return Err(PurchaseError::OfferRequired);
        }
        // Terminal rejections and accepted orders both replay independently of current catalog.
        if record.status.state != "offered" {
            txn.commit().await?;
            if matches!(record.status.state.as_str(), "paid" | "awaiting_payment")
                && record
                    .status
                    .provision_deadline()
                    .is_some_and(|d| Utc::now() >= d)
            {
                // Original-order recovery also exposes an overdue obligation
                // without waiting for the independent background interval.
                self.process(o.id).await?;
                return Ok(self.owned(user, o.id).await?.status);
            }
            return Ok(record.status);
        }
        if !exists {
            return Err(PurchaseError::NotFound);
        }
        self.purchase_repo
            .submit(
                &mut txn,
                o.id,
                &serde_json::to_string(&a).context("Acceptance serialization")?,
            )
            .await?;
        if let Some(existing) = self.unresolved(&mut txn, user, source, &o.product).await? {
            self.purchase_repo
                .state(
                    &mut txn,
                    o.id,
                    "failed",
                    Some(&format!(
                        "No charge: recover unresolved original order {} before another purchase",
                        existing.offer.id
                    )),
                )
                .await?;
            txn.commit().await?;
            return Ok(self.owned(user, o.id).await?.status);
        }
        let mut expected = if source == "backend" {
            Some(self.builtin(&o.product.kind)?)
        } else {
            None
        };
        if let Some(ref mut expected) = expected
            && o.product.kind == "hearts"
        {
            expected.facts["refill_units"] = o.product.facts["refill_units"].clone();
            expected.description = o.product.description.clone();
        }
        if o.expires_at <= Utc::now()
            || o.document_hash != docs_hash()
            || expected.is_some_and(|v| v != o.product)
        {
            self.purchase_repo
                .state(
                    &mut txn,
                    o.id,
                    "failed",
                    Some("Offer expired or changed; no charge"),
                )
                .await?;
            txn.commit().await?;
            return Ok(self.owned(user, o.id).await?.status);
        }
        let account = self
            .user_repo
            .get_purchase_composite(&mut txn, user)
            .await?
            .ok_or(PurchaseError::NotFound)?;
        if !account.user.email_verified
            || account
                .user
                .email
                .map(|e| {
                    e.with_name(account.profile.display_name.to_string())
                        .0
                        .to_string()
                })
                .as_deref()
                != Some(&o.recipient)
        {
            self.purchase_repo
                .state(
                    &mut txn,
                    o.id,
                    "failed",
                    Some("Verified recipient changed; no charge"),
                )
                .await?;
            txn.commit().await?;
            return Ok(self.owned(user, o.id).await?.status);
        }
        let hearts = if o.product.kind == "hearts" {
            let current = self.heart.get(&mut txn, user).await?;
            if current.hearts as u64
                + o.product.facts["refill_units"]
                    .as_u64()
                    .context("Missing refill quantity")?
                > self.heart_config.hearts_max
            {
                self.purchase_repo
                    .state(
                        &mut txn,
                        o.id,
                        "failed",
                        Some("No refill necessary; no charge"),
                    )
                    .await?;
                txn.commit().await?;
                return Ok(self.owned(user, o.id).await?.status);
            }
            Some(current)
        } else {
            None
        };
        let accepted_at = self.purchase_repo.timestamp(&mut txn).await?;
        let deadline = o
            .provision_window_seconds
            .and_then(|s| accepted_at.checked_add_signed(TimeDelta::seconds(s as i64)))
            .or(o.product.service_starts_at);
        let body = format!(
            "Vertragsbestätigung – Bootstrap Academy\nBestellung: {}\nEmpfänger: {}\nErklärung eingegangen: {}\n\n{}\nAusdrücklich erklärte Anforderung:\n{}\n\nDer Zahlungsweg und Preis sind oben festgehalten. Die Leistungsbereitstellung ist in der Bestellübersicht dokumentiert. Diese Bestätigung behauptet keine bereits vollständige Leistung und kein vorzeitiges Erlöschen des Widerrufsrechts.\nDie vereinbarten AGB und Widerrufsinformationen sind als unveränderliche PDF-Kopien beigefügt.",
            o.id,
            o.recipient,
            accepted_at,
            if source == "paypal" {
                format!(
                    "{}\nZahlungsweg: PayPal; keine Abbuchung vorhandener MorphCoins.",
                    o.text
                )
            } else {
                o.text.clone()
            },
            if a.early_performance_requested {
                &o.declaration
            } else {
                "Keine Anforderung vorzeitiger Leistung erklärt (kostenfreie Bestellung)."
            }
        );
        let body = if let Some(deadline) = deadline {
            format!(
                "{body}\nVereinbarte Frist für Vertragsbestätigung und Bereitstellung: {deadline} (UTC). Bei einer noch nicht erbrachten Bestellung bleiben nach Fristablauf Zahlung und Ansprüche zur Klärung erhalten; es erfolgt keine automatische verspätete Bereitstellung."
            )
        } else {
            body
        };
        let metadata = json!({"sender":self.mail.sender().map(|v|v.0.to_string()),"message_id":format!("purchase-{}@bootstrap.academy",o.id),"subject":"Ihre Vertragsbestätigung – Bootstrap Academy","content_type":"text/plain; charset=utf-8","terms_filename":"vereinbarte-agb.pdf","withdrawal_filename":"vereinbarte-widerrufsinformation.pdf","attachment_content_type":"application/pdf"});
        if o.product.coins > 0 && source != "paypal" {
            match self
                .coin_repo
                .add_coins(&mut txn, user, -(o.product.coins as i64), false)
                .await
            {
                Ok(_) => {}
                Err(CoinRepoAddCoinsError::NotEnoughCoins) => {
                    self.purchase_repo
                        .state(
                            &mut txn,
                            o.id,
                            "failed",
                            Some("Not enough coins; order rejected without charge"),
                        )
                        .await?;
                    txn.commit().await?;
                    return Ok(self.owned(user, o.id).await?.status);
                }
                Err(CoinRepoAddCoinsError::Other(e)) => return Err(e.into()),
            }
            self.purchase_repo
                .accept(&mut txn, o.id, &body, &metadata.to_string(), accepted_at)
                .await?;
            self.coin_repo
                .create_transaction(
                    &mut txn,
                    &CoinTransaction {
                        id: o.id.into(),
                        user_id: user,
                        coins: -(o.product.coins as i64),
                        description: Some(
                            if o.product.kind.starts_with("premium_") {
                                "Premium".to_owned()
                            } else {
                                format!("Purchase {}", o.id)
                            }
                            .try_into()
                            .context("Transaction description")?,
                        ),
                        created_at: Utc::now(),
                        include_in_credit_note: false,
                    },
                )
                .await?;
        }
        if o.product.coins == 0 || source == "paypal" {
            self.purchase_repo
                .accept(&mut txn, o.id, &body, &metadata.to_string(), accepted_at)
                .await?;
        }
        self.purchase_repo
            .state(
                &mut txn,
                o.id,
                if source == "paypal" {
                    "awaiting_payment"
                } else {
                    "paid"
                },
                None,
            )
            .await?;
        if source != "paypal" {
            self.purchase_repo.record_debit(&mut txn, o.id).await?;
        }
        let _ = hearts; // Quantity was checked once under the account lock; fulfillment is gated below.
        txn.commit().await?;
        // The worker owns recovery. Failure never reports a committed debit as absent.
        if let Err(e) = self.process(o.id).await {
            tracing::warn!(order_id=%o.id,error=%e,"Purchase remains recoverable")
        }
        Ok(self.owned(user, o.id).await?.status)
    }
    async fn observe_provision(&self, id: Uuid) -> anyhow::Result<()> {
        let mut txn = self.db.begin_transaction().await?;
        self.purchase_repo.observe_provision(&mut txn, id).await?;
        txn.commit().await
    }
    async fn process(&self, id: Uuid) -> anyhow::Result<()> {
        self.observe_provision(id).await?;
        let mut txn = self.db.begin_transaction().await?;
        let claimed = self.purchase_repo.claim(&mut txn, id).await?;
        txn.commit().await?;
        if let Some(record) = claimed {
            let o = &record.status.offer;
            let metadata = record
                .message_metadata
                .as_ref()
                .context("Missing saved message metadata")?;
            let field = |key: &str| {
                metadata[key]
                    .as_str()
                    .map(str::to_owned)
                    .context("Missing saved message field")
            };
            anyhow::ensure!(
                field("content_type")? == "text/plain; charset=utf-8"
                    && field("attachment_content_type")? == "application/pdf",
                "Unsupported saved message MIME; retain exact artifact for review"
            );
            let attempt = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                self.mail.send(Email {
                    sender: Some(field("sender")?.parse()?),
                    message_id: Some(field("message_id")?),
                    recipient: o.recipient.parse()?,
                    subject: field("subject")?,
                    body: record
                        .confirmation_body
                        .context("Accepted purchase has no confirmation")?,
                    content_type: ContentType::Text,
                    reply_to: None,
                    attachments: vec![
                        EmailAttachment {
                            filename: field("terms_filename")?,
                            content_type: AttachmentContentType::Pdf,
                            content: record.terms_pdf,
                        },
                        EmailAttachment {
                            filename: field("withdrawal_filename")?,
                            content_type: AttachmentContentType::Pdf,
                            content: record.withdrawal_pdf,
                        },
                    ],
                }),
            )
            .await;
            let outcome = match attempt {
                Ok(Ok(true)) => "smtp_accepted",
                Ok(Ok(false)) => "definite_rejection",
                Ok(Err(_)) | Err(_) => "handoff_uncertain",
            };
            let mut txn = self.db.begin_transaction().await?;
            self.purchase_repo
                .acknowledge(&mut txn, id, record.delivery_generation, outcome)
                .await?;
            txn.commit().await?;
        }
        let mut txn = self.db.begin_transaction().await?;
        // Read owner without a held order lock, then acquire the shared account lock first.
        let Some(initial) = self.purchase_repo.get(&mut txn, id).await? else {
            return Ok(());
        };
        let user = initial.status.offer.user_id;
        txn.commit().await?;
        let mut txn = self.db.begin_transaction().await?;
        let exists = self.purchase_repo.lock_user(&mut txn, user).await?;
        let record = self
            .purchase_repo
            .get(&mut txn, id)
            .await?
            .context("Purchase missing")?;
        if record.status.state != "paid" {
            return Ok(());
        }
        let o = &record.status.offer;
        if !exists {
            self.purchase_repo
                .state(
                    &mut txn,
                    id,
                    "review",
                    Some("Account deleted; preserve unperformed purchase and settlement claim"),
                )
                .await?;
            return txn.commit().await;
        }
        if record
            .status
            .provision_deadline()
            .is_some_and(|t| t <= Utc::now())
        {
            self.purchase_repo.state(&mut txn,id,"review",Some("Service deadline passed without confirmed provision; no performance or payout inferred")).await?;
            return txn.commit().await;
        }
        if record.status.confirmation_smtp_accepted_at.is_none() {
            return Ok(());
        }
        if o.product.kind.starts_with("premium_") {
            let now = Utc::now();
            let latest = self
                .premium_repo
                .get_latest_by_user_id(&mut txn, user.into())
                .await?;
            let months = o.product.facts["months"]
                .as_u64()
                .context("Missing agreed period")? as u32;
            if !self.purchase_repo.bind_period(&mut txn, id).await? {
                return txn.commit().await;
            }
            let (premium, start) = if let Some(mut p) = latest.filter(|p| p.until > now) {
                let start = p.until;
                p.until = add_months(start, months);
                self.premium_repo.extend(&mut txn, p.id, p.until).await?;
                (p, start)
            } else {
                let p = Premium {
                    id: id.into(),
                    user_id: user.into(),
                    since: now,
                    until: add_months(now, months),
                };
                self.premium_repo.create(&mut txn, p).await?;
                (p, now)
            };
            self.purchase_repo.fulfill(&mut txn,id,&json!({"period_id":*premium.id,"purchased_since":start,"purchased_until":premium.until,"ledger_id":if o.product.coins>0 {Some(id)} else {None},"automatic_renewal_changed":false}).to_string()).await?;
        }
        if o.product.kind == "hearts" {
            let mut hearts = self.heart.get(&mut txn, user.into()).await?;
            let units = o.product.facts["refill_units"]
                .as_u64()
                .context("Missing agreed refill")?;
            let maximum = o.product.facts["maximum"]
                .as_u64()
                .context("Missing agreed cap")?;
            let before = hearts.hearts;
            if before as u64 + units > maximum {
                self.purchase_repo.state(&mut txn,id,"review",Some("Accepted refill quantity no longer fits after free refill; payment and unperformed claim retained for review")).await?;
            } else {
                hearts.hearts = before as u64 + units;
                self.heart_repo.set(&mut txn, user.into(), hearts).await?;
                self.purchase_repo.fulfill(&mut txn,id,&json!({"hearts_before":before,"hearts_after":hearts.hearts,"added":units,"ledger_id":if o.product.coins>0 {Some(id)} else {None}}).to_string()).await?;
            }
        }
        txn.commit().await?;
        self.observe_provision(id).await
    }
}

impl<Db, Auth, InternalAuth, UserRepo, CoinRepo, Heart, HeartRepo, PremiumRepo, PurchaseRepo, Mail>
    PurchaseFeatureService
    for PurchaseFeatureServiceImpl<
        Db,
        Auth,
        InternalAuth,
        UserRepo,
        CoinRepo,
        Heart,
        HeartRepo,
        PremiumRepo,
        PurchaseRepo,
        Mail,
    >
where
    Db: Database,
    Auth: AuthService<Db::Transaction>,
    InternalAuth: AuthInternalService,
    UserRepo: UserRepository<Db::Transaction>,
    CoinRepo: CoinRepository<Db::Transaction>,
    Heart: HeartService<Db::Transaction>,
    HeartRepo: HeartRepository<Db::Transaction>,
    PremiumRepo: PremiumRepository<Db::Transaction>,
    PurchaseRepo: PurchaseRepository<Db::Transaction>,
    Mail: EmailService,
{
    async fn cash_offer(
        &self,
        token: &AccessToken,
        product: PurchaseProduct,
    ) -> Result<PurchaseStatus, PurchaseError> {
        if product.kind != "coins" {
            return Err(PurchaseError::Unavailable);
        }
        self.issue(self.principal(token).await?, "paypal", product)
            .await
    }
    async fn cash_accept(
        &self,
        token: &AccessToken,
        acceptance: PurchaseAcceptance,
    ) -> Result<PurchaseStatus, PurchaseError> {
        self.accept_for(self.principal(token).await?, "paypal", acceptance)
            .await
    }
    async fn cash_captured(
        &self,
        payment: academy_models::paypal::PaypalPayment,
    ) -> Result<PurchaseStatus, PurchaseError> {
        let snapshot = &payment.snapshot;
        let id = snapshot
            .contract_order_id
            .ok_or(PurchaseError::Unavailable)?;
        let capture = payment.capture.as_ref().ok_or(PurchaseError::Unavailable)?;
        let mut txn = self.db.begin_transaction().await?;
        let r = self
            .purchase_repo
            .get(&mut txn, id)
            .await?
            .ok_or(PurchaseError::NotFound)?;
        if r.status.offer.source != "paypal"
            || r.status.offer.user_id != *snapshot.order.user_id
            || r.status.accepted_at.is_none()
            || r.status.offer.product.coins != snapshot.order.coins
            || r.status.offer.product.facts["gross_total"].as_str()
                != Some(&snapshot.gross_total.to_string())
            || capture.amount != snapshot.gross_total
            || capture.currency != snapshot.currency
            || capture.status != "COMPLETED"
        {
            return Err(PurchaseError::OfferRequired);
        }
        self.purchase_repo.cash_capture(&mut txn,id,&json!({"kind":"paypal_capture","paypal_order_id":snapshot.order.id,"capture":capture,"invoice_number":snapshot.order.invoice_number,"credited_units":snapshot.order.coins,"wallet_fulfillment":"not_implied_by_capture"}).to_string()).await?;
        if r.status.state == "awaiting_payment" {
            self.purchase_repo.state(&mut txn, id, "paid", None).await?;
        }
        txn.commit().await?;
        if let Err(error) = self.process(id).await {
            tracing::warn!(order_id=%id,%error,"Captured payment confirmation pending");
        }
        Ok(self.owned(snapshot.order.user_id, id).await?.status)
    }
    async fn cash_fulfilled(
        &self,
        payment: academy_models::paypal::PaypalPayment,
    ) -> Result<PurchaseStatus, PurchaseError> {
        let id = payment
            .snapshot
            .contract_order_id
            .ok_or(PurchaseError::Unavailable)?;
        let mut txn = self.db.begin_transaction().await?;
        let r = self
            .purchase_repo
            .get(&mut txn, id)
            .await?
            .ok_or(PurchaseError::NotFound)?;
        if r.status.offer.source != "paypal"
            || r.status.offer.user_id != *payment.snapshot.order.user_id
            || payment.balance.is_none()
            || payment.fulfilled_at.is_none()
            || r.status.confirmation_smtp_accepted_at.is_none()
        {
            return Err(PurchaseError::Unavailable);
        }
        let result = json!({"kind":"coin_balance_provided","paypal_order_id":payment.snapshot.order.id,"provided_at":payment.fulfilled_at,"balance":payment.balance.map(|v|json!({"coins":v.coins,"withheld_coins":v.withheld_coins})),"capture":payment.capture});
        if let Some(old) = r.status.fulfillment {
            if old != result {
                return Err(PurchaseError::OfferRequired);
            }
        } else {
            self.purchase_repo
                .fulfill(&mut txn, id, &result.to_string())
                .await?;
            if r.status.state == "review" {
                self.purchase_repo
                    .state(&mut txn, id, "review", r.status.review_reason.as_deref())
                    .await?;
            }
        }
        txn.commit().await?;
        self.observe_provision(id).await?;
        Ok(self.owned(payment.snapshot.order.user_id, id).await?.status)
    }
    async fn cash_receipt(
        &self,
        payment: academy_models::paypal::PaypalPayment,
        invoice: Vec<u8>,
    ) -> anyhow::Result<bool> {
        let mut attachments = vec![
            json!({"filename":format!("Rechnung-R{:07}.pdf",payment.snapshot.order.invoice_number),"bytes":invoice}),
        ];
        let (body, metadata) = if let Some(id) = payment.snapshot.contract_order_id {
            let r = self.owned(payment.snapshot.order.user_id, id).await?;
            attachments.push(json!({"filename":r.message_metadata.as_ref().context("Missing metadata")?["terms_filename"],"bytes":r.terms_pdf}));
            attachments.push(json!({"filename":r.message_metadata.as_ref().context("Missing metadata")?["withdrawal_filename"],"bytes":r.withdrawal_pdf}));
            (
                r.confirmation_body.context("Missing accepted body")?,
                r.message_metadata.context("Missing metadata")?,
            )
        } else {
            (
                format!(
                    "Anbei die Rechnung R{:07} zu PayPal-Bestellung {}. Diese Übersendung belegt keine historischen Vertragserklärungen oder damaligen Dokumentversionen. Es werden keine aktuellen Rechtstexte als früher vereinbart dargestellt.",
                    payment.snapshot.order.invoice_number,
                    payment.snapshot.order.id.as_str()
                ),
                json!({"sender":self.mail.sender().map(|s|s.0.to_string()),"subject":"Ihre Rechnung – Bootstrap Academy","content_type":"text/plain; charset=utf-8","attachment_content_type":"application/pdf"}),
            )
        };
        let candidate = json!({"body":body,"metadata":metadata,"recipient":payment.snapshot.recipient.0.to_string(),"message_id":format!("invoice-{}@bootstrap.academy",payment.snapshot.order.id.as_str()),"attachments":attachments});
        let mut txn = self.db.begin_transaction().await?;
        let artifact: serde_json::Value = serde_json::from_str(
            &self
                .purchase_repo
                .invoice_artifact(&mut txn, &payment.snapshot.order.id, &candidate.to_string())
                .await?,
        )?;
        let attempt = Uuid::new_v4();
        self.purchase_repo
            .invoice_attempt(&mut txn, &payment.snapshot.order.id, attempt, "started")
            .await?;
        txn.commit().await?;
        let field = |key: &str| {
            artifact["metadata"][key]
                .as_str()
                .context("Missing saved invoice metadata")
        };
        anyhow::ensure!(
            field("content_type")? == "text/plain; charset=utf-8"
                && field("attachment_content_type")? == "application/pdf",
            "Unsupported saved invoice MIME"
        );
        let email = Email {
            sender: Some(field("sender")?.parse()?),
            recipient: artifact["recipient"]
                .as_str()
                .context("Missing recipient")?
                .parse()?,
            subject: field("subject")?.into(),
            body: artifact["body"].as_str().context("Missing body")?.into(),
            message_id: Some(
                artifact["message_id"]
                    .as_str()
                    .context("Missing identity")?
                    .into(),
            ),
            reply_to: None,
            content_type: ContentType::Text,
            attachments: artifact["attachments"]
                .as_array()
                .context("Missing attachments")?
                .iter()
                .map(|a| {
                    Ok(EmailAttachment {
                        filename: a["filename"].as_str().context("Missing filename")?.into(),
                        content_type: AttachmentContentType::Pdf,
                        content: serde_json::from_value(a["bytes"].clone())?,
                    })
                })
                .collect::<anyhow::Result<_>>()?,
        };
        let outcome =
            match tokio::time::timeout(std::time::Duration::from_secs(30), self.mail.send(email))
                .await
            {
                Ok(Ok(true)) => "smtp_accepted",
                Ok(Ok(false)) => "definite_rejection",
                _ => "handoff_uncertain",
            };
        let mut txn = self.db.begin_transaction().await?;
        self.purchase_repo
            .invoice_attempt(&mut txn, &payment.snapshot.order.id, attempt, outcome)
            .await?;
        txn.commit().await?;
        Ok(outcome == "smtp_accepted")
    }
    async fn retained_offer(&self, user: UserId, kind: &str) -> Result<PurchaseStatus, PurchaseError> {
        self.issue(user, "backend", self.builtin(kind)?).await
    }
    async fn retained_accept(&self, user: UserId, acceptance: PurchaseAcceptance) -> Result<PurchaseStatus, PurchaseError> {
        self.accept_for(user, "backend", acceptance).await
    }
    async fn retained_get(&self, user: UserId, id: Uuid) -> Result<PurchaseStatus, PurchaseError> {
        Ok(self.owned(user, id).await?.status)
    }
    async fn retained_resources(&self, user: UserId) -> Result<serde_json::Value, PurchaseError> {
        let mut txn = self.db.begin_transaction().await?;
        if !self.purchase_repo.lock_user(&mut txn, *user).await? {
            return Err(PurchaseError::Unavailable);
        }
        let premium = self.premium_repo.get_latest_by_user_id(&mut txn, user).await?;
        let hearts = self.heart.get(&mut txn, user).await?;
        let balance = self.coin_repo.get_balance(&mut txn, user).await?;
        let now = Utc::now();
        let result = json!({
            "subject":user,"purpose":"retained_learning","ordinary_authority":false,
            "coins":balance.coins,"withheld_coins":balance.withheld_coins,
            "hearts":hearts.hearts,"hearts_max":self.heart_config.hearts_max,
            "last_free_refill":hearts.last_refill,
            "premium":premium.map(|p|json!({"period_id":p.id,"since":p.since,"until":p.until,
                "active":p.since<=now && now<p.until})),
            "renewal_activated":false,"purchase_performed":false
        });
        txn.commit().await?;
        Ok(result)
    }
    async fn offer(
        &self,
        token: &AccessToken,
        kind: &str,
    ) -> Result<PurchaseStatus, PurchaseError> {
        self.issue(self.principal(token).await?, "backend", self.builtin(kind)?)
            .await
    }
    async fn external_offer(
        &self,
        token: &InternalToken,
        user: UserId,
        source: &str,
        product: PurchaseProduct,
    ) -> Result<PurchaseStatus, PurchaseError> {
        self.internal(token)?;
        if !matches!(
            (source, product.kind.as_str()),
            ("skills", "course") | ("events", "webinar" | "coaching")
        ) {
            return Err(PurchaseError::Unavailable);
        }
        self.issue(user, source, product).await
    }
    async fn accept(
        &self,
        token: &AccessToken,
        acceptance: PurchaseAcceptance,
    ) -> Result<PurchaseStatus, PurchaseError> {
        self.accept_for(self.principal(token).await?, "backend", acceptance)
            .await
    }
    async fn external_accept(
        &self,
        token: &InternalToken,
        user: UserId,
        source: &str,
        acceptance: PurchaseAcceptance,
    ) -> Result<PurchaseStatus, PurchaseError> {
        self.internal(token)?;
        if !matches!(source, "skills" | "events") {
            return Err(PurchaseError::Unavailable);
        }
        self.accept_for(user, source, acceptance).await
    }
    async fn get(&self, token: &AccessToken, id: Uuid) -> Result<PurchaseStatus, PurchaseError> {
        Ok(self.owned(self.principal(token).await?, id).await?.status)
    }
    async fn external_get(
        &self,
        token: &InternalToken,
        user: UserId,
        id: Uuid,
    ) -> Result<PurchaseStatus, PurchaseError> {
        self.internal(token)?;
        Ok(self.owned(user, id).await?.status)
    }
    async fn external_complete(
        &self,
        token: &InternalToken,
        user: UserId,
        source: &str,
        id: Uuid,
        result: serde_json::Value,
    ) -> Result<PurchaseStatus, PurchaseError> {
        self.internal(token)?;
        if !matches!(source, "skills" | "events") || result.to_string().len() > 65536 {
            return Err(PurchaseError::Unavailable);
        }
        let mut txn = self.db.begin_transaction().await?;
        let record = self
            .purchase_repo
            .get(&mut txn, id)
            .await?
            .ok_or(PurchaseError::NotFound)?;
        if record.status.offer.user_id != *user || record.status.offer.source != source {
            return Err(PurchaseError::NotFound);
        }
        if let Some(old) = &record.status.fulfillment {
            if old != &result {
                return Err(PurchaseError::OfferRequired);
            }
        } else {
            if !matches!(record.status.state.as_str(), "paid" | "review")
                || record.status.financial_evidence.is_none()
            {
                return Err(PurchaseError::Unavailable);
            }
            let provided = result["provided_at"]
                .as_str()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .ok_or(PurchaseError::Unavailable)?
                .with_timezone(&Utc);
            let observed = result["confirmation_smtp_accepted_at"]
                .as_str()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .ok_or(PurchaseError::Unavailable)?
                .with_timezone(&Utc);
            if Some(observed) != record.status.confirmation_smtp_accepted_at
                || result["order_id"].as_str() != Some(&id.to_string())
                || !matches!(
                    (source, result["kind"].as_str()),
                    ("skills", Some("course_access_provided"))
                        | ("events", Some("booking_access_provided"))
                )
            {
                return Err(PurchaseError::OfferRequired);
            }
            if source == "events"
                && record.status.offer.product.facts["availability_protocol"]
                    == "committed_candidate_v1"
            {
                let candidate = &result["candidate"];
                let mut canonical = candidate.clone();
                canonical.sort_all_objects();
                let hash = format!(
                    "{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&canonical).context("Candidate serialization")?
                    )
                );
                let product = &record.status.offer.product;
                if result["timing_basis"] != "committed_candidate_v1"
                    || result["candidate_hash"].as_str() != Some(&hash)
                    || result["provided_at"] != result["availability_observed_at"]
                    || candidate["order_id"] != result["order_id"]
                    || candidate["user_id"].as_str() != Some(&user.to_string())
                    || candidate["offer_hash"] != record.status.offer.hash
                    || candidate["event_id"] != product.reference
                    || candidate["event_kind"] != product.kind
                    || candidate["scheduled_start"] != product.facts["start"]
                    || candidate["scheduled_end"] != product.facts["end"]
                    || candidate["confirmation_smtp_accepted_at"]
                        != result["confirmation_smtp_accepted_at"]
                    || candidate["paid_coins"] != product.coins
                    || candidate["ledger_id"]
                        != record.status.financial_evidence.as_ref().unwrap()["ledger_id"]
                    || provided < observed
                    || product.service_starts_at.is_none_or(|t| provided >= t)
                {
                    return Err(PurchaseError::OfferRequired);
                }
            }
            self.purchase_repo
                .fulfill(&mut txn, id, &result.to_string())
                .await?;
            // A delayed report is evidence of the source action, not authority
            // to erase an account-deletion/deadline review or recreate access.
            if record.status.state == "review" {
                self.purchase_repo
                    .state(
                        &mut txn,
                        id,
                        "review",
                        record.status.review_reason.as_deref(),
                    )
                    .await?;
            } else if record
                .status
                .provision_deadline()
                .is_some_and(|cutoff| observed >= cutoff || provided >= cutoff)
            {
                self.purchase_repo.state(&mut txn,id,"review",Some("Source provision report missed accepted service deadline; actual report retained without inferring performance")).await?;
            }
        }
        txn.commit().await?;
        self.observe_provision(id).await?;
        Ok(self.owned(user, id).await?.status)
    }
    async fn document(
        &self,
        token: &AccessToken,
        id: Uuid,
        kind: &str,
    ) -> Result<Vec<u8>, PurchaseError> {
        self.recipient_document(self.principal(token).await?, id, kind)
            .await
    }
    async fn recipient_document(
        &self,
        user: UserId,
        id: Uuid,
        kind: &str,
    ) -> Result<Vec<u8>, PurchaseError> {
        let r = self.owned(user, id).await?;
        match kind {
            "timing" | "timing-original" => {
                let mut txn = self.db.begin_transaction().await?;
                self.purchase_repo
                    .timing_statement(&mut txn, id, kind == "timing-original")
                    .await?
                    .ok_or(PurchaseError::Unavailable)
            }
            "fulfillment" | "fulfillment-original" => {
                let mut txn = self.db.begin_transaction().await?;
                self.purchase_repo
                    .fulfillment_statement(&mut txn, id, kind == "fulfillment-original")
                    .await?
                    .ok_or(PurchaseError::Unavailable)
            }
            "terms" => Ok(r.terms_pdf),
            "withdrawal" => Ok(r.withdrawal_pdf),
            "confirmation" => r
                .confirmation_body
                .map(String::into_bytes)
                .ok_or(PurchaseError::Unavailable),
            _ => Err(PurchaseError::NotFound),
        }
    }
    async fn list(&self, token: &AccessToken) -> Result<Vec<PurchaseStatus>, PurchaseError> {
        let user = self.principal(token).await?;
        let mut txn = self.db.begin_transaction().await?;
        Ok(self.purchase_repo.list(&mut txn, *user).await?)
    }
    async fn retry(&self) -> anyhow::Result<()> {
        let mut txn = self.db.begin_transaction().await?;
        let ids = self.purchase_repo.pending(&mut txn).await?;
        txn.commit().await?;
        for id in ids {
            if let Err(e) = self.process(id).await {
                tracing::warn!(order_id=%id,error=%e,"Purchase recovery failed; obligation retained")
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod closing_document_tests;
