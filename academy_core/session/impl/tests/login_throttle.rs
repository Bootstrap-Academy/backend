//! The login throttle against a real Valkey.
//!
//! The unit tests in `src/login_throttle.rs` pin the arithmetic against a mock
//! cache; this one proves that the buckets survive a round trip through Valkey,
//! that a lock is seen by a second call and that it disappears on its own once
//! its time is up.

use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};

use academy_cache_contracts::CacheService;
use academy_cache_valkey::{ValkeyCache, ValkeyCacheConfig};
use academy_core_session_contracts::login_throttle::{
    SessionLoginThrottleError, SessionLoginThrottleService,
};
use academy_core_session_impl::login_throttle::{
    SessionLoginThrottleConfig, SessionLoginThrottleServiceImpl,
};
use academy_models::user::UserNameOrEmailAddress;
use academy_shared_impl::{hash::HashServiceImpl, time::TimeServiceImpl};

const CLIENT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));
const OTHER_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 42));

type Sut = SessionLoginThrottleServiceImpl<TimeServiceImpl, HashServiceImpl, ValkeyCache>;

#[tokio::test]
async fn account_is_locked_and_unlocks_itself() {
    let sut = setup().await;
    let login = login("locked-account");

    // two attempts may fail without consequence
    for _ in 0..2 {
        sut.check(&login, CLIENT_IP).await.unwrap();
        sut.record_account_failure(&login).await.unwrap();
    }
    sut.check(&login, CLIENT_IP).await.unwrap();

    // the third one locks the login
    sut.record_account_failure(&login).await.unwrap();
    let retry_after = assert_locked(sut.check(&login, CLIENT_IP).await);
    assert!(
        retry_after <= Duration::from_secs(1),
        "the first lock is the initial one: {retry_after:?}"
    );

    // a fourth failure while it is locked doubles the lock
    sut.record_account_failure(&login).await.unwrap();
    let retry_after = assert_locked(sut.check(&login, CLIENT_IP).await);
    assert!(
        retry_after > Duration::from_secs(1),
        "the second lock is longer than the first: {retry_after:?}"
    );

    // and it ends on its own
    tokio::time::sleep(Duration::from_millis(2100)).await;
    sut.check(&login, CLIENT_IP).await.unwrap();
}

#[tokio::test]
async fn the_lock_follows_the_account_not_the_spelling() {
    let sut = setup().await;
    let name = login("dieter");
    let email = UserNameOrEmailAddress::Email("dieter@example.com".parse().unwrap());

    for _ in 0..3 {
        sut.record_account_failure(&name).await.unwrap();
        sut.record_account_failure(&email).await.unwrap();
    }

    // both spellings are locked, because both were counted
    assert_locked(sut.check(&name, CLIENT_IP).await);
    assert_locked(sut.check(&email, CLIENT_IP).await);

    // a successful login clears the login it was made with
    sut.reset(&name).await.unwrap();
    sut.check(&name, CLIENT_IP).await.unwrap();
}

#[tokio::test]
async fn the_address_is_blocked_on_its_own() {
    let sut = setup().await;

    // five failures against five different logins do not lock any of them
    for i in 0..5 {
        let login = login(&format!("victim-{i}"));
        sut.record_account_failure(&login).await.unwrap();
        sut.record_ip_failure(CLIENT_IP).await.unwrap();
        sut.check(&login, OTHER_IP).await.unwrap();
    }

    // but the address they came from has used up its budget
    assert_locked(sut.check(&login("victim-0"), CLIENT_IP).await);

    // another address is unaffected: nobody is locked out by a neighbour
    sut.check(&login("victim-0"), OTHER_IP).await.unwrap();
}

fn login(name: &str) -> UserNameOrEmailAddress {
    UserNameOrEmailAddress::Name(name.try_into().unwrap())
}

fn assert_locked(result: Result<(), SessionLoginThrottleError>) -> Duration {
    match result {
        Err(SessionLoginThrottleError::TooManyFailedAttempts(retry_after)) => retry_after,
        other => panic!("expected a refused attempt, got {other:?}"),
    }
}

async fn setup() -> Sut {
    let config = academy_config::load().unwrap();

    let cache = ValkeyCache::connect(&ValkeyCacheConfig {
        url: config.cache.url,
        max_connections: config.cache.max_connections,
        min_connections: config.cache.min_connections,
        acquire_timeout: config.cache.acquire_timeout.into(),
        idle_timeout: config.cache.idle_timeout.map(Into::into),
        max_lifetime: config.cache.max_lifetime.map(Into::into),
    })
    .await
    .unwrap();
    cache.ping().await.unwrap();
    cache.clear().await.unwrap();

    SessionLoginThrottleServiceImpl::new(
        TimeServiceImpl,
        HashServiceImpl,
        cache,
        SessionLoginThrottleConfig {
            fails_before_lock: 3,
            fail_window: Duration::from_secs(60),
            lock_initial: Duration::from_secs(1),
            lock_max: Duration::from_secs(2),
            fails_per_ip: 5,
            ip_window: Duration::from_secs(60),
        },
    )
}
