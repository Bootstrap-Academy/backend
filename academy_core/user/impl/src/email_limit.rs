use std::time::Duration;

use academy_cache_contracts::CacheService;
use academy_core_user_contracts::email_limit::{UserEmailLimitError, UserEmailLimitService};
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
    async fn reserve_verification(
        &self,
        user_id: UserId,
        email: &EmailAddress,
    ) -> Result<(), UserEmailLimitError> {
        let user_id = user_id.hyphenated();
        let mailbox = hex::encode(self.hash.sha256(&mailbox(email)).0);
        let shared = format!("email_code_limit:verification:mailbox:{mailbox}");

        // A mailbox that has used up its shared budget refuses before anything
        // is counted, so the owner of an address that others have filled keeps
        // its own budget for the time after. Only a request that races another
        // one to the last place in the shared budget can still be refused by
        // the shared bucket after counting in the requester's buckets.
        if let Some(retry_after) = self
            .refused(&shared, Duration::ZERO, self.config.code_emails_per_mailbox)
            .await?
        {
            return Err(UserEmailLimitError::Limited(retry_after));
        }

        // The buckets of the requesting account come first, so whatever a
        // refusal leaves counted belongs to the requester. A second click
        // within the cooldown is refused by the first bucket and costs the
        // account nothing. The cooldown belongs to the address, so an account
        // that corrects a mistyped address gets a code to the corrected one at
        // once.
        self.reserve(
            &format!("email_code_limit:verification:address:{user_id}:{mailbox}"),
            self.config.code_email_cooldown,
            self.config.code_emails_per_address,
        )
        .await?;
        self.reserve(
            &format!("email_code_limit:verification:account:{user_id}"),
            Duration::ZERO,
            self.config.code_emails_per_account,
        )
        .await?;
        // Only emails that are actually sent reach the shared bucket, and its
        // budget is above what one account can fill, so one account alone
        // cannot lock anybody out of the confirmation.
        self.reserve(&shared, Duration::ZERO, self.config.code_emails_per_mailbox)
            .await
    }

    // The address does not belong in the logs.
    #[trace_instrument(skip(self, email))]
    async fn reserve_password_reset_address(
        &self,
        email: &EmailAddress,
    ) -> Result<(), UserEmailLimitError> {
        // The address is counted exactly as requested. The lookup finds the
        // account case-insensitively, but the code is only sent when the
        // stored address equals the requested one exactly. Any other spelling
        // sends nothing and must not fill the bucket of the owner's address.
        let address = hex::encode(self.hash.sha256(&email.as_str().to_owned()).0);
        self.reserve(
            &format!("email_code_limit:password_reset:address:{address}"),
            self.config.code_email_cooldown,
            self.config.code_emails_per_address,
        )
        .await
    }

    #[trace_instrument(skip(self))]
    async fn reserve_password_reset_account(
        &self,
        user_id: UserId,
    ) -> Result<(), UserEmailLimitError> {
        self.reserve(
            &format!(
                "email_code_limit:password_reset:account:{}",
                user_id.hyphenated()
            ),
            Duration::ZERO,
            self.config.code_emails_per_account,
        )
        .await
    }
}

impl<Time, Hash, Cache> UserEmailLimitServiceImpl<Time, Hash, Cache>
where
    Time: TimeService,
    Cache: CacheService,
{
    /// How long the bucket under `key` still refuses another email, if it
    /// does, without counting anything.
    async fn refused(
        &self,
        key: &str,
        cooldown: Duration,
        budget: u64,
    ) -> Result<Option<Duration>, UserEmailLimitError> {
        let previous = self
            .cache
            .get::<SentCodes>(key)
            .await
            .context("Failed to get the email limit from cache")?;
        Ok(refusal(
            previous.as_ref(),
            self.time.now(),
            cooldown,
            budget,
        ))
    }

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
    // A concurrent request may have written the bucket with a time a little
    // after `now`. A bucket without a cooldown must not refuse for that.
    let cooldown_ends = (!cooldown.is_zero()).then(|| after(previous.last_sent, cooldown));
    let budget_ends = (previous.count >= budget).then_some(previous.window_ends);
    let until = cooldown_ends.max(budget_ends)?;
    (until - now).to_std().ok().filter(|left| !left.is_zero())
}

fn after(time: DateTime<Utc>, duration: Duration) -> DateTime<Utc> {
    TimeDelta::from_std(duration)
        .ok()
        .and_then(|duration| time.checked_add_signed(duration))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// The mailbox an address delivers to, as far as the verification limit is
/// concerned.
///
/// ASCII case does not matter, and neither does a `+tag` in an unquoted local
/// part, which most providers deliver to the mailbox without it. Gmail also
/// ignores dots in the local part and delivers `googlemail.com` to the same
/// mailbox as `gmail.com`. Otherwise every variant of one address would come
/// with a budget of its own. The domain is lowercased as a whole; its Unicode
/// and its punycode spelling still count apart.
fn mailbox(email: &EmailAddress) -> String {
    let user = email.0.user().to_ascii_lowercase();
    let quoted = user.starts_with('"');
    let mut user = match user.split_once('+') {
        Some((base, _)) if !quoted && !base.is_empty() => base.to_owned(),
        _ => user,
    };
    let mut domain = email.0.domain().to_lowercase();
    if domain == "googlemail.com" {
        domain = "gmail.com".into();
    }
    if domain == "gmail.com" && !quoted {
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
    use academy_demo::{
        SHA256HASH1, SHA256HASH1_HEX,
        user::{ADMIN, ADMIN2, BAR, FOO},
    };
    use academy_models::Sha256Hash;
    use academy_shared_contracts::{hash::MockHashService, time::MockTimeService};
    use academy_utils::assert_matches;
    use chrono::TimeZone;

    use super::*;

    type Sut = UserEmailLimitServiceImpl<MockTimeService, MockHashService, MockCacheService>;
    type Store = Arc<Mutex<HashMap<String, (SentCodes, DateTime<Utc>)>>>;

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

    /// Distinct addresses of up to 32 bytes give distinct keys; which hash is
    /// used is pinned by the `*_keys` tests.
    fn fake_hash(data: &str) -> Sha256Hash {
        let mut hash = [0; 32];
        for (i, byte) in data.bytes().enumerate() {
            hash[i % 32] ^= byte;
        }
        Sha256Hash(hash)
    }

    /// The service with a cache that keeps its buckets in memory and lets
    /// them expire with the test's clock, the way the real cache does.
    fn sut(clock: &Clock) -> (Sut, Store) {
        let mut time = MockTimeService::new();
        let now = clock.clone();
        time.expect_now().returning(move || now.now());

        let mut hash = MockHashService::new();
        hash.expect_sha256::<String>()
            .returning(|data| fake_hash(data));

        let store = Store::default();
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
            let (store, clock) = (Arc::clone(&store), clock.clone());
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

        let sut = Sut {
            time,
            hash,
            cache,
            config: config(),
        };
        (sut, store)
    }

    fn config() -> UserFeatureConfig {
        UserFeatureConfig {
            code_email_cooldown: MINUTE,
            code_email_window: HOUR,
            code_emails_per_account: 5,
            code_emails_per_address: 3,
            code_emails_per_mailbox: 4,
            ..Default::default()
        }
    }

    fn address(address: &str) -> EmailAddress {
        address.parse().unwrap()
    }

    /// The number of emails counted in the shared bucket of a mailbox.
    fn mailbox_count(store: &Store, mailbox: &str) -> u64 {
        let key = format!(
            "email_code_limit:verification:mailbox:{}",
            hex::encode(fake_hash(mailbox).0)
        );
        store
            .lock()
            .unwrap()
            .get(&key)
            .map_or(0, |(value, _)| value.count)
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
        let (sut, _) = sut(&clock);
        let to = address("learner@example.com");

        sut.reserve_verification(FOO.user.id, &to).await.unwrap();

        // a second click a few seconds later sends nothing
        clock.advance(Duration::from_secs(20));
        let retry_after = refused(sut.reserve_verification(FOO.user.id, &to).await);
        assert_eq!(retry_after, Duration::from_secs(40));

        // once the cooldown is over the next code goes out
        clock.advance(Duration::from_secs(40));
        sut.reserve_verification(FOO.user.id, &to).await.unwrap();
    }

    #[tokio::test]
    async fn an_address_has_a_budget_per_window() {
        let clock = Clock::new();
        let (sut, _) = sut(&clock);
        let to = address("learner@example.com");

        for _ in 0..3 {
            sut.reserve_verification(FOO.user.id, &to).await.unwrap();
            clock.advance(2 * MINUTE);
        }

        // the budget is used up until the window that began with the first
        // email has passed, although the cooldown is long over
        let retry_after = refused(sut.reserve_verification(FOO.user.id, &to).await);
        assert_eq!(retry_after, HOUR - 6 * MINUTE);

        // a refused email is not counted and does not extend the window
        clock.advance(HOUR - 6 * MINUTE);
        sut.reserve_verification(FOO.user.id, &to).await.unwrap();
    }

    #[tokio::test]
    async fn the_variants_of_an_address_share_its_budget_within_an_account() {
        let clock = Clock::new();
        let (sut, _) = sut(&clock);

        sut.reserve_verification(FOO.user.id, &address("first.last@gmail.com"))
            .await
            .unwrap();

        for variant in [
            "First.Last@Gmail.com",
            "first.last+1@gmail.com",
            "firstlast@gmail.com",
            "f.i.r.s.t.l.a.s.t+x@googlemail.com",
        ] {
            refused(
                sut.reserve_verification(FOO.user.id, &address(variant))
                    .await,
            );
        }

        // outside of Gmail a dot is part of the address
        sut.reserve_verification(FOO.user.id, &address("firstlast@example.com"))
            .await
            .unwrap();
        refused(
            sut.reserve_verification(FOO.user.id, &address("firstlast+tag@example.com"))
                .await,
        );
        sut.reserve_verification(FOO.user.id, &address("first.last@example.com"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn another_account_with_a_variant_does_not_lock_the_owner() {
        let clock = Clock::new();
        let (sut, store) = sut(&clock);

        sut.reserve_verification(BAR.user.id, &address("victim+x@example.com"))
            .await
            .unwrap();
        sut.reserve_verification(FOO.user.id, &address("victim@example.com"))
            .await
            .unwrap();

        assert_eq!(mailbox_count(&store, "victim@example.com"), 2);
    }

    #[tokio::test]
    async fn a_refusal_costs_only_the_requesters_buckets() {
        let clock = Clock::new();
        let (sut, store) = sut(&clock);

        // The attacker uses up the budget of its account on its own addresses
        // and then keeps asking for a variant of the victim's address.
        for n in 0..5 {
            sut.reserve_verification(BAR.user.id, &address(&format!("bar{n}@example.com")))
                .await
                .unwrap();
        }
        for _ in 0..3 {
            refused(
                sut.reserve_verification(BAR.user.id, &address("victim+x@example.com"))
                    .await,
            );
            clock.advance(2 * MINUTE);
        }
        assert_eq!(mailbox_count(&store, "victim@example.com"), 0);

        sut.reserve_verification(FOO.user.id, &address("victim@example.com"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn the_mailbox_has_a_budget_across_accounts() {
        let clock = Clock::new();
        let (sut, _) = sut(&clock);

        // one account sends as many codes to the mailbox as it may
        for _ in 0..3 {
            sut.reserve_verification(BAR.user.id, &address("victim+x@example.com"))
                .await
                .unwrap();
            clock.advance(2 * MINUTE);
        }
        refused(
            sut.reserve_verification(BAR.user.id, &address("victim+x@example.com"))
                .await,
        );

        // and leaves the owner of the mailbox room for its own code
        sut.reserve_verification(FOO.user.id, &address("victim@example.com"))
            .await
            .unwrap();

        // a further account finds the shared budget used up until the window
        // that began with the first email has passed
        let retry_after = refused(
            sut.reserve_verification(ADMIN.user.id, &address("Victim+y@example.com"))
                .await,
        );
        assert_eq!(retry_after, HOUR - 6 * MINUTE);

        // once that window is over the mailbox takes codes again
        clock.advance(retry_after);
        sut.reserve_verification(ADMIN2.user.id, &address("victim@example.com"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn an_account_has_a_budget_but_no_cooldown() {
        let clock = Clock::new();
        let (sut, _) = sut(&clock);

        // five codes in quick succession, one per corrected address
        for n in 0..5 {
            sut.reserve_verification(FOO.user.id, &address(&format!("typo{n}@example.com")))
                .await
                .unwrap();
            clock.advance(Duration::from_secs(5));
        }

        let retry_after = refused(
            sut.reserve_verification(FOO.user.id, &address("typo5@example.com"))
                .await,
        );
        assert_eq!(retry_after, HOUR - Duration::from_secs(25));
    }

    #[tokio::test]
    async fn a_password_reset_counts_the_address_exactly_as_requested() {
        let clock = Clock::new();
        let (sut, _) = sut(&clock);

        // No other spelling reaches the owner's code, so none of them touches
        // the owner's bucket: neither a tag, dots or the other Gmail domain,
        // nor other case or a Kelvin sign that Unicode lowercases to `k`.
        for variant in [
            "kowner+x@gmail.com",
            "k.owner@gmail.com",
            "kowner@googlemail.com",
            "KOwner@Gmail.COM",
            "\u{212A}owner@gmail.com",
        ] {
            sut.reserve_password_reset_address(&address(variant))
                .await
                .unwrap();
        }
        sut.reserve_password_reset_address(&address("kowner@gmail.com"))
            .await
            .unwrap();

        // the same spelling again is the same address
        refused(
            sut.reserve_password_reset_address(&address("kowner@gmail.com"))
                .await,
        );
    }

    #[tokio::test]
    async fn a_full_mailbox_refuses_before_counting_anything() {
        let clock = Clock::new();
        let (sut, store) = sut(&clock);

        // Two other accounts fill the shared budget of the mailbox.
        for _ in 0..3 {
            sut.reserve_verification(BAR.user.id, &address("victim+x@example.com"))
                .await
                .unwrap();
            clock.advance(2 * MINUTE);
        }
        sut.reserve_verification(ADMIN.user.id, &address("victim+y@example.com"))
            .await
            .unwrap();

        // The owner is refused until the shared window is over, and its own
        // buckets stay untouched for the time after.
        let owner = FOO.user.id.hyphenated().to_string();
        for _ in 0..5 {
            let retry_after = refused(
                sut.reserve_verification(FOO.user.id, &address("victim@example.com"))
                    .await,
            );
            assert_eq!(retry_after, HOUR - 6 * MINUTE);
        }
        assert!(!store.lock().unwrap().keys().any(|key| key.contains(&owner)));

        clock.advance(HOUR - 6 * MINUTE);
        sut.reserve_verification(FOO.user.id, &address("victim@example.com"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_bucket_without_cooldown_ignores_a_write_a_little_ahead() {
        let clock = Clock::new();
        let (sut, store) = sut(&clock);

        // A concurrent request wrote the shared bucket and the account bucket
        // with a time a few milliseconds after this request's clock.
        let ahead = SentCodes {
            count: 1,
            window_ends: after(clock.now(), HOUR),
            last_sent: after(clock.now(), Duration::from_millis(5)),
        };
        for key in [
            format!(
                "email_code_limit:verification:mailbox:{}",
                hex::encode(fake_hash("victim@example.com").0)
            ),
            format!(
                "email_code_limit:password_reset:account:{}",
                FOO.user.id.hyphenated()
            ),
        ] {
            store
                .lock()
                .unwrap()
                .insert(key, (ahead, after(clock.now(), HOUR)));
        }

        sut.reserve_verification(FOO.user.id, &address("victim@example.com"))
            .await
            .unwrap();
        sut.reserve_password_reset_account(FOO.user.id)
            .await
            .unwrap();
        assert_eq!(mailbox_count(&store, "victim@example.com"), 2);
    }

    #[tokio::test]
    async fn each_kind_of_code_is_counted_on_its_own() {
        let clock = Clock::new();
        let (sut, _) = sut(&clock);
        let to = address("learner@example.com");

        sut.reserve_verification(FOO.user.id, &to).await.unwrap();
        sut.reserve_password_reset_address(&to).await.unwrap();
        refused(sut.reserve_password_reset_address(&to).await);

        for n in 1..5 {
            sut.reserve_verification(FOO.user.id, &address(&format!("l{n}@example.com")))
                .await
                .unwrap();
        }
        refused(
            sut.reserve_verification(FOO.user.id, &address("l5@example.com"))
                .await,
        );
        sut.reserve_password_reset_account(FOO.user.id)
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
            // a `+` inside quotes is part of the local part
            ("\"A+b\"@example.com", "\"a+b\"@example.com"),
            // only ASCII letters are folded in the local part
            ("\u{212A}evin@example.com", "\u{212A}evin@example.com"),
            ("a@B\u{dc}CHER.example", "a@b\u{fc}cher.example"),
        ] {
            assert_eq!(mailbox(&address(input)), expected, "{input}");
        }
    }

    #[tokio::test]
    async fn verification_keys() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let user_id = FOO.user.id.hyphenated();
        let first = SentCodes {
            count: 1,
            window_ends: after(now, HOUR),
            last_sent: now,
        };

        let shared = format!("email_code_limit:verification:mailbox:{SHA256HASH1_HEX}");
        let mut cache = MockCacheService::new();
        // The shared bucket is looked at once before anything is counted.
        let peek = shared.clone();
        cache
            .expect_get::<SentCodes>()
            .once()
            .withf(move |key| key == peek)
            .return_once(|_| Box::pin(std::future::ready(Ok(None))));
        for key in [
            format!("email_code_limit:verification:address:{user_id}:{SHA256HASH1_HEX}"),
            format!("email_code_limit:verification:account:{user_id}"),
            shared,
        ] {
            cache = cache
                .with_get(key.clone(), None::<SentCodes>)
                .with_compare_and_set(key, None, first, HOUR, true);
        }
        let mut time = MockTimeService::new();
        time.expect_now().times(4).return_const(now);

        let sut = Sut {
            time,
            hash: MockHashService::new()
                .with_sha256("firstlast@gmail.com".to_owned(), *SHA256HASH1),
            cache,
            config: config(),
        };

        sut.reserve_verification(FOO.user.id, &address("First.Last+x@googlemail.com"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn password_reset_address_key() {
        let now = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let key = format!("email_code_limit:password_reset:address:{SHA256HASH1_HEX}");

        let sut = Sut {
            time: MockTimeService::new().with_now(now),
            hash: MockHashService::new()
                .with_sha256("First.Last+x@GoogleMail.com".to_owned(), *SHA256HASH1),
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

        sut.reserve_password_reset_address(&address("First.Last+x@GoogleMail.com"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_request_that_loses_the_race_looks_again() {
        let first = Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap();
        let second = after(first, Duration::from_millis(5));
        let key = format!(
            "email_code_limit:password_reset:account:{}",
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

        let result = sut.reserve_password_reset_account(FOO.user.id).await;

        assert_matches!(result, Err(UserEmailLimitError::Limited(left)) if *left == HOUR - Duration::from_millis(5));
    }
}
