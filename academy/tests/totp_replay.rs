//! Durable second-factor regressions with owned PostgreSQL and Valkey.
use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
};

use academy::environment::{ConfigProvider, Provider, types};
use academy_core_mfa_contracts::authenticate::{
    MfaAuthenticateError, MfaAuthenticateResult, MfaAuthenticateService,
};
use academy_core_mfa_impl::authenticate::MfaAuthenticateServiceImpl;
use academy_demo::{
    mfa::ADMIN2_TOTP_1,
    user::{ADMIN2, BAR},
};
use academy_di::{Provide, provider};
use academy_models::mfa::{MfaAuthentication, TotpSecret};
use academy_persistence_contracts::{Database, Transaction, mfa::MfaRepository};
use academy_persistence_postgres::mfa::PostgresMfaRepository;
use academy_shared_contracts::time::TimeService;
use academy_shared_contracts::totp::TotpService;
use academy_shared_impl::{
    hash::HashServiceImpl,
    secret::SecretServiceImpl,
    totp::{TotpServiceConfig, TotpServiceImpl},
};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use tracing::instrument::WithSubscriber;

#[path = "../../academy_persistence/postgres/tests/common/mod.rs"]
mod common;

#[derive(Clone)]
struct FixedTime(Arc<AtomicI64>);
impl TimeService for FixedTime {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.0.load(Ordering::SeqCst), 0).unwrap()
    }
}
provider! { ReplayProvider { time: FixedTime, config: TotpServiceConfig, disable: types::MfaDisable, } }
provider! { GenerationProvider { time: FixedTime, config: TotpServiceConfig, } }
type Auth = MfaAuthenticateServiceImpl<
    HashServiceImpl,
    TotpServiceImpl<SecretServiceImpl, FixedTime>,
    types::MfaDisable,
    PostgresMfaRepository,
>;

async fn fixture() -> (Auth, types::Database, types::Cache, FixedTime) {
    let db = common::setup().await;
    let config = academy_config::load().unwrap();
    let port = std::env::var("AUTH_REVIEW_VALKEY_PORT").unwrap();
    assert_eq!(config.cache.url, format!("redis://127.0.0.1:{port}/0"));
    let cache = academy::cache::connect(&config.cache).await.unwrap();
    cache.clear().await.unwrap();
    let mut base = Provider::new(
        ConfigProvider::new(&config).unwrap(),
        db.clone(),
        cache.clone(),
        academy_email_impl::EmailServiceImpl::dummy().await,
    );
    let time = FixedTime(Arc::new(AtomicI64::new(1724949831)));
    let mut provider = ReplayProvider {
        _cache: Default::default(),
        time: time.clone(),
        config: TotpServiceConfig {
            secret_length: 24.try_into().unwrap(),
        },
        disable: base.provide(),
    };
    let mut txn = db.begin_transaction().await.unwrap();
    PostgresMfaRepository
        .save_totp_device_secret(&mut txn, ADMIN2_TOTP_1.id, &secret())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    (provider.provide(), db, cache, time)
}
fn secret() -> TotpSecret {
    TotpSecret::try_new(b"XSSYkVp8pDsOnT1jB5eN0CB8".to_vec()).unwrap()
}
fn cmd() -> MfaAuthentication {
    MfaAuthentication {
        totp_code: Some("960546".try_into().unwrap()),
        recovery_code: None,
    }
}
async fn authenticate(
    auth: &Auth,
    db: &types::Database,
) -> Result<MfaAuthenticateResult, MfaAuthenticateError> {
    let mut txn = db.begin_transaction().await.unwrap();
    let result = auth.authenticate(&mut txn, ADMIN2.user.id, cmd()).await;
    if result.is_ok() {
        txn.commit().await.unwrap();
    } else {
        txn.rollback().await.unwrap();
    }
    result
}
async fn watermark(db: &types::Database) -> (i64, String) {
    let txn = db.begin_transaction().await.unwrap();
    let row = txn
        .txn()
        .query_one(
            "SELECT last_accepted_step,xmin::text FROM totp_device_secrets WHERE id=$1",
            &[&*ADMIN2_TOTP_1.id],
        )
        .await
        .unwrap();
    (row.get(0), row.get(1))
}

#[tokio::test]
async fn a_second_factor_has_exactly_one_committed_winner_among_256_requests() {
    let (auth, db, _, _) = fixture().await;
    let barrier = tokio::sync::Barrier::new(256);
    let results = join_all((0..256).map(|_| async {
        barrier.wait().await;
        authenticate(&auth, &db).await
    }))
    .await;
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Ok(MfaAuthenticateResult::Ok)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(MfaAuthenticateError::Failed)))
            .count(),
        255
    );
    assert_eq!(watermark(&db).await.0, 1724949831 / 30);
}

#[tokio::test]
async fn cache_loss_and_skew_boundaries_do_not_restore_a_consumed_code() {
    let (auth, db, cache, time) = fixture().await;
    let step = 1724949831 / 30;
    time.0.store((step - 1) * 30, Ordering::SeqCst);
    assert_eq!(
        authenticate(&auth, &db).await.unwrap(),
        MfaAuthenticateResult::Ok
    );
    let before = watermark(&db).await;
    cache.clear().await.unwrap();
    for now in [
        (step - 1) * 30,
        step * 30,
        (step + 2) * 30 - 1,
        (step + 2) * 30,
    ] {
        time.0.store(now, Ordering::SeqCst);
        assert!(matches!(
            authenticate(&auth, &db).await,
            Err(MfaAuthenticateError::Failed)
        ));
        assert_eq!(
            watermark(&db).await,
            before,
            "rejected request changed the replay row"
        );
    }
}

#[tokio::test]
async fn invalid_codes_and_wrong_owners_never_advance_the_row() {
    let (auth, db, _, _) = fixture().await;
    let before = watermark(&db).await;
    let mut txn = db.begin_transaction().await.unwrap();
    let bad = MfaAuthentication {
        totp_code: Some("384957".try_into().unwrap()),
        recovery_code: None,
    };
    assert!(matches!(
        auth.authenticate(&mut txn, ADMIN2.user.id, bad).await,
        Err(MfaAuthenticateError::Failed)
    ));
    assert!(
        !PostgresMfaRepository
            .consume_totp_step(&mut txn, BAR.user.id, &secret(), 1724949831 / 30, false)
            .await
            .unwrap()
    );
    txn.commit().await.unwrap();
    assert_eq!(watermark(&db).await, before);
}

#[tokio::test]
async fn database_write_failure_denies_authorization_and_does_not_consume_the_code() {
    let (auth, db, _, _) = fixture().await;
    let before = watermark(&db).await;
    let mut txn = db.begin_transaction().await.unwrap();
    txn.txn()
        .batch_execute("SET TRANSACTION READ ONLY")
        .await
        .unwrap();
    assert!(matches!(
        auth.authenticate(&mut txn, ADMIN2.user.id, cmd()).await,
        Err(MfaAuthenticateError::Other(_))
    ));
    txn.rollback().await.unwrap();
    assert_eq!(watermark(&db).await, before);
    assert_eq!(
        authenticate(&auth, &db).await.unwrap(),
        MfaAuthenticateResult::Ok
    );
}

#[tokio::test]
async fn replacing_a_secret_resets_only_that_secrets_watermark() {
    let (auth, db, _, _) = fixture().await;
    authenticate(&auth, &db).await.unwrap();
    let before = watermark(&db).await.0;
    let mut txn = db.begin_transaction().await.unwrap();
    PostgresMfaRepository
        .save_totp_device_secret(&mut txn, ADMIN2_TOTP_1.id, &secret())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(watermark(&db).await.0, before);
    let mut txn = db.begin_transaction().await.unwrap();
    let replacement = TotpSecret::try_new(b"different-owned-totp-secret".to_vec()).unwrap();
    PostgresMfaRepository
        .save_totp_device_secret(&mut txn, ADMIN2_TOTP_1.id, &replacement)
        .await
        .unwrap();
    assert!(
        !PostgresMfaRepository
            .consume_totp_step(&mut txn, ADMIN2.user.id, &secret(), before + 1, false)
            .await
            .unwrap()
    );
    txn.commit().await.unwrap();
    assert_eq!(watermark(&db).await.0, -1);
}

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[tokio::test]
async fn trace_events_and_spans_never_contain_the_totp_value_or_secret() {
    let (auth, db, _, _) = fixture().await;
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(Arc::clone(&bytes));
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(
            tracing_subscriber::fmt::format::FmtSpan::NEW
                | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
        )
        .with_writer(move || writer.clone())
        .finish();
    let dispatch = tracing::Dispatch::new(subscriber);
    let mut generation = GenerationProvider {
        _cache: Default::default(),
        time: FixedTime(Arc::new(AtomicI64::new(1724949831))),
        config: TotpServiceConfig {
            secret_length: 24.try_into().unwrap(),
        },
    };
    let totp: TotpServiceImpl<SecretServiceImpl, FixedTime> = generation.provide();
    let (generated_secret, setup) =
        tracing::dispatcher::with_default(&dispatch, || totp.generate_secret());
    authenticate(&auth, &db)
        .with_subscriber(dispatch.clone())
        .await
        .unwrap();
    assert!(matches!(
        authenticate(&auth, &db).with_subscriber(dispatch).await,
        Err(MfaAuthenticateError::Failed)
    ));
    let log = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("consume_totp_step"),
        "TRACE capture must include the durable reservation span"
    );
    assert!(
        !log.contains("960546"),
        "TRACE contained the synthetic TOTP value"
    );
    assert!(
        !log.contains("XSSYkVp8pDsOnT1jB5eN0CB8"),
        "TRACE contained the synthetic secret"
    );
    assert!(
        !log.contains(&format!("{:?}", secret().as_slice())),
        "TRACE contained authenticator bytes through debug formatting"
    );
    assert!(
        !log.contains(&format!("{:?}", generated_secret.as_slice()))
            && !log.contains(setup.secret.as_str()),
        "TRACE exposed a newly generated authenticator secret"
    );
    assert!(
        !log.contains("totp_code_used"),
        "obsolete code-bearing cache key still appeared"
    );
}
