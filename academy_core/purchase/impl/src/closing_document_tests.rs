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
}
struct Repo {
    saved: Arc<Mutex<Saved>>,
    creating: bool,
    exists: bool,
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
        assert!(self.creating);
        assert_eq!(user, *subject());
        self.saved.lock().unwrap().calls.push("list");
        Ok(vec![])
    }
    async fn observe_provision(&self, _txn: &mut MockTransaction, _id: Uuid) -> anyhow::Result<()> {
        panic!("unexpected observe_provision")
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
        panic!("unexpected timestamp")
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
        panic!("unexpected record_debit")
    }
    async fn submit(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _payload: &str,
    ) -> anyhow::Result<()> {
        panic!("unexpected submit")
    }
    async fn bind_period(&self, _txn: &mut MockTransaction, _id: Uuid) -> anyhow::Result<bool> {
        panic!("unexpected bind_period")
    }
    async fn accept(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _body: &str,
        _metadata: &str,
        _accepted_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<()> {
        panic!("unexpected accept")
    }
    async fn state(
        &self,
        _txn: &mut MockTransaction,
        _id: Uuid,
        _state: &str,
        _reason: Option<&str>,
    ) -> anyhow::Result<()> {
        panic!("unexpected state")
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
        panic!("unexpected claim")
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
fn sut(saved: Arc<Mutex<Saved>>, creating: bool, exists: bool, commit: bool) -> Sut {
    Sut {
        db: MockDatabase::build(commit),
        auth: MockAuthService::new(),
        internal_auth: MockAuthInternalService::new(),
        user_repo: MockUserRepository::new(),
        coin_repo: MockCoinRepository::new(),
        heart: MockHeartService::new(),
        heart_repo: MockHeartRepository::new(),
        premium_repo: MockPremiumRepository::new(),
        purchase_repo: Repo {
            saved,
            creating,
            exists,
        },
        mail: MockEmailService::new(),
        premium_config: PremiumFeatureConfig {
            monthly_price: 1000,
            yearly_price: 10000,
        },
        heart_config: HeartFeatureConfig {
            hearts_max: 10,
            hearts_refill_price: 100,
            auto_refill_time: chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
        },
        purchase_config: PurchaseFeatureConfig {
            provision_window_seconds: [("premium_monthly".into(), 600)].into(),
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
    let result = s.issue(subject(), source, product).await.unwrap();
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
    for (kind, price, premium) in [
        ("premium_monthly", 1000, true),
        ("premium_yearly", 10000, true),
        ("hearts", 100, false),
        ("course", 1337, false),
        ("coins", 1337, false),
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
        assert_eq!(offer.text.contains("Premium-Zeitraum"), premium);
        if kind == "coins" {
            assert!(offer.text.contains("13.37 EUR"));
            assert!(offer.text.contains("2.13 EUR"));
        } else {
            assert!(offer.text.contains(&format!("{price} MorphCoins")));
        }
        if kind == "hearts" {
            assert_eq!(offer.product.facts["refill_units"], 7);
            assert!(offer.text.contains("3,5 zusätzliche Herzen"));
            assert!(offer.text.contains("5 Herzen insgesamt"));
        }
    }
}
#[tokio::test]
async fn closing_new_offer_binds_r2_terms_and_exact_r1_withdrawal_without_account_migration() {
    let r = issued().await;
    assert_eq!(r.terms_pdf, AGB_2026_09_R2_PDF);
    assert_ne!(r.terms_pdf, AGB_2026_09_R1_PDF);
    assert_eq!(r.withdrawal_pdf, WIDERRUFSBELEHRUNG_2026_09_R1_PDF);
    assert_eq!(
        r.status.offer.document_hash,
        document_hash(AGB_2026_09_R2_PDF, WIDERRUFSBELEHRUNG_2026_09_R1_PDF)
    );
    assert_eq!(r.status.state, "offered");
    assert!(r.submission.is_none());
}
#[tokio::test]
async fn closing_offer_displays_exact_whole_hours_and_hashes_the_stored_text() {
    for (seconds, duration) in [
        (600, "600 Sekunden"),
        (3600, "1 Stunde"),
        (3601, "3601 Sekunden"),
        (86400, "24 Stunden"),
    ] {
        let r = issued_with_window(seconds).await;
        let mut offer = r.status.offer;
        assert_eq!(offer.provision_window_seconds, Some(seconds));
        assert!(offer.text.contains(&format!(
            "Vertragsbestätigung innerhalb von {duration} nach Eingang deiner Bestellung."
        )));
        let hash = std::mem::take(&mut offer.hash);
        assert_eq!(
            hash,
            format!("{:x}", Sha256::digest(serde_json::to_vec(&offer).unwrap()))
        );
    }
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
            calls: vec![],
        }));
        let mut s = sut(Arc::clone(&saved), false, exists, true);
        s.purchase_config.provision_window_seconds.clear();
        s.premium_config.monthly_price = 9999;
        let result = s
            .accept_for(subject(), "backend", old.submission.clone().unwrap())
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
            calls: vec![],
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
            calls: vec![],
        }));
        let s = sut(saved, false, true, false);
        let mut a = old.submission.clone().unwrap();
        if changed_hash {
            a.offer_hash = "different".into();
        } else {
            a.early_performance_requested = false;
        }
        assert!(matches!(
            s.accept_for(subject(), "backend", a).await,
            Err(PurchaseError::OfferRequired)
        ));
    }
}
