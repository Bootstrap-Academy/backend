use std::time::Duration;

use academy_cache_contracts::CacheService;
use academy_core_user_contracts::email_limit::{
    UserEmailCodeKind, UserEmailLimitError, UserEmailLimitService,
};
use academy_di::Build;
use academy_models::{email_address::EmailAddress, user::UserId};
use academy_shared_contracts::{hash::HashService, time::TimeService};
use academy_utils::trace_instrument;
use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::UserFeatureConfig;

#[derive(Debug, Clone, Build)]
#[cfg_attr(test, derive(Default))]
pub struct UserEmailLimitServiceImpl<Time, Hash, Cache> {
    time: Time,
    hash: Hash,
    cache: Cache,
    config: UserFeatureConfig,
}

/// The emails with a code counted in one bucket of the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct SentCodes {
    /// Number of emails counted in the current window.
    count: u64,
    /// End of the current window. A window starts with the first email
    /// counted in it and is not extended by the ones that follow.
    window_ends: DateTime<Utc>,
    /// Time of the last email counted.
    last_sent: DateTime<Utc>,
}

impl<Time, Hash, Cache> UserEmailLimitService for UserEmailLimitServiceImpl<Time, Hash, Cache>
where
    Time: TimeService,
    Hash: HashService,
    Cache: CacheService,
{
    // The address does not belong in the logs.
    #[trace_instrument(skip(self, email))]
    async fn reserve_address(
        &self,
        kind: UserEmailCodeKind,
        email: &EmailAddress,
    ) -> Result<(), UserEmailLimitError> {
        let key = format!(
            "email_code_limit:{}:address:{}",
            kind_key(kind),
            hex::encode(self.hash.sha256(&mailbox(email)).0)
        );
        self.reserve(
            &key,
            self.config.code_email_cooldown,
            self.config.code_emails_per_address,
        )
        .await
    }

    #[trace_instrument(skip(self))]
    async fn reserve_account(
        &self,
        kind: UserEmailCodeKind,
        user_id: UserId,
    ) -> Result<(), UserEmailLimitError> {
        let key = format!(
            "email_code_limit:{}:account:{}",
            kind_key(kind),
            user_id.hyphenated()
        );
        // The cooldown belongs to the address, so an account that corrects a
        // mistyped address gets a code to the corrected one at once.
        self.reserve(&key, Duration::ZERO, self.config.code_emails_per_account)
            .await
    }
}

impl<Time, Hash, Cache> UserEmailLimitServiceImpl<Time, Hash, Cache>
where
    Time: TimeService,
    Cache: CacheService,
{
    /// Count an email in the bucket under `key`, unless the last one was
    /// counted less than `cooldown` ago or the bucket already holds `budget`
    /// emails in its window.
    async fn reserve(
        &self,
        key: &str,
        cooldown: Duration,
        budget: u64,
    ) -> Result<(), UserEmailLimitError> {
        loop {
            let now = self.time.now();
            let previous = self
                .cache
                .get::<SentCodes>(key)
                .await
                .context("Failed to get the email limit from cache")?;

            if let Some(retry_after) = refusal(previous.as_ref(), now, cooldown, budget) {
                trace!("email with a code refused");
                return Err(UserEmailLimitError::Limited(retry_after));
            }

            let next = match previous {
                Some(previous) if now < previous.window_ends => SentCodes {
                    count: previous.count.saturating_add(1),
                    last_sent: now,
                    ..previous
                },
                _ => SentCodes {
                    count: 1,
                    window_ends: after(now, self.config.code_email_window),
                    last_sent: now,
                },
            };

            // The bucket has to outlive both its window and the cooldown of
            // the email just counted.
            let ttl = (next.window_ends.max(after(now, cooldown)) - now)
                .to_std()
                .unwrap_or_default()
                .max(Duration::from_secs(1));

            // Only a request whose view of the bucket is still current writes
            // it; a request that lost the race looks at the bucket again.
            if self
                .cache
                .compare_and_set(key, &previous, &next, ttl)
                .await
                .context("Failed to update the email limit atomically")?
            {
                return Ok(());
            }
        }
    }
}

/// How long the bucket still refuses another email, if it does.
fn refusal(
    previous: Option<&SentCodes>,
    now: DateTime<Utc>,
    cooldown: Duration,
    budget: u64,
) -> Option<Duration> {
    let previous = previous?;
    let cooldown_ends = after(previous.last_sent, cooldown);
    let until = if previous.count >= budget {
        cooldown_ends.max(previous.window_ends)
    } else {
        cooldown_ends
    };
    (until - now).to_std().ok().filter(|left| !left.is_zero())
}

fn after(time: DateTime<Utc>, duration: Duration) -> DateTime<Utc> {
    TimeDelta::from_std(duration)
        .ok()
        .and_then(|duration| time.checked_add_signed(duration))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

fn kind_key(kind: UserEmailCodeKind) -> &'static str {
    match kind {
        UserEmailCodeKind::Verification => "verification",
        UserEmailCodeKind::PasswordReset => "password_reset",
    }
}

/// The mailbox an address delivers to, as far as the limit is concerned.
///
/// Case does not matter, and neither does a `+tag` in the local part, which
/// most providers deliver to the mailbox without it. Gmail also ignores dots in
/// the local part and delivers `googlemail.com` to the same mailbox as
/// `gmail.com`. Otherwise every variant of one address would come with a
/// budget of its own.
fn mailbox(email: &EmailAddress) -> String {
    let user = email.0.user().to_lowercase();
    let mut user = match user.split_once('+') {
        Some((base, _)) if !base.is_empty() => base.to_owned(),
        _ => user,
    };
    let mut domain = email.0.domain().to_lowercase();
    if domain == "googlemail.com" {
        domain = "gmail.com".into();
    }
    if domain == "gmail.com" {
        user.retain(|c| c != '.');
    }
    format!("{user}@{domain}")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use academy_cache_contracts::MockCacheService;
    use academy_demo::{SHA256HASH1, SHA256HASH1_HEX, user::FOO};
    use academy_models::Sha256Hash;
    use academy_shared_contracts::{hash::MockHashService, time::MockTimeService};
    use academy_utils::assert_matches;
    use chrono::TimeZone;

    use super::*;

    type Sut = UserEmailLimitServiceImpl<MockTimeService, MockHashService, MockCacheService>;

    const MINUTE: Duration = Duration::from_secs(60);
    const HOUR: Duration = Duration::from_secs(3600);

    /// A clock the test moves forward by hand.
    #[derive(Clone)]
    struct Clock(Arc<Mutex<DateTime<Utc>>>);

    impl Clock {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(
                Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap(),
            )))
        }

        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }

        fn advance(&self, duration: Duration) {
            let mut now = self.0.lock().unwrap();
            *now = after(*now, duration);
        }
    }

    /// The service with a cache that keeps its buckets in memory and lets
    /// them expire with the test's clock, the way the real cache does.
    fn sut(clock: &Clock) -> Sut {
        let mut time = MockTimeService::new();
        let now = clock.clone();
        time.expect_now().returning(move || now.now());

        // Distinct addresses of up to 32 bytes give distinct keys; which hash
        // is used is pinned by `address_key_is_the_hash_of_the_mailbox`.
        let mut hash = MockHashService::new();
        hash.expect_sha256::<String>().returning(|data| {
            let mut hash = [0; 32];
            for (i, byte) in data.bytes().enumerate() {
                hash[i % 32] ^= byte;
            }
            Sha256Hash(hash)
        });

        let store = Arc::new(Mutex::new(
            HashMap::<String, (SentCodes, DateTime<Utc>)>::new(),
        ));
        let live = {
            let (store, clock) = (Arc::clone(&store), clock.clone());
            move |key: &str| {
                let store = store.lock().unwrap();
                store
                    .get(key)
                    .filter(|(_, expires)| clock.now() < *expires)
                    .map(|(value, _)| *value)
            }
        };
        let mut cache = MockCacheService::new();
        cache.expect_get::<SentCodes>().returning({
            let live = live.clone();
            move |key| Box::pin(std::future::ready(Ok(live(key))))
        });
        cache.expect_compare_and_set::<SentCodes>().returning({
            let clock = clock.clone();
            move |key, expected, value, ttl| {
                let updated = live(key) == *expected;
                if updated {
                    store
                        .lock()
                        .unwrap()
                        .insert(key.into(), (*value, after(clock.now(), ttl)));
                }
                Box::pin(std::future::ready(Ok(updated)))
            }
        });

        Sut {
            time,
            hash,
            cache,
            config: config(),
        }
    }

    fn config() -> UserFeatureConfig {
        UserFeatureConfig {
            code_email_cooldown: MINUTE,
            code_email_window: HOUR,
            code_emails_per_account: 5,
            code_emails_per_address: 3,
            ..Default::default()
        }
    }

    fn address(address: &str) -> EmailAddress {
        address.parse().unwrap()
    }

    fn refused(result: Result<(), UserEmailLimitError>) -> Duration {
        match result {
            Err(UserEmailLimitError::Limited(retry_after)) => retry_after,
            other => panic!("expected a refused email, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn an_address_gets_one_code_per_cooldown() {
        let clock = Clock::new();
        let sut = sut(&clock);
        let to = address("learner@example.com");

        sut.reserve_address(UserEmailCodeKind::Verification, &to)
            .await
            .unwrap();

        // a second click a few seconds later sends nothing
        clock.advance(Duration::from_secs(20));
        let retry_after = refused(
            sut.reserve_address(UserEmailCodeKind::Verification, &to)
                .await,
        );
        assert_eq!(retry_after, Duration::from_secs(40));

        // once the cooldown is over the next code goes out
        clock.advance(Duration::from_secs(40));
        sut.reserve_address(UserEmailCodeKind::Verification, &to)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn an_address_has_a_budget_per_window() {
        let clock = Clock::new();
        let sut = sut(&clock);
        let to = address("learner@example.com");

        for _ in 0..3 {
            sut.reserve_address(UserEmailCodeKind::Verification, &to)
                .await
                .unwrap();
            clock.advance(2 * MINUTE);
        }

        // the budget is used up until the window that began with the first
        // email has passed, although the cooldown is long over
        let retry_after = refused(
            sut.reserve_address(UserEmailCodeKind::Verification, &to)
                .await,
        );
        assert_eq!(retry_after, HOUR - 6 * MINUTE);

        // a refused email is not counted and does not extend the window
        clock.advance(HOUR - 6 * MINUTE);
        sut.reserve_address(UserEmailCodeKind::Verification, &to)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn the_variants_of_an_address_share_its_budget() {
        let clock = Clock::new();
        let sut = sut(&clock);

        sut.reserve_address(
            UserEmailCodeKind::Verification,
            &address("first.last@gmail.com"),
        )
        .await
        .unwrap();

        for variant in [
            "First.Last@Gmail.com",
            "first.last+1@gmail.com",
            "firstlast@gmail.com",
            "f.i.r.s.t.l.a.s.t+x@googlemail.com",
        ] {
            refused(
                sut.reserve_address(UserEmailCodeKind::Verification, &address(variant))
                    .await,
            );
        }

        // outside of Gmail a dot is part of the address
        sut.reserve_address(
            UserEmailCodeKind::Verification,
            &address("firstlast@example.com"),
        )
        .await
        .unwrap();
        refused(
            sut.reserve_address(
                UserEmailCodeKind::Verification,
                &address("firstlast+tag@example.com"),
            )
            .await,
        );
        sut.reserve_address(
            UserEmailCodeKind::Verification,
            &address("first.last@example.com"),
        )
        .await
        .unwrap();
    }

    #[test]
    fn mailbox_folds_only_what_lands_in_the_same_mailbox() {
        for (input, expected) in [
            ("Learner@Example.COM", "learner@example.com"),
            ("learner+news@example.com", "learner@example.com"),
            ("first.last@example.com", "first.last@example.com"),
            ("first.last+a+b@gmail.com", "firstlast@gmail.com"),
            ("First.Last@GoogleMail.com", "firstlast@gmail.com"),
            // an address that is nothing but a tag keeps it
            ("+tag@example.com", "+tag@example.com"),
        ] {
            assert_eq!(mailbox(&address(input)), expected, "{input}");
        }
    }

    #[tokio::test]
    async fn an_account_has_a_budget_but_no_cooldown() {
        let clock = Clock::new();
        let sut = sut(&clock);

        // five codes in quick succession, one per corrected address
        for _ in 0..5 {
            sut.reserve_account(UserEmailCodeKind::Verification, FOO.user.id)
                .await
                .unwrap();
            clock.advance(Duration::from_secs(5));
        }

        let retry_after = refused(
            sut.reserve_account(UserEmailCodeKind::Verification, FOO.user.id)
                .await,
        );
        assert_eq!(retry_after, HOUR - Duration::from_secs(25));
    }

    #[tokio::test]
    async fn each_kind_of_code_is_counted_on_its_own() {
        let clock = Clock::new();
        let sut = sut(&clock);
        let to = address("learner@example.com");

        sut.reserve_address(UserEmailCodeKind::Verification, &to)
            .await
            .unwrap();
        sut.reserve_address(UserEmailCodeKind::PasswordReset, &to)
            .await
            .unwrap();
        refused(
            sut.reserve_address(UserEmailCodeKind::PasswordReset, &to)
                .await,
        );

        for _ in 0..5 {
            sut.reserve_account(UserEmailCodeKind::Verification, FOO.user.id)
                .await
                .unwrap();
        }
        sut.reserve_account(UserEmailCodeKind::PasswordReset, FOO.user.id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn address_key_is_the_hash_of_the_mailbox() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let key = format!("email_code_limit:password_reset:address:{SHA256HASH1_HEX}");

        let sut = Sut {
            time: MockTimeService::new().with_now(now),
            hash: MockHashService::new()
                .with_sha256("firstlast@gmail.com".to_owned(), *SHA256HASH1),
            cache: MockCacheService::new()
                .with_get(key.clone(), None::<SentCodes>)
                .with_compare_and_set(
                    key,
                    None,
                    SentCodes {
                        count: 1,
                        window_ends: after(now, HOUR),
                        last_sent: now,
                    },
                    HOUR,
                    true,
                ),
            config: config(),
        };

        sut.reserve_address(
            UserEmailCodeKind::PasswordReset,
            &address("First.Last+x@googlemail.com"),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_request_that_loses_the_race_looks_again() {
        let first = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let second = after(first, Duration::from_millis(5));
        let key = format!(
            "email_code_limit:verification:account:{}",
            FOO.user.id.hyphenated()
        );
        let concurrent = SentCodes {
            count: 5,
            window_ends: after(first, HOUR),
            last_sent: first,
        };

        let mut time = MockTimeService::new();
        let mut times = [first, second].into_iter();
        time.expect_now()
            .times(2)
            .returning(move || times.next().unwrap());

        // Another request counted the last email of the budget between this
        // request's read and its write.
        let mut cache = MockCacheService::new();
        let mut reads = [None, Some(concurrent)].into_iter();
        cache
            .expect_get::<SentCodes>()
            .times(2)
            .returning(move |_| Box::pin(std::future::ready(Ok(reads.next().unwrap()))));
        cache = cache.with_compare_and_set(
            key,
            None,
            SentCodes {
                count: 1,
                window_ends: after(first, HOUR),
                last_sent: first,
            },
            HOUR,
            false,
        );

        let sut = Sut {
            time,
            cache,
            config: config(),
            ..Sut::default()
        };

        let result = sut
            .reserve_account(UserEmailCodeKind::Verification, FOO.user.id)
            .await;

        assert_matches!(result, Err(UserEmailLimitError::Limited(left)) if *left == HOUR - Duration::from_millis(5));
    }
}
