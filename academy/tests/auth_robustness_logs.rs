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
