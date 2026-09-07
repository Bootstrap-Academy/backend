use std::{net::IpAddr, time::Duration};

use academy_cache_contracts::CacheService;
use academy_core_session_contracts::login_throttle::{
    SessionLoginThrottleError, SessionLoginThrottleService,
};
use academy_di::Build;
use academy_models::user::UserNameOrEmailAddress;
use academy_shared_contracts::{hash::HashService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use tracing::trace;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct SessionLoginThrottleServiceImpl<Time, Hash, Cache> {
    time: Time,
    hash: Hash,
    cache: Cache,
    config: SessionLoginThrottleConfig,
}

#[derive(Debug, Clone)]
pub struct SessionLoginThrottleConfig {
    /// Number of failed attempts against one login after which it is locked.
    pub fails_before_lock: u64,
    /// Lifetime of the counter of failed attempts against one login.
    pub fail_window: Duration,
    /// Length of the first lock. Every following one is twice as long as the
    /// one before it.
    pub lock_initial: Duration,
    /// Longest lock a login can receive.
    pub lock_max: Duration,
    /// Number of failed attempts from one client address after which further
    /// attempts from it are refused.
    ///
    /// An address is shared by everybody behind the same NAT, so this budget is
    /// deliberately far larger than the per login one.
    pub fails_per_ip: u64,
    /// Lifetime of the counter of failed attempts from one client address, and
    /// the length of the block that follows it.
    pub ip_window: Duration,
}

/// Failed attempts counted in one bucket of the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct FailedAttempts {
    /// Number of failed attempts counted so far.
    count: u64,
    /// Point in time until which further attempts are refused, if the bucket
    /// is over its budget.
    #[serde(default)]
    blocked_until: Option<DateTime<Utc>>,
}

impl<Time, Hash, Cache> SessionLoginThrottleService
    for SessionLoginThrottleServiceImpl<Time, Hash, Cache>
where
    Time: TimeService,
    Hash: HashService,
    Cache: CacheService,
{
    // The login identifier does not belong in the logs of every failed attempt.
    #[trace_instrument(skip(self, name_or_email))]
    async fn check(
        &self,
        name_or_email: &UserNameOrEmailAddress,
        client_ip: IpAddr,
    ) -> Result<(), SessionLoginThrottleError> {
        let now = self.time.now();

        // The address is only looked at once the login itself is in the clear,
        // so a locked login costs a single cache read.
        if let Some(retry_after) = self
            .blocked_for(&self.account_key(name_or_email), now)
            .await?
        {
            trace!("login attempt refused");
            return Err(SessionLoginThrottleError::TooManyFailedAttempts(
                retry_after,
            ));
        }

        if let Some(retry_after) = self.blocked_for(&self.ip_key(client_ip), now).await? {
            trace!("login attempt refused");
            return Err(SessionLoginThrottleError::TooManyFailedAttempts(
                retry_after,
            ));
        }

        Ok(())
    }

    #[trace_instrument(skip(self, name_or_email))]
    async fn record_account_failure(
        &self,
        name_or_email: &UserNameOrEmailAddress,
    ) -> anyhow::Result<()> {
        let now = self.time.now();
        let key = self.account_key(name_or_email);
        let count = self.get(&key).await?.map_or(0, |a| a.count) + 1;

        // The first `fails_before_lock` attempts pass unhindered; each one after
        // that is answered with a lock twice as long as the previous one, up to
        // `lock_max`.
        let blocked_until = (count >= self.config.fails_before_lock).then(|| {
            let steps = count - self.config.fails_before_lock;
            let lock = self
                .config
                .lock_initial
                .saturating_mul(2u32.saturating_pow(steps.try_into().unwrap_or(u32::MAX)))
                .min(self.config.lock_max);
            now + TimeDelta::from_std(lock).unwrap_or(TimeDelta::MAX)
        });

        let ttl = self.ttl(now, self.config.fail_window, blocked_until);
        self.set(
            &key,
            FailedAttempts {
                count,
                blocked_until,
            },
            ttl,
        )
        .await
    }

    #[trace_instrument(skip(self))]
    async fn record_ip_failure(&self, client_ip: IpAddr) -> anyhow::Result<()> {
        let now = self.time.now();
        let key = self.ip_key(client_ip);
        let count = self.get(&key).await?.map_or(0, |a| a.count) + 1;

        // Unlike a single login, an address is not locked for longer and longer:
        // it is shared by everybody behind the same NAT, so it only waits out
        // one window.
        let blocked_until = (count >= self.config.fails_per_ip)
            .then(|| now + TimeDelta::from_std(self.config.ip_window).unwrap_or(TimeDelta::MAX));

        let ttl = self.ttl(now, self.config.ip_window, blocked_until);
        self.set(
            &key,
            FailedAttempts {
                count,
                blocked_until,
            },
            ttl,
        )
        .await
    }

    #[trace_instrument(skip(self, name_or_email))]
    async fn reset(&self, name_or_email: &UserNameOrEmailAddress) -> anyhow::Result<()> {
        self.cache
            .remove(&self.account_key(name_or_email))
            .await
            .context("Failed to reset failed login attempts in cache")
    }
}

impl<Time, Hash, Cache> SessionLoginThrottleServiceImpl<Time, Hash, Cache>
where
    Hash: HashService,
    Cache: CacheService,
{
    /// Return how long the given bucket is still blocked for, if it is.
    async fn blocked_for(&self, key: &str, now: DateTime<Utc>) -> anyhow::Result<Option<Duration>> {
        Ok(self
            .get(key)
            .await?
            .and_then(|attempts| attempts.blocked_until)
            .and_then(|blocked_until| (blocked_until - now).to_std().ok()))
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<FailedAttempts>> {
        self.cache
            .get(key)
            .await
            .context("Failed to get failed login attempts from cache")
    }

    async fn set(&self, key: &str, attempts: FailedAttempts, ttl: Duration) -> anyhow::Result<()> {
        self.cache
            .set(key, &attempts, Some(ttl))
            .await
            .context("Failed to save failed login attempts in cache")
    }

    /// Keep a bucket at least until its block has passed, so a lock cannot be
    /// shaken off by letting the counter expire.
    fn ttl(
        &self,
        now: DateTime<Utc>,
        window: Duration,
        blocked_until: Option<DateTime<Utc>>,
    ) -> Duration {
        blocked_until
            .and_then(|blocked_until| (blocked_until - now).to_std().ok())
            .unwrap_or_default()
            .max(window)
    }

    fn account_key(&self, name_or_email: &UserNameOrEmailAddress) -> String {
        let value = match name_or_email {
            UserNameOrEmailAddress::Name(name) => name,
            UserNameOrEmailAddress::Email(email) => email.as_str(),
        }
        .to_lowercase();
        format!("login_throttle_account:{}", self.hash_hex(&value))
    }

    fn ip_key(&self, client_ip: IpAddr) -> String {
        format!(
            "login_throttle_ip:{}",
            self.hash_hex(&client_ip.to_string())
        )
    }

    fn hash_hex(&self, value: &str) -> String {
        hex::encode(self.hash.sha256(&value.to_owned()).0)
    }
}

impl<Time, Hash, Cache> SessionLoginThrottleServiceImpl<Time, Hash, Cache> {
    pub fn new(time: Time, hash: Hash, cache: Cache, config: SessionLoginThrottleConfig) -> Self {
        Self {
            time,
            hash,
            cache,
            config,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use academy_cache_contracts::MockCacheService;
    use academy_demo::{SHA256HASH1, SHA256HASH1_HEX, user::FOO};
    use academy_shared_contracts::{hash::MockHashService, time::MockTimeService};
    use academy_utils::assert_matches;
    use chrono::{TimeZone, Utc};

    use super::*;

    const CLIENT_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7));

    fn config() -> SessionLoginThrottleConfig {
        SessionLoginThrottleConfig {
            fails_before_lock: 5,
            fail_window: Duration::from_secs(15 * 60),
            lock_initial: Duration::from_secs(60),
            lock_max: Duration::from_secs(15 * 60),
            fails_per_ip: 30,
            ip_window: Duration::from_secs(15 * 60),
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 7, 12, 0, 0).unwrap()
    }

    fn login() -> UserNameOrEmailAddress {
        UserNameOrEmailAddress::Name(FOO.user.name.clone())
    }

    fn account_key() -> String {
        format!("login_throttle_account:{SHA256HASH1_HEX}")
    }

    fn ip_key() -> String {
        format!("login_throttle_ip:{SHA256HASH1_HEX}")
    }

    fn hash_login() -> MockHashService {
        MockHashService::new().with_sha256(FOO.user.name.clone().into_inner(), *SHA256HASH1)
    }

    fn hash_ip() -> MockHashService {
        MockHashService::new().with_sha256(CLIENT_IP.to_string(), *SHA256HASH1)
    }

    /// Nothing counted, nothing refused.
    #[tokio::test]
    async fn check_ok() {
        // Arrange
        let hash = MockHashService::new()
            .with_sha256(FOO.user.name.clone().into_inner(), *SHA256HASH1)
            .with_sha256(CLIENT_IP.to_string(), *SHA256HASH1);

        let cache = MockCacheService::new()
            .with_get::<FailedAttempts>(account_key(), None)
            .with_get::<FailedAttempts>(ip_key(), None);

        let sut = SessionLoginThrottleServiceImpl::new(
            MockTimeService::new().with_now(now()),
            hash,
            cache,
            config(),
        );

        // Act
        let result = sut.check(&login(), CLIENT_IP).await;

        // Assert
        result.unwrap();
    }

    /// A lock that has not run out yet refuses the attempt and says how long
    /// the caller has to wait.
    #[tokio::test]
    async fn check_locked() {
        // Arrange
        let cache = MockCacheService::new().with_get(
            account_key(),
            Some(FailedAttempts {
                count: 5,
                blocked_until: Some(now() + TimeDelta::seconds(42)),
            }),
        );

        let sut = SessionLoginThrottleServiceImpl::new(
            MockTimeService::new().with_now(now()),
            hash_login(),
            cache,
            config(),
        );

        // Act
        let result = sut.check(&login(), CLIENT_IP).await;

        // Assert
        assert_matches!(
            result,
            Err(SessionLoginThrottleError::TooManyFailedAttempts(retry_after))
                if *retry_after == Duration::from_secs(42)
        );
    }

    /// A lock that has run out is ignored, even though the counter is still
    /// there.
    #[tokio::test]
    async fn check_lock_expired() {
        // Arrange
        let hash = MockHashService::new()
            .with_sha256(FOO.user.name.clone().into_inner(), *SHA256HASH1)
            .with_sha256(CLIENT_IP.to_string(), *SHA256HASH1);

        let cache = MockCacheService::new()
            .with_get(
                account_key(),
                Some(FailedAttempts {
                    count: 5,
                    blocked_until: Some(now() - TimeDelta::seconds(1)),
                }),
            )
            .with_get::<FailedAttempts>(ip_key(), None);

        let sut = SessionLoginThrottleServiceImpl::new(
            MockTimeService::new().with_now(now()),
            hash,
            cache,
            config(),
        );

        // Act
        let result = sut.check(&login(), CLIENT_IP).await;

        // Assert
        result.unwrap();
    }

    /// The address is checked as well, so a machine working through a list of
    /// accounts is stopped even though no single account is locked.
    #[tokio::test]
    async fn check_ip_blocked() {
        // Arrange
        let hash = MockHashService::new()
            .with_sha256(FOO.user.name.clone().into_inner(), *SHA256HASH1)
            .with_sha256(CLIENT_IP.to_string(), *SHA256HASH1);

        let cache = MockCacheService::new()
            .with_get::<FailedAttempts>(account_key(), None)
            .with_get(
                ip_key(),
                Some(FailedAttempts {
                    count: 30,
                    blocked_until: Some(now() + TimeDelta::seconds(600)),
                }),
            );

        let sut = SessionLoginThrottleServiceImpl::new(
            MockTimeService::new().with_now(now()),
            hash,
            cache,
            config(),
        );

        // Act
        let result = sut.check(&login(), CLIENT_IP).await;

        // Assert
        assert_matches!(
            result,
            Err(SessionLoginThrottleError::TooManyFailedAttempts(retry_after))
                if *retry_after == Duration::from_secs(600)
        );
    }

    /// Below the threshold a failure is only counted.
    #[tokio::test]
    async fn record_account_failure_below_threshold() {
        // Arrange
        let cache = MockCacheService::new()
            .with_get(
                account_key(),
                Some(FailedAttempts {
                    count: 3,
                    blocked_until: None,
                }),
            )
            .with_set(
                account_key(),
                FailedAttempts {
                    count: 4,
                    blocked_until: None,
                },
                Some(Duration::from_secs(15 * 60)),
            );

        let sut = SessionLoginThrottleServiceImpl::new(
            MockTimeService::new().with_now(now()),
            hash_login(),
            cache,
            config(),
        );

        // Act
        let result = sut.record_account_failure(&login()).await;

        // Assert
        result.unwrap();
    }

    /// Each lock lasts twice as long as the one before it, up to the maximum.
    #[tokio::test]
    async fn record_account_failure_lock_doubles_and_is_capped() {
        for (count_before, lock_seconds) in [
            (4, 60),
            (5, 120),
            (6, 240),
            (7, 480),
            (8, 900),
            (9, 900),
            (100, 900),
        ] {
            // Arrange
            let cache = MockCacheService::new()
                .with_get(
                    account_key(),
                    Some(FailedAttempts {
                        count: count_before,
                        blocked_until: None,
                    }),
                )
                .with_set(
                    account_key(),
                    FailedAttempts {
                        count: count_before + 1,
                        blocked_until: Some(now() + TimeDelta::seconds(lock_seconds)),
                    },
                    // The counter outlives the lock it caused.
                    Some(Duration::from_secs(15 * 60)),
                );

            let sut = SessionLoginThrottleServiceImpl::new(
                MockTimeService::new().with_now(now()),
                hash_login(),
                cache,
                config(),
            );

            // Act
            let result = sut.record_account_failure(&login()).await;

            // Assert
            result.unwrap();
        }
    }

    /// The address is blocked for exactly one window, never longer.
    #[tokio::test]
    async fn record_ip_failure_blocks_for_one_window() {
        // Arrange
        let cache = MockCacheService::new()
            .with_get(
                ip_key(),
                Some(FailedAttempts {
                    count: 29,
                    blocked_until: None,
                }),
            )
            .with_set(
                ip_key(),
                FailedAttempts {
                    count: 30,
                    blocked_until: Some(now() + TimeDelta::seconds(15 * 60)),
                },
                Some(Duration::from_secs(15 * 60)),
            );

        let sut = SessionLoginThrottleServiceImpl::new(
            MockTimeService::new().with_now(now()),
            hash_ip(),
            cache,
            config(),
        );

        // Act
        let result = sut.record_ip_failure(CLIENT_IP).await;

        // Assert
        result.unwrap();
    }

    /// A successful login drops the counter of that login.
    #[tokio::test]
    async fn reset() {
        // Arrange
        let cache = MockCacheService::new().with_remove(account_key());

        let sut = SessionLoginThrottleServiceImpl::new(
            MockTimeService::new(),
            hash_login(),
            cache,
            config(),
        );

        // Act
        let result = sut.reset(&login()).await;

        // Assert
        result.unwrap();
    }
}
