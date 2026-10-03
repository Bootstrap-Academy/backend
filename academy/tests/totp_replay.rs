//! Real Valkey regression for simultaneous use of the same second factor.
//! Runs alongside the cache and login-throttle tests in the Valkey CI job.

use academy_cache_contracts::CacheService;
use academy_di::{Provide, provider};
use academy_models::mfa::TotpSecret;
use academy_shared_contracts::{hash::HashService, time::TimeService, totp::TotpService};
use academy_shared_impl::{
    hash::HashServiceImpl,
    secret::SecretServiceImpl,
    totp::{TotpServiceConfig, TotpServiceImpl},
};
use chrono::{DateTime, Utc};
use futures::future::join_all;

#[derive(Clone)]
struct FixedTime;

impl TimeService for FixedTime {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(1724949831, 0).unwrap()
    }
}

provider! {
    Provider {
        time: FixedTime,
        cache: academy_cache_valkey::ValkeyCache,
        config: TotpServiceConfig,
    }
}

#[tokio::test]
async fn a_second_factor_can_only_authorize_one_concurrent_request() {
    let config = academy_config::load().unwrap();
    let cache = academy::cache::connect(&config.cache).await.unwrap();
    let mut provider = Provider {
        _cache: Default::default(),
        time: FixedTime,
        cache: cache.clone(),
        config: TotpServiceConfig {
            secret_length: 24.try_into().unwrap(),
        },
    };
    let totp: TotpServiceImpl<
        SecretServiceImpl,
        FixedTime,
        HashServiceImpl,
        academy_cache_valkey::ValkeyCache,
    > = provider.provide();
    let secret = TotpSecret::try_new(b"XSSYkVp8pDsOnT1jB5eN0CB8".to_vec()).unwrap();
    let code: academy_models::mfa::TotpCode = "960546".try_into().unwrap();
    let hash = HashServiceImpl.sha256(&*secret);
    let key = format!("totp_code_used:{}:{}", hex::encode(hash.0), *code);
    cache.remove(&key).await.unwrap();

    let results = join_all((0..32).map(|_| totp.check(&code, secret.clone()))).await;
    let successes = results.iter().filter(|result| result.is_ok()).count();
    cache.remove(&key).await.unwrap();
    assert_eq!(successes, 1, "one code authorized {successes} requests");
    assert!(
        results
            .iter()
            .filter(|result| result.is_err())
            .all(|result| {
                matches!(
                    result,
                    Err(academy_shared_contracts::totp::TotpCheckError::RecentlyUsed)
                )
            })
    );
}
