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
    /// The acceptance as it was recorded.
    submitted: Option<PurchaseAcceptance>,
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
        payload: &str,
    ) -> anyhow::Result<()> {
        assert!(self.accepting, "unexpected submit");
        let mut saved = self.saved.lock().unwrap();
        saved.calls.push("submit");
        saved.submitted = Some(serde_json::from_str(payload)?);
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
        // As the PostgreSQL repository reads it back.
        status.refusal = PurchaseRefusal::of(state, reason);
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
    premium_repo(monthly, None)
}
/// `renewal_repo` for an account whose paid Premium runs until `paid_until`.
fn premium_repo(
    monthly: Option<u64>,
    paid_until: Option<DateTime<Utc>>,
) -> MockPremiumRepository<MockTransaction> {
    use academy_models::premium::{Premium, PremiumPlan, PremiumRenewalStatus};
    let mut repo = MockPremiumRepository::new();
    // The reconciliation that switches off a renewal with a missed confirmation deadline;
    // the state it leaves is what the two reads below return. It also names the paid period.
    repo.expect_get_latest_by_user_id().returning(move |_, _| {
        Box::pin(std::future::ready(Ok(paid_until.map(|until| Premium {
            id: Uuid::nil().into(),
            user_id: subject(),
            since: until - TimeDelta::days(30),
            until,
        }))))
    });
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
    accept_with(record, renewal, debit, None).await
}
/// `accept_record` for an account that holds `hearts` half hearts at the acceptance.
async fn accept_with(
    record: PurchaseRecord,
    renewal: Option<u64>,
    debit: bool,
    hearts: Option<u64>,
) -> (PurchaseStatus, Arc<Mutex<Saved>>) {
    let (status, saved) = accept_in(
        record,
        At {
            renewal,
            debit,
            hearts,
            ..At::default()
        },
    )
    .await;
    (status.unwrap(), saved)
}
/// The account, the configuration and the request at an acceptance (`accept_in`).
#[derive(Default)]
struct At {
    renewal: Option<u64>,
    debit: bool,
    hearts: Option<u64>,
    paid_until: Option<DateTime<Utc>>,
    stage2: Option<PurchaseStage2>,
    /// The early start as the request states it; `None` sends it as stage 1 always does.
    early: Option<bool>,
}
/// Accepts the saved `record` as `accept_with` does, with everything `at` sets.
async fn accept_in(
    record: PurchaseRecord,
    at: At,
) -> (Result<PurchaseStatus, PurchaseError>, Arc<Mutex<Saved>>) {
    let At {
        renewal,
        debit,
        hearts,
        paid_until,
        stage2,
        early,
    } = at;
    let acceptance = PurchaseAcceptance {
        order_id: record.status.offer.id,
        offer_hash: record.status.offer.hash.clone(),
        accepted: true,
        early_performance_requested: early.unwrap_or(true),
    };
    let coins = record.status.offer.product.coins as i64;
    let saved = Arc::new(Mutex::new(Saved {
        record: Some(record),
        ..Saved::default()
    }));
    let mut s = sut_with_database(Arc::clone(&saved), false, true, database_any());
    s.purchase_repo.accepting = true;
    s.premium_repo = premium_repo(renewal, paid_until);
    s.purchase_config.stage2 = stage2;
    if let Some(hearts) = hearts {
        s.heart.expect_get().returning(move |_, _| {
            Box::pin(std::future::ready(Ok(academy_models::heart::Hearts {
                hearts,
                last_refill: Utc::now(),
            })))
        });
    }
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
    let status = s.accept_for(subject(), "backend", acceptance, None).await;
    (status, saved)
}
/// Stage 2 in force from `from` on, with documents of its own.
fn stage2_from(from: DateTime<Utc>) -> PurchaseStage2 {
    PurchaseStage2 {
        effective_from: from,
        documents: PurchaseDocuments {
            terms_pdf: b"%PDF-synthetic-terms-2026-10".to_vec(),
            terms_version: "2026-10".into(),
            withdrawal_pdf: b"%PDF-synthetic-withdrawal-2026-10".to_vec(),
        },
    }
}
/// An offer of `kind` issued while `stage2` is configured, for an account with the monthly
/// renewal `renewal`, Premium paid until `paid_until` and 3 of 10 half hearts.
async fn issued_in(
    kind: &str,
    renewal: Option<u64>,
    paid_until: Option<DateTime<Utc>>,
    stage2: Option<PurchaseStage2>,
) -> PurchaseRecord {
    let saved = Arc::new(Mutex::new(Saved::default()));
    let mut s = sut(Arc::clone(&saved), true, true, true);
    s.premium_repo = premium_repo(renewal, paid_until);
    s.purchase_config
        .provision_window_seconds
        .insert(kind.into(), 86400);
    s.purchase_config.stage2 = stage2;
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
    s.issue(subject(), "backend", s.builtin(kind).unwrap(), None)
        .await
        .unwrap();
    saved.lock().unwrap().record.clone().unwrap()
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
            stage2: None,
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
            stage2: None,
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
            (Product, "Premium"),
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
            (Product, "Premium"),
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
    assert_eq!(lines(&summary.de)[0], (Product, "Premium"));
    assert_eq!(lines(&summary.en)[0], (Product, "Premium"));
    assert_eq!(
        lines(&summary.de)[2],
        (Term, "Laufzeit: 1 Jahr, ohne automatische Verlängerung.")
    );
    // The term stands once, in the term line; the title keeps it for the order list.
    assert_eq!(yearly.product.title, "Premium für ein Jahr");
    assert_eq!(monthly.product.title, "Premium für einen Monat");
    let monthly_en = monthly.summary.as_ref().unwrap().en.text();
    for (text, unit) in [
        (&monthly.text, "Monat"),
        (&monthly_en, "month"),
        (&yearly.text, "Jahr"),
        (&summary.en.text(), "year"),
    ] {
        assert_eq!(text.matches(unit).count(), 1, "{text}");
    }
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
    // The order summary of a PayPal offer names PayPal; the mail only adds that the
    // balance is not charged.
    assert_eq!(
        agreed_offer(&offer("paypal", true)),
        "Anbieter: bootstrap academy GmbH.\nKurz\nKeine Abbuchung vorhandener Morphcoins."
    );
    // An offer with its own full text keeps it as it is; an older PayPal text does not
    // name PayPal, so the mail adds the way of payment.
    assert_eq!(agreed_offer(&offer("skills", false)), "Kurz");
    assert_eq!(
        agreed_offer(&offer("paypal", false)),
        "Kurz\nZahlungsweg: PayPal; keine Abbuchung vorhandener MorphCoins."
    );
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

/// A rendered bundle of stage 2 in its own directory: two PDFs and the manifest of
/// `scripts/render-purchase-documents.py --release 2026-10` next to them.
struct Stage2Bundle {
    dir: std::path::PathBuf,
}

impl Stage2Bundle {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("academy-stage2-bundle-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("agb-2026-10.pdf"), STAGE2_TERMS).unwrap();
        std::fs::write(
            dir.join("widerrufsbelehrung-2026-10.pdf"),
            STAGE2_WITHDRAWAL,
        )
        .unwrap();
        Self { dir }
    }

    /// Writes a manifest with `edit` applied to its JSON and returns the config pinning it.
    fn manifest(
        &self,
        edit: impl FnOnce(&mut serde_json::Value),
    ) -> academy_models::purchase::PurchaseStage2Config {
        let mut manifest = json!({
            "release": "2026-10",
            "rendering": "local static HTML, A4",
            "effective_from": "2026-12-09T00:00:00+01:00",
            "effective_date_de": "9. Dezember 2026",
            "effective_date_en": "9 December 2026",
            "frontend_commit": "0123456789abcdef0123456789abcdef01234567",
            "documents": [],
            "bundle": {
                "terms_version": "2026-10",
                "terms_pdf": "agb-2026-10.pdf",
                "terms_sha256": format!("{:x}", Sha256::digest(STAGE2_TERMS)),
                "withdrawal_pdf": "widerrufsbelehrung-2026-10.pdf",
                "withdrawal_sha256": format!("{:x}", Sha256::digest(STAGE2_WITHDRAWAL)),
            },
        });
        edit(&mut manifest);
        let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        let path = self
            .dir
            .join(format!("manifest-{}.json", Uuid::new_v4().simple()));
        std::fs::write(&path, &bytes).unwrap();
        academy_models::purchase::PurchaseStage2Config {
            manifest_path: path,
            manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
        }
    }
}

impl Drop for Stage2Bundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const STAGE2_TERMS: &[u8] = b"%PDF-1.7 synthetic terms 2026-10";
const STAGE2_WITHDRAWAL: &[u8] = b"%PDF-1.7 synthetic withdrawal 2026-10";

#[test]
fn stage_two_takes_start_version_and_documents_from_its_pinned_manifest() {
    assert!(PurchaseDocuments::load_stage2(None).unwrap().is_none());
    let bundle = Stage2Bundle::new();
    let stage2 = PurchaseDocuments::load_stage2(Some(&bundle.manifest(|_| {})))
        .unwrap()
        .unwrap();
    assert_eq!(
        stage2.effective_from,
        "2026-12-08T23:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
    assert_eq!(stage2.documents.terms_version, "2026-10");
    assert_eq!(
        stage2.documents.hash(),
        document_hash(STAGE2_TERMS, STAGE2_WITHDRAWAL)
    );
    // A start in summer time works the same way.
    let summer = bundle.manifest(|m| m["effective_from"] = json!("2027-06-01T00:00:00+02:00"));
    assert_eq!(
        PurchaseDocuments::load_stage2(Some(&summer))
            .unwrap()
            .unwrap()
            .effective_from,
        "2027-05-31T22:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
}

#[test]
fn stage_two_refuses_a_manifest_that_does_not_fit() {
    let bundle = Stage2Bundle::new();
    let refused = |config: academy_models::purchase::PurchaseStage2Config, what: &str| {
        assert!(
            PurchaseDocuments::load_stage2(Some(&config)).is_err(),
            "{what}"
        );
    };
    // The manifest itself is not the pinned one, or not where it should be.
    let mut config = bundle.manifest(|_| {});
    config.manifest_sha256 = "0".repeat(64);
    refused(config, "manifest checksum");
    let mut config = bundle.manifest(|_| {});
    config.manifest_path = std::path::PathBuf::from("relative/manifest.json");
    refused(config, "relative path");
    // A start that is not 00:00 in Berlin, the version or release of stage 1, a dirty render.
    for (what, edit) in [
        (
            "start at 00:00 UTC",
            json!({"effective_from": "2026-12-09T00:00:00Z"}),
        ),
        (
            "start without offset",
            json!({"effective_from": "2026-12-09T00:00:00"}),
        ),
        ("release r4", json!({"release": "2026-09-r4"})),
        (
            "rendered from changes",
            json!({"frontend_commit": "0123456789abcdef-dirty"}),
        ),
    ] {
        refused(
            bundle.manifest(|m| {
                for (key, value) in edit.as_object().unwrap() {
                    m[key] = value.clone();
                }
            }),
            what,
        );
    }
    for (what, key, value) in [
        ("terms version r3", "terms_version", json!("2026-09-r3")),
        ("PDF checksum", "withdrawal_sha256", json!("0".repeat(64))),
        ("PDF outside", "terms_pdf", json!("../agb-2026-10.pdf")),
        ("missing PDF", "terms_pdf", json!("agb-2026-11.pdf")),
    ] {
        refused(bundle.manifest(|m| m["bundle"][key] = value), what);
    }
    // The documents of stage 1.
    std::fs::write(
        bundle.dir.join("widerrufsbelehrung-2026-10.pdf"),
        WIDERRUFSBELEHRUNG_2026_09_R1_PDF,
    )
    .unwrap();
    refused(
        bundle.manifest(|m| {
            m["bundle"]["withdrawal_sha256"] = json!(format!(
                "{:x}",
                Sha256::digest(WIDERRUFSBELEHRUNG_2026_09_R1_PDF)
            ))
        }),
        "withdrawal of stage 1",
    );
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
        "Dein vereinbartes Angebot:\nAnbieter: bootstrap academy GmbH.\nPremium\nUnbegrenzte Herzen.\nLaufzeit: 1 Monat. Danach läuft deine monatliche Verlängerung weiter (1.000 Morphcoins je Monat).\nPremium startet direkt nach der Bestätigungsmail"
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
    // Clients read why without matching the sentence: a new offer can be asked for.
    assert_eq!(status.refusal, Some(PurchaseRefusal::OfferChanged));
    let saved = saved.lock().unwrap();
    assert!(saved.body.is_none());
    assert!(!saved.calls.contains(&"accept") && !saved.calls.contains(&"record_debit"));
}

#[tokio::test]
async fn a_heart_refill_is_accepted_only_while_it_ends_at_the_promised_count() {
    // Quoted at 3 of 10 half hearts: "Du bekommst 3,5 Herzen dazu und hast dann 5 von 5."
    let record = issued_kind_with_window("hearts", 600).await;
    assert_eq!(record.status.offer.product.facts["refill_units"], json!(7));
    assert!(record.status.offer.text.contains("und hast dann 5 von 5."));
    // Unchanged: accepted once, with one debit.
    let (status, saved) = accept_with(record.clone(), None, true, Some(3)).await;
    assert_eq!(status.state, "paid");
    assert_eq!(status.refusal, None);
    {
        let saved = saved.lock().unwrap();
        assert!(saved.body.is_some());
        assert_eq!(
            saved
                .calls
                .iter()
                .filter(|call| **call == "record_debit")
                .count(),
            1
        );
    }
    // Hearts lost or regained while the dialog was open: the quantity no longer leads to
    // 5 of 5, so the offer is refused as changed, without a charge.
    for hearts in [0, 2, 4, 9] {
        let (status, saved) = accept_with(record.clone(), None, false, Some(hearts)).await;
        assert_eq!(status.state, "failed", "{hearts}");
        assert_eq!(status.review_reason.as_deref(), Some(OFFER_CHANGED_REASON));
        assert_eq!(status.refusal, Some(PurchaseRefusal::OfferChanged));
        let saved = saved.lock().unwrap();
        assert!(saved.body.is_none());
        assert!(!saved.calls.contains(&"accept") && !saved.calls.contains(&"record_debit"));
    }
    // Already full: nothing to refill, and no new offer to ask for.
    let (status, saved) = accept_with(record, None, false, Some(10)).await;
    assert_eq!(status.state, "failed");
    assert_eq!(
        status.review_reason.as_deref(),
        Some("No refill necessary; no charge")
    );
    assert_eq!(status.refusal, None);
    assert!(!saved.lock().unwrap().calls.contains(&"record_debit"));
}

#[tokio::test]
async fn a_premium_offer_of_stage_one_is_accepted_with_its_own_text() {
    // Before the product line became "Premium", it named the term as well. Every promise
    // of such an offer is the same as today's, so it keeps the revision, is accepted once,
    // and the mail confirms it with its own stored text.
    let mut record = premium_offer("premium_monthly", None).await;
    let offer = &mut record.status.offer;
    let summary = offer.summary.as_mut().unwrap();
    summary.de.lines[0].text = "Premium für einen Monat".into();
    summary.en.lines[0].text = "Premium for one month".into();
    offer.text = summary.de.text();
    offer.hash.clear();
    offer.hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&*offer).unwrap()));
    assert!(offer.product.revision.starts_with("purchase-2026-10-s1:"));
    let (status, saved) = accept_record(record, None, true).await;
    assert_eq!(status.state, "paid");
    let saved = saved.lock().unwrap();
    let body = saved.body.as_deref().unwrap();
    assert!(
        body.contains("\nPremium für einen Monat\nUnbegrenzte Herzen.\n"),
        "{body}"
    );
    assert!(body.contains("für deine Bestellung „Premium für einen Monat“."));
    assert_eq!(
        saved
            .calls
            .iter()
            .filter(|call| **call == "record_debit")
            .count(),
        1
    );
}

#[tokio::test]
async fn the_confirmation_mail_names_the_term_of_premium_once() {
    // The first sentence names the product line of the summary; the term stands in the
    // term line only. The mail is German.
    for (kind, unit) in [("premium_monthly", "Monat"), ("premium_yearly", "Jahr")] {
        let record = premium_offer(kind, None).await;
        assert_ne!(record.status.offer.product.title, "Premium");
        let (status, saved) = accept_record(record, None, true).await;
        assert_eq!(status.state, "paid", "{kind}");
        let saved = saved.lock().unwrap();
        let body = saved.body.as_deref().unwrap();
        assert!(
            body.contains("hier ist die Vertragsbestätigung für deine Bestellung „Premium“."),
            "{body}"
        );
        assert!(body.contains(&format!(
            "\nPremium\nUnbegrenzte Herzen.\nLaufzeit: 1 {unit},"
        )));
        assert_eq!(body.matches(unit).count(), 1, "{body}");
    }
    // A heart refill names its product line, which is its title, as before; an offer with
    // its own full text names its title.
    let mut hearts = live_offer("hearts", 4).await;
    assert_eq!(hearts.product.title, "Herzen auffüllen");
    assert_eq!(order_name(&hearts), "Herzen auffüllen");
    hearts.summary = None;
    hearts.product.title = "Synthetischer Kurs".into();
    assert_eq!(order_name(&hearts), "Synthetischer Kurs");
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

const NO_DECLARATION: &str =
    "Eine Erklärung zum vorzeitigen Beginn hast du bei dieser Bestellung nicht abgegeben.";

#[tokio::test]
async fn stage_two_begins_exactly_at_its_effective_date() {
    let stage2 = stage2_from(Utc::now() - TimeDelta::seconds(1));
    let legacy = PurchaseDocuments::legacy().hash();
    // Unset, and set to a time still ahead: every offer keeps stage 1.
    for configured in [None, Some(stage2_from(Utc::now() + TimeDelta::hours(1)))] {
        let offer = issued_in("hearts", None, None, configured)
            .await
            .status
            .offer;
        assert_eq!(offer.acceptance, None);
        assert_eq!(offer.terms_version, None);
        assert_eq!(offer.declaration, DECLARATION);
        assert_eq!(offer.document_hash, legacy);
        assert!(offer.requires_early_performance());
    }
    // From the effective date on: the order button accepts, the documents are those of
    // stage 2, the offer names their version, and a heart refill carries no declaration.
    let offer = issued_in("hearts", None, None, Some(stage2.clone()))
        .await
        .status
        .offer;
    assert_eq!(offer.acceptance, Some(OfferAcceptance::Button));
    assert_eq!(offer.terms_version.as_deref(), Some("2026-10"));
    assert_eq!(offer.document_hash, stage2.documents.hash());
    assert_eq!(offer.declaration, "");
    let summary = offer.summary.as_ref().unwrap();
    assert_eq!(
        (
            summary.de.declaration.as_str(),
            summary.en.declaration.as_str()
        ),
        ("", "")
    );
    assert!(!offer.requires_early_performance());
    // The lines of a heart refill are those of stage 1.
    assert!(offer.text.contains("und hast dann 5 von 5."));
}

/// The switch happens at the configured moment itself, and never for the Daily cohort.
#[test]
fn stage_two_applies_from_its_start_inclusive() {
    let start: DateTime<Utc> = "2026-12-08T23:00:00Z".parse().unwrap();
    let mut s = sut_with_database(
        Arc::new(Mutex::new(Saved::default())),
        false,
        true,
        database_any(),
    );
    assert!(!s.stage2_at(LearningMode::Legacy, start));
    s.purchase_config.stage2 = Some(stage2_from(start));
    assert!(s.stage2_at(LearningMode::Legacy, start));
    assert!(s.stage2_at(LearningMode::Shadow, start));
    assert!(!s.stage2_at(LearningMode::Legacy, start - TimeDelta::nanoseconds(1)));
    assert!(!s.stage2_at(LearningMode::Daily, start + TimeDelta::days(1)));
}

/// The declarations of stage 2 are the sentences the Widerrufsbelehrung 2026-10 and its
/// English translation quote under "Erklärungen bei der Bestellung" / "Declarations when you
/// order". The fixtures are copies of the reviewed drafts without their internal header
/// comment (`legal/agb-fassung-2026-10-16/entwurf-v2/`, SHA-256 of the drafts `999c3bc7…`
/// and `99803313…`); the frontend tests pin the same copies.
#[test]
fn the_premium_declaration_is_the_sentence_of_the_withdrawal_instructions() {
    for (fixture, sha256, quoted) in [
        (
            include_str!("../tests/fixtures/legal-2026-10/widerrufsbelehrung-2026-10.md"),
            "b99f539e738234b918526aada586eba2a142c4fa958a08a167882611f9250687",
            format!("„{DECLARATION_PREMIUM}“"),
        ),
        (
            include_str!("../tests/fixtures/legal-2026-10/widerrufsbelehrung-2026-10-en.md"),
            "d9d646bad4ac30175b8a764a1ee32dfa09664a232334128551371e60734712de",
            format!("In English: \"{DECLARATION_PREMIUM_EN}\""),
        ),
    ] {
        assert_eq!(format!("{:x}", Sha256::digest(fixture)), sha256);
        assert_eq!(
            fixture.lines().filter(|line| *line == quoted).count(),
            1,
            "{quoted}"
        );
    }
}

#[tokio::test]
async fn a_heart_refill_of_stage_two_is_accepted_without_a_declaration() {
    let stage2 = stage2_from(Utc::now() - TimeDelta::seconds(1));
    let record = issued_in("hearts", None, None, Some(stage2.clone())).await;
    for sent in [false, true] {
        let (status, saved) = accept_in(
            record.clone(),
            At {
                debit: true,
                hearts: Some(3),
                stage2: Some(stage2.clone()),
                early: Some(sent),
                ..At::default()
            },
        )
        .await;
        assert_eq!(status.unwrap().state, "paid", "early start sent: {sent}");
        let saved = saved.lock().unwrap();
        // The dialog showed no declaration, so none is recorded, whatever the request says.
        assert!(
            !saved
                .submitted
                .as_ref()
                .unwrap()
                .early_performance_requested
        );
        let body = saved.body.as_deref().unwrap();
        assert!(
            body.contains(&format!("\n\n{NO_DECLARATION}\n\n")),
            "{body}"
        );
        assert!(!body.contains("Deine Erklärung zum vorzeitigen Beginn"));
    }
}

#[tokio::test]
async fn premium_of_stage_two_asks_for_the_early_start_only_when_it_begins_within_the_withdrawal_period()
 {
    let stage2 = stage2_from(Utc::now() - TimeDelta::seconds(1));
    // The period ends with the Berlin day 14 days after the order, moved past weekends and
    // holidays; a paid period that ends exactly then still counts as early. The offer decides
    // for its last moment of acceptance, 20 minutes after it is issued.
    let end = withdrawal_period_end(Utc::now() + TimeDelta::minutes(20));
    for (paid_until, table) in [
        (None, true),
        (Some(Utc::now() - TimeDelta::days(1)), true),
        (Some(Utc::now() + TimeDelta::days(13)), true),
        (Some(end), true),
        (Some(end + TimeDelta::seconds(1)), false),
        (Some(Utc::now() + TimeDelta::days(30)), false),
    ] {
        let record = issued_in("premium_monthly", None, paid_until, Some(stage2.clone())).await;
        let offer = &record.status.offer;
        // Should a Berlin midnight fall between `end` and the issue, the offer's own moment
        // decides; the table holds whenever both give the same end.
        let own_end = withdrawal_period_end(offer.expires_at);
        let early = paid_until.is_none_or(|until| until <= own_end);
        if own_end == end {
            assert_eq!(early, table, "{paid_until:?}");
        }
        assert_eq!(
            offer.product.facts["early_start"],
            json!(early),
            "{paid_until:?}"
        );
        let summary = offer.summary.as_ref().unwrap();
        let expected = if early {
            (DECLARATION_PREMIUM, DECLARATION_PREMIUM_EN)
        } else {
            ("", "")
        };
        assert_eq!(offer.declaration, expected.0);
        assert_eq!(
            (
                summary.de.declaration.as_str(),
                summary.en.declaration.as_str()
            ),
            expected
        );
        let at = |early_sent| At {
            debit: true,
            paid_until,
            stage2: Some(stage2.clone()),
            early: Some(early_sent),
            ..At::default()
        };
        if early {
            // Without the declaration a period that starts so early is not ordered.
            let (refused, saved) = accept_in(
                record.clone(),
                At {
                    debit: false,
                    ..at(false)
                },
            )
            .await;
            assert!(matches!(refused, Err(PurchaseError::OfferRequired)));
            assert!(!saved.lock().unwrap().calls.contains(&"record_debit"));
        }
        let (status, saved) = accept_in(record.clone(), at(early)).await;
        assert_eq!(status.unwrap().state, "paid", "{paid_until:?}");
        let saved = saved.lock().unwrap();
        let body = saved.body.as_deref().unwrap();
        if early {
            assert!(body.contains(&format!(
                "\n\nDeine Erklärung zum vorzeitigen Beginn:\n{DECLARATION_PREMIUM}\n\n"
            )));
        } else {
            assert!(
                body.contains(&format!("\n\n{NO_DECLARATION}\n\n")),
                "{body}"
            );
        }
    }
}

/// The acceptance decides the early start for the moment the offer decided it, its expiry,
/// never for the clock at the acceptance. An offer whose expiry lies past a Berlin midnight
/// that moves the end of the withdrawal period, accepted before that midnight, keeps its
/// declaration; with the clock of the acceptance it would be refused as changed.
#[tokio::test]
async fn the_early_start_is_decided_for_the_expiry_of_the_offer_not_the_clock() {
    let stage2 = stage2_from(Utc::now() - TimeDelta::seconds(1));
    let now_end = withdrawal_period_end(Utc::now());
    // The first day ahead whose end of the withdrawal period lies after today's; weekends and
    // holidays can keep it the same for a few days.
    let expiry = (1..=10)
        .map(|days| Utc::now() + TimeDelta::days(days))
        .find(|at| withdrawal_period_end(*at) > now_end)
        .unwrap();
    let paid_until = withdrawal_period_end(expiry);
    assert!(paid_until > now_end, "early for the expiry, not for now");
    let mut record = issued_in("premium_monthly", None, None, Some(stage2.clone())).await;
    assert_eq!(
        record.status.offer.product.facts["early_start"],
        json!(true)
    );
    record.status.offer.expires_at = expiry;
    let (status, saved) = accept_in(
        record,
        At {
            debit: true,
            paid_until: Some(paid_until),
            stage2: Some(stage2),
            early: Some(true),
            ..At::default()
        },
    )
    .await;
    assert_eq!(status.unwrap().state, "paid");
    assert!(
        saved
            .lock()
            .unwrap()
            .body
            .as_deref()
            .unwrap()
            .contains(DECLARATION_PREMIUM)
    );
}

#[tokio::test]
async fn an_offer_keeps_the_documents_and_rules_it_was_issued_with() {
    let stage2 = stage2_from(Utc::now() - TimeDelta::seconds(1));
    // Issued before the effective date, accepted after it: stage 1 with its own documents.
    let before = issued_in("premium_monthly", None, None, None).await;
    let (status, saved) = accept_in(
        before,
        At {
            debit: true,
            stage2: Some(stage2.clone()),
            ..At::default()
        },
    )
    .await;
    assert_eq!(status.unwrap().state, "paid");
    assert!(
        saved
            .lock()
            .unwrap()
            .body
            .as_deref()
            .unwrap()
            .contains(DECLARATION)
    );
    // Stage 2 switched off again: its offers are refused as changed, nothing is charged.
    let after = issued_in("hearts", None, None, Some(stage2.clone())).await;
    let (status, saved) = accept_in(
        after,
        At {
            hearts: Some(3),
            early: Some(false),
            ..At::default()
        },
    )
    .await;
    let status = status.unwrap();
    assert_eq!(status.state, "failed");
    assert_eq!(status.refusal, Some(PurchaseRefusal::OfferChanged));
    assert!(!saved.lock().unwrap().calls.contains(&"record_debit"));
    // A Premium offer without the declaration, while the paid period now ends within 14
    // days: the start would fall into the withdrawal period, so the offer has changed.
    let late = Utc::now() + TimeDelta::days(30);
    let record = issued_in("premium_monthly", None, Some(late), Some(stage2.clone())).await;
    assert_eq!(record.status.offer.declaration, "");
    let (status, _) = accept_in(
        record,
        At {
            paid_until: Some(Utc::now() + TimeDelta::days(10)),
            stage2: Some(stage2),
            early: Some(false),
            ..At::default()
        },
    )
    .await;
    let status = status.unwrap();
    assert_eq!(status.state, "failed");
    assert_eq!(status.refusal, Some(PurchaseRefusal::OfferChanged));
}

#[tokio::test]
async fn the_renewal_in_a_stage_two_offer_names_its_euro_amount() {
    use OfferLineRole::*;
    let stage2 = stage2_from(Utc::now() - TimeDelta::seconds(1));
    let offer = issued_in("premium_monthly", Some(1000), None, Some(stage2))
        .await
        .status
        .offer;
    let summary = offer.summary.as_ref().unwrap();
    assert_eq!(
        lines(&summary.de)[2],
        (
            Term,
            "Laufzeit: 1 Monat. Danach läuft deine monatliche Verlängerung weiter (1.000 Morphcoins = 10,00\u{a0}€ inkl. 19\u{a0}% MwSt. je Monat)."
        )
    );
    assert_eq!(
        lines(&summary.en)[2],
        (
            Term,
            "Term: 1 month. After that, your monthly renewal continues (1,000 Morphcoins = €10.00 incl. 19% VAT per month)."
        )
    );
    // Stage 1 keeps its line.
    let before = issued_in("premium_monthly", Some(1000), None, None)
        .await
        .status
        .offer;
    assert!(before.text.contains(
        "\nLaufzeit: 1 Monat. Danach läuft deine monatliche Verlängerung weiter (1.000 Morphcoins je Monat).\n"
    ));
}
