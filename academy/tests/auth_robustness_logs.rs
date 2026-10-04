//! Capture synthetic JWTs in memory; never persist token values in evidence.
use std::{
    collections::HashMap,
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use academy_di::Provide;
use academy_shared_contracts::jwt::JwtService;
use academy_shared_impl::{
    jwt::{JwtServiceConfig, JwtServiceImpl},
    time::TimeServiceImpl,
};

academy_di::provider! { LogProvider { config: JwtServiceConfig, } }

fn service() -> JwtServiceImpl<TimeServiceImpl> {
    let mut provider = LogProvider {
        _cache: Default::default(),
        config: JwtServiceConfig::new("owned-synthetic-review-key", &HashMap::new()).unwrap(),
    };
    provider.provide()
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

#[test]
fn jwt_verification_must_not_log_bearer_with_application_trace_formatter() {
    let jwt = service();
    let token = jwt
        .sign(serde_json::json!({"probe": true}), Duration::from_secs(60))
        .unwrap();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(Arc::clone(&bytes));
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        jwt.verify::<serde_json::Value>(&token).unwrap();
        jwt.verify_with_key::<serde_json::Value>("auth", &token)
            .unwrap();
    });
    let logged = String::from_utf8(bytes.lock().unwrap().clone())
        .unwrap()
        .contains(&token);
    assert!(
        !logged,
        "raw bearer JWT present with application-style TRACE formatting"
    );
}

#[test]
fn jwt_signing_must_not_log_token_in_trace_return() {
    let jwt = service();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let writer = Capture(Arc::clone(&bytes));
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let token = tracing::subscriber::with_default(subscriber, || {
        jwt.sign_with_key(
            "shop",
            serde_json::json!({"aud": "shop"}),
            Duration::from_secs(60),
        )
        .unwrap()
    });
    let logged = String::from_utf8(bytes.lock().unwrap().clone())
        .unwrap()
        .contains(&token);
    assert!(!logged, "raw service JWT present in TRACE return event");
}

#[test]
fn internal_key_separation_and_expiry_are_enforced() {
    let keys = HashMap::from([
        ("auth".into(), "owned-auth-key".into()),
        ("shop".into(), "owned-shop-key".into()),
    ]);
    let mut provider = LogProvider {
        _cache: Default::default(),
        config: JwtServiceConfig::new("owned-user-key", &keys).unwrap(),
    };
    let jwt: JwtServiceImpl<TimeServiceImpl> = provider.provide();
    let token = jwt
        .sign_with_key(
            "shop",
            serde_json::json!({"aud":"shop"}),
            Duration::from_secs(60),
        )
        .unwrap();
    assert!(
        jwt.verify_with_key::<serde_json::Value>("shop", &token)
            .is_ok()
    );
    assert!(
        jwt.verify_with_key::<serde_json::Value>("auth", &token)
            .is_err()
    );
    assert!(jwt.verify::<serde_json::Value>(&token).is_err());
    let expired = jwt
        .sign_with_key("auth", serde_json::json!({"aud":"auth"}), Duration::ZERO)
        .unwrap();
    assert!(
        jwt.verify_with_key::<serde_json::Value>("auth", &expired)
            .is_err()
    );
}

#[test]
fn credential_debug_never_visits_the_payload_in_any_build() {
    use academy_models::{
        Sensitive, VerificationCode,
        auth::{AccessToken, InternalToken, RefreshToken},
        mfa::MfaRecoveryCode,
        oauth2::{OAuth2AuthorizationUrl, OAuth2ProviderClientSecret, OAuth2State},
        user::UserPassword,
    };

    struct ForbiddenDebug;
    impl std::fmt::Debug for ForbiddenDebug {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            panic!("a sensitive formatter visited its payload");
        }
    }
    assert!(format!("{:?}", Sensitive(ForbiddenDebug)).contains("redacted"));
    let secret = "owned-credential-debug-probe";
    let values = [
        format!("{:?}", Sensitive(secret)),
        format!("{:?}", AccessToken::new(secret)),
        format!("{:?}", RefreshToken::new(secret)),
        format!("{:?}", InternalToken::new(secret)),
        format!("{:?}", UserPassword::try_new(secret).unwrap()),
        format!("{:?}", OAuth2ProviderClientSecret::new(secret)),
    ];
    for value in values {
        assert!(
            !value.contains(secret),
            "credential Debug exposed its payload"
        );
    }
    for code in [
        format!(
            "{:?}",
            MfaRecoveryCode::try_new("ABCDEF-GHIJKL-MNOPQR-STUVWX").unwrap()
        ),
        format!(
            "{:?}",
            VerificationCode::try_new("ABCD-EFGH-IJKL-MNOP").unwrap()
        ),
    ] {
        assert!(code.contains("redacted"), "code Debug exposed its payload");
    }
    let nonce = "a".repeat(OAuth2State::LEN);
    let url = OAuth2AuthorizationUrl {
        state: nonce.clone().try_into().unwrap(),
        authorize_url: format!("https://example.com/?state={secret}")
            .parse()
            .unwrap(),
    };
    assert!(
        !format!("{url:?}").contains(secret),
        "OAuth URL exposed its nonce"
    );
    assert!(!format!("{url:?}").contains(&nonce));
}

fn secret_trace(bytes: &Arc<Mutex<Vec<u8>>>) -> impl tracing::Subscriber + Send + Sync + 'static {
    let writer = Capture(Arc::clone(bytes));
    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::TRACE)
        .with_span_events(
            tracing_subscriber::fmt::format::FmtSpan::NEW
                | tracing_subscriber::fmt::format::FmtSpan::CLOSE,
        )
        .with_writer(move || writer.clone())
        .finish()
}

#[test]
fn recovery_and_verification_generation_do_not_log_credentials() {
    use academy_shared_contracts::secret::SecretService;
    use academy_shared_impl::secret::SecretServiceImpl;

    let bytes = Arc::new(Mutex::new(Vec::new()));
    let (recovery, verification, random) =
        tracing::subscriber::with_default(secret_trace(&bytes), || {
            (
                SecretServiceImpl.generate_mfa_recovery_code(),
                SecretServiceImpl.generate_verification_code(),
                SecretServiceImpl.generate(48),
            )
        });
    let log = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("generate_mfa_recovery_code") && log.contains("generate_verification_code"),
        "positive generation TRACE controls missing"
    );
    for value in [recovery.as_str(), verification.as_str(), random.0.as_str()] {
        assert!(
            !log.contains(value),
            "a generated credential appeared in TRACE; value omitted"
        );
    }
}

#[tokio::test]
async fn password_and_actual_argon2_verifier_do_not_appear_in_trace() {
    use academy_shared_contracts::password::{PasswordService, PasswordVerifyError};
    use academy_shared_impl::password::PasswordServiceImpl;
    use tracing::instrument::WithSubscriber;

    let bytes = Arc::new(Mutex::new(Vec::new()));
    let password = "owned-password-trace-probe";
    let wrong = "owned-wrong-password-probe";
    let hash = async {
        let service = PasswordServiceImpl::default();
        let hash = service.hash(password.to_owned().into()).await.unwrap();
        service
            .verify(password.to_owned().into(), hash.clone())
            .await
            .unwrap();
        assert!(matches!(
            service.verify(wrong.to_owned().into(), hash.clone()).await,
            Err(PasswordVerifyError::InvalidPassword)
        ));
        hash
    }
    .with_subscriber(secret_trace(&bytes))
    .await;
    assert!(hash.starts_with("$argon2id$"));
    let log = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    assert!(
        log.contains("academy_shared_impl::password")
            && log.contains("hash")
            && log.contains("verify"),
        "positive password TRACE controls missing"
    );
    for value in [password, wrong, hash.as_str()] {
        assert!(
            !log.contains(value),
            "a password or its actual verifier appeared in TRACE; value omitted"
        );
    }
}
