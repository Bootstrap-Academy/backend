//! Pure real-service controls; no database, payment, SMTP or heart operations.
use super::*;
use academy_assets::email::AGB_2026_09_R1_PDF;
use academy_auth_contracts::{MockAuthService, internal::MockAuthInternalService};
use academy_core_heart_contracts::heart::MockHeartService;
use academy_email_contracts::MockEmailService;
use academy_models::user::{User, UserComposite, UserDetails, UserInvoiceInfo, UserProfile};
use academy_persistence_contracts::{
    MockDatabase, MockTransaction, coin::MockCoinRepository, heart::MockHeartRepository,
    premium::MockPremiumRepository, user::MockUserRepository,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Saved {
    record: Option<PurchaseRecord>,
    calls: Vec<&'static str>,
    /// Confirmation mail body of an accepted order.
    body: Option<String>,
}
struct Repo {
    saved: Arc<Mutex<Saved>>,
    creating: bool,
    exists: bool,
    /// Lets an acceptance run through submission, state changes, debit and recovery.
    accepting: bool,
}
impl PurchaseRepository<MockTransaction> for Repo {
    async fn create(&self, _: &mut MockTransaction, record: &PurchaseRecord) -> anyhow::Result<()> {
        assert!(self.creating);
        let mut saved = self.saved.lock().unwrap();
        assert!(saved.record.is_none());
        saved.calls.push("create");
        saved.record = Some(record.clone());
        Ok(())
    }
    async fn get(
        &self,
        _: &mut MockTransaction,
        id: Uuid,
    ) -> anyhow::Result<Option<PurchaseRecord>> {
        assert!(!self.creating);
        let mut saved = self.saved.lock().unwrap();
        let record = saved.record.clone().unwrap();
        assert_eq!(id, record.status.offer.id);
        saved.calls.push("get");
        Ok(Some(record))
    }
    async fn lock_user(&self, _: &mut MockTransaction, user: Uuid) -> anyhow::Result<bool> {
        assert_eq!(user, *subject());
        self.saved.lock().unwrap().calls.push("lock_user");
        Ok(self.exists)
    }
    async fn list(
        &self,
        _: &mut MockTransaction,
        user: Uuid,
    ) -> anyhow::Result<Vec<PurchaseStatus>> {
        assert!(self.creating || self.accepting);
        assert_eq!(user, *subject());
        self.saved.lock().unwrap().calls.push("list");
        Ok(vec![])
    }
    async fn observe_provision(&self, _txn: &mut MockTransaction, _id: Uuid) -> anyhow::Result<()> {
        assert!(self.accepting, "unexpected observe_provision");
        self.saved.lock().unwrap().calls.push("observe_provision");
        Ok(())
    }
    async fn timing_statement(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _original: bool,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        panic!("unexpected timing_statement")
    }
    async fn timestamp(
        &self,
        _txn: &mut MockTransaction,
    ) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
        assert!(self.accepting, "unexpected timestamp");
        Ok(Utc::now())
    }
    async fn cash_capture(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _evidence: &str,
    ) -> anyhow::Result<()> {
        panic!("unexpected cash_capture")
    }
    async fn invoice_artifact(
        &self,
        _txn: &mut MockTransaction,
        _order: &str,
        _candidate: &str,
    ) -> anyhow::Result<String> {
        panic!("unexpected invoice_artifact")
    }
    async fn invoice_attempt(
        &self,
        _txn: &mut MockTransaction,
        _order: &str,
        _attempt: Uuid,
        _observation: &str,
    ) -> anyhow::Result<()> {
        panic!("unexpected invoice_attempt")
    }
    async fn export(&self, _txn: &mut MockTransaction, _user: Uuid) -> anyhow::Result<String> {
        panic!("unexpected export")
    }
    async fn record_debit(&self, _txn: &mut MockTransaction, _id: Uuid) -> anyhow::Result<()> {
        assert!(self.accepting, "unexpected record_debit");
        self.saved.lock().unwrap().calls.push("record_debit");
        Ok(())
    }
    async fn submit(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _payload: &str,
    ) -> anyhow::Result<()> {
        assert!(self.accepting, "unexpected submit");
        self.saved.lock().unwrap().calls.push("submit");
        Ok(())
    }
    async fn bind_period(&self, _txn: &mut MockTransaction, _id: Uuid) -> anyhow::Result<bool> {
        panic!("unexpected bind_period")
    }
    async fn accept(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        body: &str,
        _metadata: &str,
        accepted_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<()> {
        assert!(self.accepting, "unexpected accept");
        let mut saved = self.saved.lock().unwrap();
        saved.calls.push("accept");
        saved.body = Some(body.into());
        saved.record.as_mut().unwrap().status.accepted_at = Some(accepted_at);
        Ok(())
    }
    async fn state(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        state: &str,
        reason: Option<&str>,
    ) -> anyhow::Result<()> {
        assert!(self.accepting, "unexpected state");
        let mut saved = self.saved.lock().unwrap();
        saved.calls.push("state");
        let status = &mut saved.record.as_mut().unwrap().status;
        status.state = state.into();
        status.review_reason = reason.map(Into::into);
        Ok(())
    }
    async fn fulfill(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _result: &str,
    ) -> anyhow::Result<()> {
        panic!("unexpected fulfill")
    }
    async fn fulfillment_statement(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _original: bool,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        panic!("unexpected fulfillment_statement")
    }
    async fn pending(&self, _txn: &mut MockTransaction) -> anyhow::Result<Vec<Uuid>> {
        panic!("unexpected pending")
    }
    async fn claim(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
    ) -> anyhow::Result<Option<PurchaseRecord>> {
        assert!(self.accepting, "unexpected claim");
        self.saved.lock().unwrap().calls.push("claim");
        Ok(None)
    }
    async fn acknowledge(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _generation: i64,
        _outcome: &str,
    ) -> anyhow::Result<()> {
        panic!("unexpected acknowledge")
    }
}

type Sut = PurchaseFeatureServiceImpl<
    MockDatabase,
    MockAuthService<MockTransaction>,
    MockAuthInternalService,
    MockUserRepository<MockTransaction>,
    MockCoinRepository<MockTransaction>,
    MockHeartService<MockTransaction>,
    MockHeartRepository<MockTransaction>,
    MockPremiumRepository<MockTransaction>,
    Repo,
    MockEmailService,
>;
fn subject() -> UserId {
    Uuid::parse_str("11111111-1111-4111-8111-111111111111")
        .unwrap()
        .into()
}
/// The account's renewal: `Some(monthly price)` for a monthly renewal it ordered separately.
fn renewal_repo(monthly: Option<u64>) -> MockPremiumRepository<MockTransaction> {
    use academy_models::premium::{PremiumPlan, PremiumRenewalStatus};
    let mut repo = MockPremiumRepository::new();
    // The reconciliation that switches off a renewal with a missed confirmation deadline;
    // the state it leaves is what the two reads below return.
    repo.expect_get_latest_by_user_id()
        .returning(|_, _| Box::pin(std::future::ready(Ok(None))));
    repo.expect_get_subscription().returning(move |_, _| {
        Box::pin(std::future::ready(
            Ok(monthly.map(|_| PremiumPlan::Monthly)),
        ))
    });
    repo.expect_get_renewal().returning(move |_, _| {
        Box::pin(std::future::ready(Ok(monthly.map(|monthly_price| {
            PremiumRenewalStatus {
                id: Uuid::nil().into(),
                monthly_price,
                confirmation_sent: true,
            }
        }))))
    });
    repo
}
/// A database that hands out any number of transactions, each of which may commit.
fn database_any() -> MockDatabase {
    let mut db = MockDatabase::new();
    db.expect_begin_transaction().returning(|| {
        let mut txn = MockTransaction::new();
        txn.expect_commit()
            .returning(|| Box::pin(std::future::ready(Ok(()))));
        Box::pin(std::future::ready(Ok(txn)))
    });
    db
}
/// Accepts the saved `record` through every step a real acceptance takes: submission,
/// comparison with the current catalog and renewal, debit, confirmation and recovery.
/// `debit` says whether the coins may be charged; otherwise any charge fails the test.
async fn accept_record(
    record: PurchaseRecord,
    renewal: Option<u64>,
    debit: bool,
) -> (PurchaseStatus, Arc<Mutex<Saved>>) {
    let acceptance = PurchaseAcceptance {
        order_id: record.status.offer.id,
        offer_hash: record.status.offer.hash.clone(),
        accepted: true,
        early_performance_requested: true,
    };
    let coins = record.status.offer.product.coins as i64;
    let saved = Arc::new(Mutex::new(Saved {
        record: Some(record),
        ..Saved::default()
    }));
    let mut s = sut_with_database(Arc::clone(&saved), false, true, database_any());
    s.purchase_repo.accepting = true;
    s.premium_repo = renewal_repo(renewal);
    // The confirmation records the sender; nothing is sent from here.
    s.mail.expect_sender().returning(|| None);
    let mut users = MockUserRepository::new();
    users
        .expect_get_purchase_composite()
        .returning(|_, _| Box::pin(std::future::ready(Ok(Some(account())))));
    s.user_repo = users;
    if debit {
        s.coin_repo
            .expect_add_coins()
            .once()
            .returning(move |_, user, amount, withhold| {
                assert_eq!((user, amount, withhold), (subject(), -coins, false));
                Box::pin(std::future::ready(Ok(academy_models::coin::Balance {
                    coins: 0,
                    withheld_coins: 0,
                })))
            });
        s.coin_repo
            .expect_create_transaction()
            .once()
            .returning(|_, _| Box::pin(std::future::ready(Ok(()))));
    }
    let status = s
        .accept_for(subject(), "backend", acceptance, None)
        .await
        .unwrap();
    (status, saved)
}
fn sut(saved: Arc<Mutex<Saved>>, creating: bool, exists: bool, commit: bool) -> Sut {
    sut_with_database(saved, creating, exists, MockDatabase::build(commit))
}
fn sut_with_database(
    saved: Arc<Mutex<Saved>>,
    creating: bool,
    exists: bool,
    db: MockDatabase,
) -> Sut {
    Sut {
        db,
        auth: MockAuthService::new(),
        internal_auth: MockAuthInternalService::new(),
        user_repo: MockUserRepository::new(),
        coin_repo: MockCoinRepository::new(),
        heart: MockHeartService::new(),
        heart_repo: MockHeartRepository::new(),
        premium_repo: renewal_repo(None),
        purchase_repo: Repo {
            saved,
            creating,
            exists,
            accepting: false,
        },
        mail: MockEmailService::new(),
        premium_config: PremiumFeatureConfig {
            monthly_price: 1000,
            yearly_price: 10000,
            daily_documents: None,
        },
        heart_config: HeartFeatureConfig {
            hearts_max: 10,
            hearts_refill_price: 100,
            auto_refill_time: chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        },
        learning_policy_config: LearningPolicyConfig::default(),
        purchase_config: PurchaseFeatureConfig {
            provision_window_seconds: [("premium_monthly".into(), 600)].into(),
            vat_percent: "19".into(),
        },
    }
}
fn account() -> UserComposite {
    UserComposite {
        user: User {
            id: subject(),
            name: "synthetic".try_into().unwrap(),
            email: Some("synthetic@example.invalid".try_into().unwrap()),
            email_verified: true,
            created_at: Utc::now(),
            last_login: None,
            last_name_change: None,
            enabled: true,
            admin: false,
            terms_version: Some("2026-09-r1".try_into().unwrap()),
            terms_accepted_at: None,
            age_confirmed_at: None,
            terms_declined_at: None,
        },
        profile: UserProfile {
            display_name: "Synthetic".try_into().unwrap(),
            bio: Default::default(),
            tags: Default::default(),
            leaderboard_opt_out: false,
        },
        details: UserDetails {
            mfa_enabled: false,
            password_login: true,
            oauth2_login: false,
        },
        invoice_info: UserInvoiceInfo::default(),
    }
}
fn document_hash(terms: &[u8], withdrawal: &[u8]) -> String {
    let mut bytes = Vec::new();
    for document in [terms, withdrawal] {
        bytes.extend_from_slice(&(document.len() as u64).to_be_bytes());
        bytes.extend_from_slice(document);
    }
    format!("{:x}", Sha256::digest(bytes))
}
async fn issued() -> PurchaseRecord {
    issued_with_window(600).await
}
async fn issued_with_window(seconds: u64) -> PurchaseRecord {
    issued_kind_with_window("premium_monthly", seconds).await
}
async fn issued_kind_with_window(kind: &str, seconds: u64) -> PurchaseRecord {
    let saved = Arc::new(Mutex::new(Saved::default()));
    let mut s = sut(Arc::clone(&saved), true, true, true);
    s.purchase_config
        .provision_window_seconds
        .insert(kind.into(), seconds);
    s.user_repo = MockUserRepository::new().with_get_purchase_composite(subject(), Some(account()));
    if kind == "hearts" {
        s.heart = MockHeartService::new().with_get(
            subject(),
            academy_models::heart::Hearts {
                hearts: 3,
                last_refill: Utc::now(),
            },
        );
    }
    let (source, product) = match kind {
        "course" | "coins" => (
            if kind == "course" { "skills" } else { "paypal" },
            PurchaseProduct {
                kind: kind.into(),
                reference: "synthetic".into(),
                title: "Synthetic product".into(),
                description: "Synthetic product description".into(),
                coins: 1337,
                facts: serde_json::json!({"gross_total":"13.37","vat_total":"2.13"}),
                revision: "synthetic".into(),
                service_starts_at: None,
            },
        ),
        _ => ("backend", s.builtin(kind).unwrap()),
    };
    let result = s.issue(subject(), source, product, None).await.unwrap();
    let saved = saved.lock().unwrap();
    let record = saved.record.clone().unwrap();
    assert_eq!(
        saved.calls,
        if source == "backend" {
            vec!["lock_user", "list", "create"]
        } else {
            vec!["lock_user", "create"]
        }
    );
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::to_value(&record.status).unwrap()
    );
    record
}
#[tokio::test]
async fn offer_context_is_product_specific_without_changing_price_or_acceptance() {
    for (kind, price, summary) in [
        ("premium_monthly", 1000, true),
        ("premium_yearly", 10000, true),
        ("hearts", 100, true),
        ("course", 1337, false),
        ("coins", 1337, true),
    ] {
        let record = issued_kind_with_window(kind, 86400).await;
        let offer = record.status.offer;
        assert_eq!(record.status.state, "offered");
        assert!(record.status.accepted_at.is_none());
        assert!(record.submission.is_none());
        assert_eq!(offer.product.coins, price);
        assert_eq!(offer.provision_window_seconds, Some(86400));
        assert_eq!(offer.declaration, DECLARATION);
        assert!(offer.text.contains("24 Stunden"));
        assert_eq!(offer.summary.is_some(), summary, "{kind}");
        if let Some(summary) = &offer.summary {
            // The offer text is the German summary, and the German declaration is the agreed one.
            assert_eq!(offer.text, summary.de.text());
            assert_eq!(summary.de.declaration, offer.declaration);
            assert_eq!(summary.en.declaration, DECLARATION_EN);
            assert_eq!(summary.de.lines.len(), summary.en.lines.len());
            for (de, en) in summary.de.lines.iter().zip(&summary.en.lines) {
                assert_eq!(de.role, en.role);
            }
            assert!(offer.text.contains("keine weiteren Kosten"));
            assert!(!offer.text.contains("Anbieter"));
            assert!(!offer.text.contains("Sprache: Deutsch"));
            assert!(!offer.text.contains("hallo@bootstrap.academy"));
        }
        match kind {
            "coins" => {
                assert!(
                    offer
                        .text
                        .contains("Gesamtpreis: 13,37\u{a0}€ inkl. 19\u{a0}% MwSt.")
                );
                assert!(offer.text.contains("Du zahlst mit PayPal."));
            }
            "course" => {
                // The offers of the other services keep their full text.
                assert!(
                    offer
                        .text
                        .starts_with("Anbieter: bootstrap academy GmbH.\n")
                );
                assert!(offer.text.contains("1337 MorphCoins (13.37 EUR"));
                assert!(
                    offer
                        .text
                        .contains("Widerruf: https://bootstrap.academy/vertrag-widerrufen")
                );
                assert!(offer.text.contains(
                    "Du erhältst die Vertragsbestätigung und deine Leistung innerhalb von 24 Stunden nach Eingang deiner Bestellung."
                ));
            }
            _ => {
                assert!(offer.text.contains(&format!(
                    "Gesamtpreis: {} Morphcoins",
                    group_digits(price, '.')
                )));
                assert!(offer.text.contains("Du zahlst aus deinem Guthaben."));
            }
        }
        if kind == "hearts" {
            assert_eq!(offer.product.facts["refill_units"], 7);
            assert_eq!(
                offer.product.description,
                "Du bekommst 3,5 Herzen dazu und hast dann 5 von 5."
            );
        }
    }
}
#[tokio::test]
async fn closing_new_offer_binds_r4_terms_and_exact_r1_withdrawal_without_account_migration() {
    let r = issued().await;
    assert_eq!(r.terms_pdf, AGB_2026_09_R4_PDF);
    assert_ne!(r.terms_pdf, AGB_2026_09_R1_PDF);
    assert_eq!(r.withdrawal_pdf, WIDERRUFSBELEHRUNG_2026_09_R1_PDF);
    assert_eq!(
        r.status.offer.document_hash,
        document_hash(AGB_2026_09_R4_PDF, WIDERRUFSBELEHRUNG_2026_09_R1_PDF)
    );
    assert_eq!(r.status.state, "offered");
    assert!(r.submission.is_none());
}
#[tokio::test]
async fn closing_offer_displays_exact_whole_hours_and_hashes_the_stored_text() {
    for (seconds, duration, english) in [
        (600, "600 Sekunden", "600 seconds"),
        (3600, "1 Stunde", "1 hour"),
        (3601, "3601 Sekunden", "3601 seconds"),
        (86400, "24 Stunden", "24 hours"),
    ] {
        let r = issued_with_window(seconds).await;
        let mut offer = r.status.offer;
        assert_eq!(offer.provision_window_seconds, Some(seconds));
        assert!(offer.text.contains(&format!(
            "Premium startet direkt nach der Bestätigungsmail, spätestens in {duration}."
        )));
        assert!(offer.summary.as_ref().unwrap().en.text().contains(&format!(
            "Premium starts right after the confirmation email, within {english} at the latest."
        )));
        // The hash covers the text and both wordings of the summary.
        let hash = std::mem::take(&mut offer.hash);
        assert_eq!(
            hash,
            format!("{:x}", Sha256::digest(serde_json::to_vec(&offer).unwrap()))
        );
    }
}

/// An offer issued with the settings of the live platform: three hearts drawn as six half
/// hearts, 50 Morphcoins a refill, Premium at 1.000 and 10.000 Morphcoins, 24 hours.
async fn live_offer(kind: &str, hearts: u64) -> PurchaseOffer {
    let saved = Arc::new(Mutex::new(Saved::default()));
    let mut s = sut(Arc::clone(&saved), true, true, true);
    s.heart_config.hearts_max = 6;
    s.heart_config.hearts_refill_price = 50;
    s.purchase_config
        .provision_window_seconds
        .insert(kind.into(), 86400);
    s.user_repo = MockUserRepository::new().with_get_purchase_composite(subject(), Some(account()));
    if kind == "hearts" {
        s.heart = MockHeartService::new().with_get(
            subject(),
            academy_models::heart::Hearts {
                hearts,
                last_refill: Utc::now(),
            },
        );
    }
    let (source, product) = if kind == "coins" {
        (
            "paypal",
            PurchaseProduct {
                kind: "coins".into(),
                reference: "1000".into(),
                title: "1.000 Morphcoins".into(),
                description: "Guthaben für Angebote auf Bootstrap Academy, einmaliger Kauf.".into(),
                coins: 1000,
                facts: serde_json::json!({"gross_total":"10.00","net_unit":"0.0084","net_total":"8.40","vat_total":"1.60","vat_percent":"19","customer_details":["Synthetic","Deutschland"]}),
                revision: "synthetic".into(),
                service_starts_at: None,
            },
        )
    } else {
        ("backend", s.builtin(kind).unwrap())
    };
    s.issue(subject(), source, product, None)
        .await
        .unwrap()
        .offer
}

fn lines(wording: &OfferWording) -> Vec<(OfferLineRole, &str)> {
    wording
        .lines
        .iter()
        .map(|line| (line.role, line.text.as_str()))
        .collect()
}

#[tokio::test]
async fn short_summary_states_each_fact_once_in_german_and_english() {
    use OfferLineRole::*;
    let hearts = live_offer("hearts", 1).await;
    let summary = hearts.summary.as_ref().unwrap();
    assert_eq!(
        lines(&summary.de),
        [
            (Product, "Herzen auffüllen"),
            (
                Detail,
                "Du bekommst 2,5 Herzen dazu und hast dann 3 von 3. Wir füllen direkt nach der Bestätigungsmail auf, spätestens in 24 Stunden."
            ),
            (
                Price,
                "Gesamtpreis: 50 Morphcoins (0,50\u{a0}€ inkl. 19\u{a0}% MwSt.), keine weiteren Kosten."
            ),
            (Payment, "Du zahlst aus deinem Guthaben."),
        ]
    );
    assert_eq!(
        lines(&summary.en),
        [
            (Product, "Refill hearts"),
            (
                Detail,
                "You get 2.5 more hearts and then have 3 of 3. We refill right after the confirmation email, within 24 hours at the latest."
            ),
            (
                Price,
                "Total price: 50 Morphcoins (€0.50 incl. 19% VAT), no further costs."
            ),
            (Payment, "You pay from your balance."),
        ]
    );
    // One whole heart is one heart.
    let one = live_offer("hearts", 4).await;
    assert!(
        one.text
            .contains("Du bekommst 1 Herz dazu und hast dann 3 von 3.")
    );
    assert!(
        one.summary
            .unwrap()
            .en
            .text()
            .contains("You get 1 more heart and then have 3 of 3.")
    );

    let monthly = live_offer("premium_monthly", 6).await;
    let summary = monthly.summary.as_ref().unwrap();
    assert_eq!(
        lines(&summary.de),
        [
            (Product, "Premium für einen Monat"),
            (Detail, "Unbegrenzte Herzen."),
            (Term, "Laufzeit: 1 Monat, ohne automatische Verlängerung."),
            (
                Detail,
                "Premium startet direkt nach der Bestätigungsmail, spätestens in 24 Stunden. Läuft schon Premium, schließt die neue Zeit daran an."
            ),
            (
                Price,
                "Gesamtpreis: 1.000 Morphcoins (10,00\u{a0}€ inkl. 19\u{a0}% MwSt.), keine weiteren Kosten."
            ),
            (Payment, "Du zahlst aus deinem Guthaben."),
        ]
    );
    assert_eq!(
        lines(&summary.en),
        [
            (Product, "Premium for one month"),
            (Detail, "Unlimited hearts."),
            (Term, "Term: 1 month, no automatic renewal."),
            (
                Detail,
                "Premium starts right after the confirmation email, within 24 hours at the latest. If Premium is already running, the new period follows it."
            ),
            (
                Price,
                "Total price: 1,000 Morphcoins (€10.00 incl. 19% VAT), no further costs."
            ),
            (Payment, "You pay from your balance."),
        ]
    );

    // The yearly price also names its monthly amount (Art. 246a § 1 Abs. 1 Nr. 8 EGBGB).
    let yearly = live_offer("premium_yearly", 6).await;
    let summary = yearly.summary.as_ref().unwrap();
    assert_eq!(lines(&summary.de)[0], (Product, "Premium für ein Jahr"));
    assert_eq!(
        lines(&summary.de)[2],
        (Term, "Laufzeit: 1 Jahr, ohne automatische Verlängerung.")
    );
    assert_eq!(
        lines(&summary.de)[4],
        (
            Price,
            "Gesamtpreis: 10.000 Morphcoins (100,00\u{a0}€ inkl. 19\u{a0}% MwSt.; 8,33\u{a0}€ je Monat), keine weiteren Kosten."
        )
    );
    assert_eq!(
        lines(&summary.en)[4],
        (
            Price,
            "Total price: 10,000 Morphcoins (€100.00 incl. 19% VAT; €8.33 per month), no further costs."
        )
    );

    let coins = live_offer("coins", 6).await;
    let summary = coins.summary.as_ref().unwrap();
    assert_eq!(
        lines(&summary.de),
        [
            (Product, "1.000 Morphcoins"),
            (
                Detail,
                "Guthaben für Angebote auf Bootstrap Academy, einmaliger Kauf. Die Morphcoins sind nach der Zahlung und der Bestätigungsmail auf deinem Konto, spätestens in 24 Stunden."
            ),
            (
                Price,
                "Gesamtpreis: 10,00\u{a0}€ inkl. 19\u{a0}% MwSt., keine weiteren Kosten."
            ),
            (Payment, "Du zahlst mit PayPal."),
        ]
    );
    assert_eq!(
        lines(&summary.en),
        [
            (Product, "1,000 Morphcoins"),
            (
                Detail,
                "Balance for offers on Bootstrap Academy, one-time purchase. The Morphcoins are in your account after payment and the confirmation email, within 24 hours at the latest."
            ),
            (
                Price,
                "Total price: €10.00 incl. 19% VAT, no further costs."
            ),
            (Payment, "You pay with PayPal."),
        ]
    );
    // The invoice details stay in the facts and are not repeated in the text.
    assert!(!coins.text.contains("Synthetic"));

    for offer in [&hearts, &monthly, &yearly, &coins] {
        let summary = offer.summary.as_ref().unwrap();
        assert_eq!(offer.text, summary.de.text());
        assert_eq!(offer.declaration, DECLARATION);
        assert_eq!(summary.de.declaration, DECLARATION);
        assert_eq!(summary.en.declaration, DECLARATION_EN);
        // All courses are free: no offer promises course access.
        assert!(!offer.text.contains("Kurs"), "{}", offer.text);
        assert!(!summary.en.text().contains("course"));
        // Each fact once: one price, one deadline.
        assert_eq!(offer.text.matches("Gesamtpreis").count(), 1);
        assert_eq!(offer.text.matches("24 Stunden").count(), 1);
        assert_eq!(
            offer.text.matches("€").count(),
            1 + usize::from(offer.product.kind == "premium_yearly")
        );
    }
}

#[test]
fn confirmation_mail_names_the_provider_in_front_of_a_short_offer() {
    let offer = |source: &str, summary: bool| {
        let wording = OfferWording {
            lines: vec![OfferLine {
                role: OfferLineRole::Product,
                text: "Kurz".into(),
            }],
            declaration: DECLARATION.into(),
        };
        let mut offer: PurchaseOffer = serde_json::from_value(serde_json::json!({
            "id": Uuid::nil(), "user_id": Uuid::nil(), "source": source,
            "created_at": "2026-10-16T00:00:00Z", "expires_at": "2026-10-16T00:20:00Z",
            "recipient": "synthetic@example.invalid",
            "product": {"kind":"coins","reference":"1000","title":"t","description":"d","coins":1000,"facts":{},"revision":"r","service_starts_at":null},
            "document_hash": "d", "hash": "h", "text": "Kurz", "declaration": DECLARATION
        }))
        .unwrap();
        if summary {
            offer.summary = Some(OfferSummary {
                de: wording.clone(),
                en: wording,
            });
        }
        offer
    };
    assert_eq!(
        agreed_offer(&offer("backend", true)),
        "Anbieter: bootstrap academy GmbH.\nKurz"
    );
    assert_eq!(
        agreed_offer(&offer("paypal", true)),
        "Anbieter: bootstrap academy GmbH.\nKurz\nZahlungsweg: PayPal; keine Abbuchung vorhandener MorphCoins."
    );
    // An offer with its own full text keeps it as it is.
    assert_eq!(agreed_offer(&offer("skills", false)), "Kurz");
}

#[test]
fn amounts_are_written_in_each_language_without_rounding() {
    assert_eq!(euros_de(50), "0,50\u{a0}€");
    assert_eq!(euros_de(100_000_000), "1.000.000,00\u{a0}€");
    assert_eq!(euros_en(833), "€8.33");
    assert_eq!(euros_en(100_000_000), "€1,000,000.00");
    assert_eq!(cents_of("10.00"), Some(1000));
    assert_eq!(cents_of("13.37"), Some(1337));
    assert_eq!(cents_of("5"), Some(500));
    assert_eq!(cents_of("5.5"), Some(550));
    assert_eq!(cents_of("10.0000"), Some(1000));
    assert_eq!(cents_of("10.001"), None);
    assert_eq!(cents_of("-1.00"), None);
    assert_eq!(cents_of(".50"), None);
    assert_eq!(cents_of("1.2.3"), None);
    assert_eq!(cents_of(""), None);
    assert_eq!(duration_de(86400), "24 Stunden");
    assert_eq!(duration_en(3600), "1 hour");
}

async fn historical() -> PurchaseRecord {
    let mut r = issued().await;
    r.status.offer.text = "Original offer from before the copy change".into();
    r.status.offer.product.description = "Original product description".into();
    r.status.offer.declaration = "Original explicit request".into();
    r.terms_pdf = AGB_2026_09_R1_PDF.to_vec();
    r.status.offer.document_hash = document_hash(&r.terms_pdf, &r.withdrawal_pdf);
    r.status.offer.hash.clear();
    r.status.offer.hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&r.status.offer).unwrap())
    );
    r.status.state = "review".into();
    r.status.review_reason = Some("Stored historical review".into());
    r.status.accepted_at = Some(r.status.offer.created_at);
    r.submission = Some(PurchaseAcceptance {
        order_id: r.status.offer.id,
        offer_hash: r.status.offer.hash.clone(),
        accepted: true,
        early_performance_requested: true,
    });
    r
}
#[tokio::test]
async fn closing_saved_r1_replay_keeps_complete_original_without_new_catalog_or_effects() {
    let old = historical().await;
    for exists in [false, true] {
        let saved = Arc::new(Mutex::new(Saved {
            record: Some(old.clone()),
            ..Saved::default()
        }));
        let mut s = sut(Arc::clone(&saved), false, exists, true);
        s.purchase_config.provision_window_seconds.clear();
        s.premium_config.monthly_price = 9999;
        let result = s
            .accept_for(subject(), "backend", old.submission.clone().unwrap(), None)
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::to_value(&old.status).unwrap()
        );
        assert_eq!(saved.lock().unwrap().calls, ["lock_user", "get"]);
    }
}
#[tokio::test]
async fn closing_saved_r1_document_bytes_and_changed_submission_boundary_remain_original() {
    let old = historical().await;
    for (kind, expected) in [
        ("terms", &old.terms_pdf),
        ("withdrawal", &old.withdrawal_pdf),
    ] {
        let saved = Arc::new(Mutex::new(Saved {
            record: Some(old.clone()),
            ..Saved::default()
        }));
        let s = sut(Arc::clone(&saved), false, false, true);
        assert_eq!(
            &s.recipient_document(subject(), old.status.offer.id, kind)
                .await
                .unwrap(),
            expected
        );
        assert_eq!(saved.lock().unwrap().calls, ["get"]);
    }
    for changed_hash in [false, true] {
        let saved = Arc::new(Mutex::new(Saved {
            record: Some(old.clone()),
            ..Saved::default()
        }));
        let s = sut(saved, false, true, false);
        let mut a = old.submission.clone().unwrap();
        if changed_hash {
            a.offer_hash = "different".into();
        } else {
            a.early_performance_requested = false;
        }
        assert!(matches!(
            s.accept_for(subject(), "backend", a, None).await,
            Err(PurchaseError::OfferRequired)
        ));
    }
}

fn daily_account() -> (UserComposite, LearningPolicyConfig) {
    let mut user = account();
    let since = Utc::now() - TimeDelta::days(1);
    let terms = "approved-test-terms".try_into().unwrap();
    user.user.terms_version = Some(terms);
    user.user.terms_accepted_at = Some(since);
    let policy = LearningPolicyConfig {
        mode: LearningMode::Daily,
        terms_version: user.user.terms_version.clone(),
        accepted_since: Some(since),
        user_ids: vec![subject()],
        registered_since: None,
        daily_documents: None,
    };
    (user, policy)
}

#[tokio::test]
async fn daily_rejects_new_heart_and_course_offers_before_charges_or_documents() {
    for kind in ["hearts", "course"] {
        let old = issued_kind_with_window(kind, 600).await;
        let saved = Arc::new(Mutex::new(Saved::default()));
        let mut s = sut(Arc::clone(&saved), true, true, false);
        let (user, policy) = daily_account();
        s.learning_policy_config = policy;
        s.user_repo = MockUserRepository::new().with_get_internal_composite(subject(), Some(user));
        assert!(matches!(
            s.issue(
                subject(),
                &old.status.offer.source,
                old.status.offer.product,
                None
            )
            .await,
            Err(PurchaseError::Unavailable)
        ));
        assert!(saved.lock().unwrap().record.is_none());
    }
}

#[tokio::test]
async fn daily_rejects_stale_unaccepted_heart_and_course_offers_before_submission() {
    for kind in ["hearts", "course"] {
        let old = issued_kind_with_window(kind, 600).await;
        let acceptance = PurchaseAcceptance {
            order_id: old.status.offer.id,
            offer_hash: old.status.offer.hash.clone(),
            accepted: true,
            early_performance_requested: true,
        };
        let saved = Arc::new(Mutex::new(Saved {
            record: Some(old.clone()),
            ..Saved::default()
        }));
        let mut s = sut(Arc::clone(&saved), false, true, false);
        let (user, policy) = daily_account();
        s.learning_policy_config = policy;
        s.user_repo = MockUserRepository::new().with_get_internal_composite(subject(), Some(user));
        assert!(matches!(
            s.accept_for(subject(), &old.status.offer.source, acceptance, None)
                .await,
            Err(PurchaseError::Unavailable)
        ));
        assert_eq!(saved.lock().unwrap().calls, ["lock_user", "get"]);
    }
}

#[tokio::test]
async fn daily_keeps_historical_accepted_orders_replayable_without_new_policy_or_payment() {
    let mut old = historical().await;
    for kind in ["hearts", "course"] {
        old.status.offer.product.kind = kind.into();
        let saved = Arc::new(Mutex::new(Saved {
            record: Some(old.clone()),
            ..Saved::default()
        }));
        let mut s = sut(Arc::clone(&saved), false, true, true);
        s.learning_policy_config = daily_account().1;
        let result = s
            .accept_for(subject(), "backend", old.submission.clone().unwrap(), None)
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::to_value(&old.status).unwrap()
        );
        assert_eq!(saved.lock().unwrap().calls, ["lock_user", "get"]);
    }
}

#[tokio::test]
async fn daily_premium_keeps_coin_price_and_duration_and_describes_actual_benefit() {
    let saved = Arc::new(Mutex::new(Saved::default()));
    let mut s = sut(Arc::clone(&saved), true, true, true);
    let (user, policy) = daily_account();
    s.learning_policy_config = policy;
    s.premium_config.daily_documents = Some(PurchaseDocuments {
        terms_pdf: b"%PDF-1.7 synthetic new terms".to_vec(),
        terms_version: "approved-test-terms".into(),
        withdrawal_pdf: b"%PDF-1.7 synthetic withdrawal".to_vec(),
    });
    s.user_repo = MockUserRepository::new()
        .with_get_internal_composite(subject(), Some(user.clone()))
        .with_get_purchase_composite(subject(), Some(user));
    let offer = s
        .issue(
            subject(),
            "backend",
            s.builtin("premium_monthly").unwrap(),
            None,
        )
        .await
        .unwrap()
        .offer;
    assert_eq!(offer.product.coins, 1000);
    assert_eq!(offer.product.facts["months"], 1);
    assert_eq!(offer.product.facts["automatic_renewal"], false);
    assert!(
        offer
            .product
            .description
            .contains("beliebig viele neue Lektionen")
    );
    assert!(!offer.product.description.contains("Herzen"));
    let summary = offer.summary.as_ref().unwrap();
    assert_eq!(summary.de.lines[1].text, PREMIUM_BENEFIT_DAILY);
    assert_eq!(
        summary.en.lines[1].text,
        "You can start as many new lessons as you like every day."
    );
    let saved = saved.lock().unwrap();
    let record = saved.record.as_ref().unwrap();
    assert_eq!(record.terms_pdf, b"%PDF-1.7 synthetic new terms");
    assert_ne!(record.terms_pdf, AGB_2026_09_R4_PDF);
    assert_eq!(
        offer.document_hash,
        document_hash(&record.terms_pdf, &record.withdrawal_pdf)
    );
}

#[test]
fn daily_document_bundle_requires_matching_version_exact_hashes_and_new_terms() {
    use academy_models::learning_policy::LearningDocumentBundleConfig;
    let (_, mut config) = daily_account();
    assert!(PurchaseDocuments::load_daily(&config).is_err());
    let dir = std::env::temp_dir().join(format!("academy-policy-bundle-{}", Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let terms = b"%PDF-1.7 synthetic approved terms";
    let withdrawal = b"%PDF-1.7 synthetic approved withdrawal";
    let terms_path = dir.join("terms.pdf");
    let withdrawal_path = dir.join("withdrawal.pdf");
    std::fs::write(&terms_path, terms).unwrap();
    std::fs::write(&withdrawal_path, withdrawal).unwrap();
    config.daily_documents = Some(LearningDocumentBundleConfig {
        terms_version: config.terms_version.clone().unwrap(),
        terms_pdf_path: terms_path.clone(),
        terms_sha256: format!("{:x}", Sha256::digest(terms)),
        withdrawal_pdf_path: withdrawal_path,
        withdrawal_sha256: format!("{:x}", Sha256::digest(withdrawal)),
    });
    let documents = PurchaseDocuments::load_daily(&config).unwrap().unwrap();
    assert_eq!(documents.hash(), document_hash(terms, withdrawal));
    config.daily_documents.as_mut().unwrap().terms_sha256 = "0".repeat(64);
    assert!(PurchaseDocuments::load_daily(&config).is_err());
    let bundle = config.daily_documents.as_mut().unwrap();
    bundle.terms_sha256 = format!("{:x}", Sha256::digest(terms));
    bundle.terms_version = "different-version".try_into().unwrap();
    assert!(PurchaseDocuments::load_daily(&config).is_err());
    let bundle = config.daily_documents.as_mut().unwrap();
    bundle.terms_version = config.terms_version.clone().unwrap();
    bundle.terms_sha256 = format!("{:x}", Sha256::digest(AGB_2026_09_R4_PDF));
    std::fs::write(&terms_path, AGB_2026_09_R4_PDF).unwrap();
    assert!(PurchaseDocuments::load_daily(&config).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn no_daily_premium_offer_is_issued_with_missing_approved_documents() {
    let saved = Arc::new(Mutex::new(Saved::default()));
    let mut s = sut(Arc::clone(&saved), true, true, false);
    let (user, policy) = daily_account();
    s.learning_policy_config = policy;
    s.user_repo = MockUserRepository::new().with_get_internal_composite(subject(), Some(user));
    assert!(matches!(
        s.issue(
            subject(),
            "backend",
            s.builtin("premium_monthly").unwrap(),
            None
        )
        .await,
        Err(PurchaseError::Unavailable)
    ));
    assert!(saved.lock().unwrap().record.is_none());
}

#[tokio::test]
async fn public_policy_auth_failure_never_becomes_a_missing_subject() {
    use academy_core_purchase_contracts::LearningPolicyError;
    use academy_models::auth::{AuthError, AuthenticateError};
    for unavailable in [false, true] {
        let mut s = sut_with_database(Default::default(), false, true, MockDatabase::new());
        let mut auth = MockAuthService::new();
        auth.expect_authenticate().once().return_once(move |_| {
            Box::pin(async move {
                Err(if unavailable {
                    AuthenticateError::Other(anyhow::anyhow!("session storage offline"))
                } else {
                    AuthenticateError::InvalidToken
                })
            })
        });
        s.auth = auth;
        let error = s.learning_policy(&"token".into()).await.unwrap_err();
        assert!(match error {
            LearningPolicyError::Auth(AuthError::Authenticate(AuthenticateError::Other(_))) =>
                unavailable,
            LearningPolicyError::Auth(AuthError::Authenticate(AuthenticateError::InvalidToken)) =>
                !unavailable,
            _ => false,
        });
    }
}

/// A Premium offer issued for an account with or without a monthly renewal.
async fn premium_offer(kind: &str, renewal: Option<u64>) -> PurchaseRecord {
    let saved = Arc::new(Mutex::new(Saved::default()));
    let mut s = sut(Arc::clone(&saved), true, true, true);
    s.premium_repo = renewal_repo(renewal);
    s.purchase_config
        .provision_window_seconds
        .insert(kind.into(), 86400);
    s.user_repo = MockUserRepository::new().with_get_purchase_composite(subject(), Some(account()));
    s.issue(subject(), "backend", s.builtin(kind).unwrap(), None)
        .await
        .unwrap();
    saved.lock().unwrap().record.clone().unwrap()
}

#[tokio::test]
async fn premium_offer_names_a_separately_ordered_renewal_in_its_term_line_and_mail() {
    use OfferLineRole::*;
    let record = premium_offer("premium_monthly", Some(1000)).await;
    let offer = &record.status.offer;
    let summary = offer.summary.as_ref().unwrap();
    assert_eq!(
        offer.product.facts["renewal"],
        json!({ "monthly_price": 1000 })
    );
    assert_eq!(
        lines(&summary.de)[2],
        (
            Term,
            "Laufzeit: 1 Monat. Danach läuft deine monatliche Verlängerung weiter (1.000 Morphcoins je Monat)."
        )
    );
    assert_eq!(
        lines(&summary.en)[2],
        (
            Term,
            "Term: 1 month. After that, your monthly renewal continues (1,000 Morphcoins per month)."
        )
    );
    assert_eq!(offer.text, summary.de.text());
    assert!(!offer.text.contains("ohne automatische Verlängerung"));
    let yearly = premium_offer("premium_yearly", Some(1000)).await;
    assert!(yearly.status.offer.text.contains(
        "\nLaufzeit: 1 Jahr. Danach läuft deine monatliche Verlängerung weiter (1.000 Morphcoins je Monat).\n"
    ));
    // The line names the price agreed for the renewal, which a later price change of
    // Premium does not touch (AGB 8.4).
    let agreed = premium_offer("premium_monthly", Some(750)).await;
    let agreed_summary = agreed.status.offer.summary.as_ref().unwrap();
    assert_eq!(agreed.status.offer.product.coins, 1000);
    assert_eq!(
        lines(&agreed_summary.de)[2],
        (
            Term,
            "Laufzeit: 1 Monat. Danach läuft deine monatliche Verlängerung weiter (750 Morphcoins je Monat)."
        )
    );
    assert_eq!(
        lines(&agreed_summary.en)[2],
        (
            Term,
            "Term: 1 month. After that, your monthly renewal continues (750 Morphcoins per month)."
        )
    );
    // Without a renewal the facts say so and the term line says the period does not renew.
    let plain = premium_offer("premium_monthly", None).await;
    assert_eq!(plain.status.offer.product.facts["renewal"], json!(null));
    assert!(
        plain
            .status
            .offer
            .text
            .contains("\nLaufzeit: 1 Monat, ohne automatische Verlängerung.\n")
    );

    // Accepted with the renewal unchanged, the confirmation mail carries the line.
    let (status, accepted) = accept_record(record.clone(), Some(1000), true).await;
    assert_eq!(status.state, "paid");
    let (body, calls) = {
        let accepted = accepted.lock().unwrap();
        (accepted.body.clone().unwrap(), accepted.calls.clone())
    };
    assert!(body.contains(
        "Dein vereinbartes Angebot:\nAnbieter: bootstrap academy GmbH.\nPremium für einen Monat\nUnbegrenzte Herzen.\nLaufzeit: 1 Monat. Danach läuft deine monatliche Verlängerung weiter (1.000 Morphcoins je Monat).\nPremium startet direkt nach der Bestätigungsmail"
    ));
    assert!(body.contains(DECLARATION));
    assert!(calls.contains(&"record_debit"));

    // A renewal that ended, or one ordered after the offer, makes the offer fail without a charge.
    for (offer, renewal) in [
        (record.clone(), None),
        (plain.clone(), Some(1000)),
        (record, Some(900)),
    ] {
        let (status, rejected) = accept_record(offer, renewal, false).await;
        assert_eq!(status.state, "failed");
        assert_eq!(
            status.review_reason.as_deref(),
            Some("Offer expired or changed; no charge")
        );
        let rejected = rejected.lock().unwrap();
        assert!(rejected.body.is_none());
        assert!(!rejected.calls.contains(&"accept") && !rejected.calls.contains(&"record_debit"));
    }
}

#[tokio::test]
async fn an_offer_of_the_previous_revision_is_rejected_without_a_charge() {
    // A heart refill quoted before the switch differs from a current one only in its
    // revision: the description and the quantity are taken from the offer itself.
    let mut record = issued_kind_with_window("hearts", 600).await;
    let offer = &mut record.status.offer;
    assert!(offer.product.revision.starts_with("purchase-2026-10-s1:"));
    offer.product.revision = offer
        .product
        .revision
        .replace("purchase-2026-10-s1", "purchase-2026-09-v1");
    offer.hash.clear();
    offer.hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&*offer).unwrap()));
    let (status, saved) = accept_record(record, None, false).await;
    assert_eq!(status.state, "failed");
    assert_eq!(
        status.review_reason.as_deref(),
        Some("Offer expired or changed; no charge")
    );
    let saved = saved.lock().unwrap();
    assert!(saved.body.is_none());
    assert!(!saved.calls.contains(&"accept") && !saved.calls.contains(&"record_debit"));
}

#[tokio::test]
async fn the_monthly_amount_of_a_yearly_price_is_rounded_to_the_cent() {
    // 10.006 Morphcoins a year are 833,83 cents a month: rounded 8,34 €, truncated 8,33 €.
    let saved = Arc::new(Mutex::new(Saved::default()));
    let mut s = sut(Arc::clone(&saved), true, true, true);
    s.premium_config.yearly_price = 10_006;
    s.purchase_config
        .provision_window_seconds
        .insert("premium_yearly".into(), 86400);
    s.user_repo = MockUserRepository::new().with_get_purchase_composite(subject(), Some(account()));
    let offer = s
        .issue(
            subject(),
            "backend",
            s.builtin("premium_yearly").unwrap(),
            None,
        )
        .await
        .unwrap()
        .offer;
    assert!(
        offer
            .text
            .contains("(100,06\u{a0}€ inkl. 19\u{a0}% MwSt.; 8,34\u{a0}€ je Monat)")
    );
    assert!(
        offer
            .summary
            .unwrap()
            .en
            .text()
            .contains("(€100.06 incl. 19% VAT; €8.34 per month)")
    );
}
